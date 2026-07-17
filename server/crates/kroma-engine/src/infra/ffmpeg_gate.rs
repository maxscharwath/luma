//! A process-wide cap on how many heavy ffmpeg passes run at once.
//!
//! Every CPU-heavy media source (storyboard tiles/montage/jpeg, subtitle
//! extraction, marker fingerprinting, on-demand storyboard scrubbing) draws from
//! ONE budget here instead of each subsystem sizing its own worker pool blind to
//! the others. Before this gate, a single library change fired the storyboard +
//! subtitles + markers stages concurrently, and storyboard alone fanned each of
//! its 2 workers out to ~8 tile ffmpeg processes: dozens of ffmpeg on a 2-4 core
//! NAS, pegging it. The gate collapses that to a small budget of simultaneous
//! processes so playback and the UI keep a core.
//!
//! The budget is live: [`set_capacity`] is called at startup from the
//! `mediaConcurrency` admin setting and again whenever it changes, so an operator
//! can throttle (or open up) media processing without a restart, exactly like the
//! HLS cache budget.
//!
//! A hand-rolled counting semaphore (Mutex + Condvar): these callers all run on
//! blocking threads (the pipeline dispatcher's scoped workers, the blocking
//! storyboard generate), so a blocking acquire is exactly right and avoids pulling
//! an async runtime into the leaf process plumbing. No caller holds a permit while
//! waiting on another (every ffmpeg pass is sequential within an item), so the
//! single-budget gate cannot deadlock.

use std::sync::{Condvar, Mutex, OnceLock};

struct Gate {
    inner: Mutex<Inner>,
    changed: Condvar,
}

struct Inner {
    /// Max ffmpeg passes allowed to run at once (>= 1).
    capacity: usize,
    /// How many are running right now.
    in_use: usize,
}

static GATE: OnceLock<Gate> = OnceLock::new();

/// The budget used before [`set_capacity`] runs (e.g. an on-demand storyboard
/// generated before `AppState::new` seeds the setting) and the fallback when the
/// setting says "auto". `KROMA_FFMPEG_CONCURRENCY` overrides for ops/debugging;
/// otherwise `cores - 1` so the box always keeps a core, floored at 1.
pub fn auto_capacity() -> usize {
    if let Some(n) = std::env::var("KROMA_FFMPEG_CONCURRENCY")
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
        .filter(|n| *n > 0)
    {
        return n;
    }
    let cores = std::thread::available_parallelism().map(std::num::NonZeroUsize::get).unwrap_or(4);
    cores.saturating_sub(1).max(1)
}

fn gate() -> &'static Gate {
    GATE.get_or_init(|| Gate {
        inner: Mutex::new(Inner { capacity: auto_capacity(), in_use: 0 }),
        changed: Condvar::new(),
    })
}

/// Set the live budget (clamped to >= 1). Growing it wakes any blocked waiters so
/// the extra slots are taken up immediately; shrinking it just lets the current
/// passes drain (a permit already granted is never revoked mid-flight).
pub fn set_capacity(permits: usize) {
    let gate = gate();
    {
        let mut inner = gate.inner.lock().unwrap();
        inner.capacity = permits.max(1);
    }
    gate.changed.notify_all();
}

/// Held for the lifetime of one ffmpeg pass; returns its slot to the pool on drop
/// (including on panic or an early `?` return), so a slot is never leaked.
pub struct Permit;

impl Drop for Permit {
    fn drop(&mut self) {
        let gate = gate();
        {
            let mut inner = gate.inner.lock().unwrap();
            inner.in_use = inner.in_use.saturating_sub(1);
        }
        gate.changed.notify_one();
    }
}

/// Block until a slot is free, then take it. Call right before spawning ffmpeg and
/// keep the returned permit alive (bind it, do not `let _ = `) until the process
/// has exited.
#[must_use]
pub fn acquire() -> Permit {
    let gate = gate();
    let mut inner = gate.inner.lock().unwrap();
    while inner.in_use >= inner.capacity {
        inner = gate.changed.wait(inner).unwrap();
    }
    inner.in_use += 1;
    Permit
}
