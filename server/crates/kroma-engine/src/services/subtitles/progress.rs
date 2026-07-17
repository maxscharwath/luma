//! In-memory registry of in-flight subtitle generations. A generation reports a
//! coarse `stage` plus a fine `done/total` so the player can show a live progress
//! bar + ETA, and can be cancelled mid-run. Finished entries linger briefly so a
//! polling client catches the terminal (`done`/`error`) snapshot, then are pruned.
//!
//! This is deliberately *not* the cron job system ([`crate::services::jobs`]):
//! generations are ad-hoc, per-item, and short-lived, and the UI renders each as
//! an inline pseudo-track rather than a console row.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;

/// How long a finished generation stays listed so a polling client sees its
/// terminal snapshot before it disappears.
const LINGER: Duration = Duration::from_secs(45);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Status {
    Running,
    Done,
    Error,
}

#[derive(Clone)]
struct Entry {
    id: String,
    item_id: String,
    /// `"transcribe"` | `"translate"`.
    mode: String,
    /// Target language label (what the track will be).
    lang: Option<String>,
    /// `queued` | `model` | `extract` | `transcribe` | `translate` | `done` | `error`.
    stage: String,
    done: usize,
    total: usize,
    status: Status,
    error: Option<String>,
    /// Resulting [`crate::db::DownloadedSub`] id once finished.
    sub_id: Option<String>,
    started: Instant,
    finished: Option<Instant>,
    cancel: Arc<AtomicBool>,
}

/// A generation as the client polls it. `progress` is an overall 0..1 fraction
/// (stages mapped onto one bar); `etaSec` is a rough remaining-seconds estimate.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerationView {
    pub id: String,
    pub mode: String,
    pub lang: Option<String>,
    pub stage: String,
    /// `"running"` | `"done"` | `"error"`.
    pub status: String,
    pub progress: f32,
    pub eta_sec: Option<i64>,
    pub error: Option<String>,
    pub sub_id: Option<String>,
}

/// Map a (stage, done, total) to a single overall 0..1 bar. Extraction occupies a
/// small head (it has no sub-progress); transcription fills the rest.
fn overall(stage: &str, done: usize, total: usize) -> f32 {
    let frac = if total > 0 { (done as f32 / total as f32).clamp(0.0, 1.0) } else { 0.0 };
    match stage {
        "model" => 0.04,
        "extract" => 0.10,
        "transcribe" => (0.10 + 0.88 * frac).min(0.99),
        "translate" => (frac).min(0.99),
        "done" => 1.0,
        _ => 0.0,
    }
}

impl Entry {
    fn view(&self) -> GenerationView {
        let progress = match self.status {
            Status::Done => 1.0,
            Status::Error => overall(&self.stage, self.done, self.total),
            Status::Running => overall(&self.stage, self.done, self.total),
        };
        let eta_sec = if self.status == Status::Running && (0.04..0.999).contains(&progress) {
            let elapsed = self.started.elapsed().as_secs_f32();
            let remaining = elapsed * (1.0 - progress) / progress;
            Some(remaining.round().max(1.0) as i64)
        } else {
            None
        };
        let status = match self.status {
            Status::Running => "running",
            Status::Done => "done",
            Status::Error => "error",
        };
        GenerationView {
            id: self.id.clone(),
            mode: self.mode.clone(),
            lang: self.lang.clone(),
            stage: self.stage.clone(),
            status: status.to_string(),
            progress,
            eta_sec,
            error: self.error.clone(),
            sub_id: self.sub_id.clone(),
        }
    }
}

/// Process-wide registry of subtitle generations, held on the app state.
#[derive(Default)]
pub struct GenRegistry {
    inner: Mutex<HashMap<String, Entry>>,
    seq: AtomicU64,
}

impl GenRegistry {
    /// Register a new running generation and return a [`Handle`] the worker uses to
    /// report progress + completion.
    pub fn start(self: &Arc<Self>, item_id: &str, mode: &str, lang: Option<String>) -> Handle {
        let n = self.seq.fetch_add(1, Ordering::Relaxed);
        let id = format!("gen{n}");
        let cancel = Arc::new(AtomicBool::new(false));
        let entry = Entry {
            id: id.clone(),
            item_id: item_id.to_string(),
            mode: mode.to_string(),
            lang,
            stage: "queued".to_string(),
            done: 0,
            total: 0,
            status: Status::Running,
            error: None,
            sub_id: None,
            started: Instant::now(),
            finished: None,
            cancel: cancel.clone(),
        };
        self.inner.lock().unwrap().insert(id.clone(), entry);
        Handle { reg: self.clone(), id, cancel }
    }

    /// Live + recently-finished generations for an item, pruning stale ones.
    pub fn views_for(&self, item_id: &str) -> Vec<GenerationView> {
        let mut map = self.inner.lock().unwrap();
        map.retain(|_, e| match e.finished {
            // A finished entry lingers briefly so a polling client catches its
            // terminal snapshot, then is dropped.
            Some(f) => f.elapsed() < LINGER,
            // NEVER age-prune a still-running generation: CPU Whisper on a long
            // film can exceed any wall-clock guess, and dropping it here would make
            // the worker uncancellable and discard its later terminal snapshot.
            None => true,
        });
        // Order chronologically by start time, not the string id ("gen10" < "gen9"
        // lexicographically), so the client's list stays in creation order past 10.
        let mut entries: Vec<&Entry> = map.values().filter(|e| e.item_id == item_id).collect();
        entries.sort_by_key(|e| e.started);
        entries.into_iter().map(Entry::view).collect()
    }

    /// The id of an in-flight (not-yet-finished) generation matching this
    /// `(item, mode, target language)`, if any. Lets `generate` dedup a
    /// double-click instead of racing two workers on the same output file / DB row.
    pub fn find_running(&self, item_id: &str, mode: &str, target_lang: &str) -> Option<String> {
        let map = self.inner.lock().unwrap();
        map.values()
            .find(|e| {
                e.finished.is_none()
                    && e.item_id == item_id
                    && e.mode == mode
                    && e.lang.as_deref() == Some(target_lang)
            })
            .map(|e| e.id.clone())
    }

    /// Request cancellation of a running generation. Returns whether it was found.
    pub fn cancel(&self, id: &str) -> bool {
        let map = self.inner.lock().unwrap();
        match map.get(id) {
            Some(e) => {
                e.cancel.store(true, Ordering::Relaxed);
                true
            }
            None => false,
        }
    }

    fn mutate(&self, id: &str, f: impl FnOnce(&mut Entry)) {
        if let Some(e) = self.inner.lock().unwrap().get_mut(id) {
            f(e);
        }
    }
}

/// Worker-side handle for one generation: report progress, check cancellation, and
/// record the terminal result. Dropping it without a terminal call leaves the entry
/// `Running` until it lingers out (the endpoint always calls `done`/`fail`).
pub struct Handle {
    reg: Arc<GenRegistry>,
    id: String,
    cancel: Arc<AtomicBool>,
}

impl Handle {
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Move to a new coarse stage, resetting the fine counter.
    pub fn stage(&self, stage: &str) {
        let stage = stage.to_string();
        self.reg.mutate(&self.id, |e| {
            e.stage = stage;
            e.done = 0;
            e.total = 0;
        });
    }

    /// Update the fine progress within the current stage.
    pub fn progress(&self, done: usize, total: usize) {
        self.reg.mutate(&self.id, |e| {
            e.done = done;
            e.total = total;
        });
    }

    /// Whether cancellation has been requested (poll this in long loops).
    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    /// A shareable cancel flag for lower layers (the candle engine) to poll.
    pub fn cancel_flag(&self) -> Arc<AtomicBool> {
        self.cancel.clone()
    }

    /// Mark finished with the resulting downloaded-subtitle id.
    pub fn done(&self, sub_id: &str) {
        let sub_id = sub_id.to_string();
        self.reg.mutate(&self.id, |e| {
            e.stage = "done".to_string();
            e.status = Status::Done;
            e.sub_id = Some(sub_id);
            e.finished = Some(Instant::now());
        });
    }

    /// Mark failed (or cancelled) with a short message.
    pub fn fail(&self, msg: &str) {
        let msg = msg.to_string();
        self.reg.mutate(&self.id, |e| {
            e.stage = "error".to_string();
            e.status = Status::Error;
            e.error = Some(msg);
            e.finished = Some(Instant::now());
        });
    }
}
