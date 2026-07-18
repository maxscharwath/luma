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

#[cfg(test)]
mod tests {
    use super::*;

    fn reg() -> Arc<GenRegistry> {
        Arc::new(GenRegistry::default())
    }

    #[test]
    fn overall_maps_stages_to_a_single_bar() {
        assert_eq!(overall("model", 0, 0), 0.04);
        assert_eq!(overall("extract", 0, 0), 0.10);
        assert_eq!(overall("done", 1, 1), 1.0);
        assert_eq!(overall("queued", 0, 0), 0.0);
        // transcribe: 0.10 head + 0.88 * frac.
        assert!((overall("transcribe", 1, 2) - (0.10 + 0.88 * 0.5)).abs() < 1e-6);
        // frac clamps to 1, so transcribe tops out at 0.10 + 0.88 = 0.98.
        assert!((overall("transcribe", 5, 1) - 0.98).abs() < 1e-6);
        // translate maps frac straight onto the bar, capped at 0.99.
        assert_eq!(overall("translate", 5, 1), 0.99);
        // total 0 -> frac 0.
        assert!((overall("transcribe", 3, 0) - 0.10).abs() < 1e-6);
    }

    #[test]
    fn start_assigns_sequential_ids() {
        let r = reg();
        let h0 = r.start("item1", "transcribe", Some("French".into()));
        let h1 = r.start("item1", "translate", Some("English".into()));
        assert_eq!(h0.id(), "gen0");
        assert_eq!(h1.id(), "gen1");
    }

    #[test]
    fn views_for_filters_by_item_and_orders_by_start() {
        let r = reg();
        let _a = r.start("itemA", "transcribe", None);
        let _b = r.start("itemB", "transcribe", None);
        let _c = r.start("itemA", "translate", Some("French".into()));
        let views = r.views_for("itemA");
        assert_eq!(views.len(), 2);
        // creation order preserved (gen0 then gen2).
        assert_eq!(views[0].id, "gen0");
        assert_eq!(views[1].id, "gen2");
        assert_eq!(views[0].status, "running");
    }

    #[test]
    fn handle_stage_progress_and_view() {
        let r = reg();
        let h = r.start("item1", "transcribe", Some("French".into()));
        h.stage("transcribe");
        h.progress(1, 2);
        let v = &r.views_for("item1")[0];
        assert_eq!(v.stage, "transcribe");
        assert!((v.progress - (0.10 + 0.88 * 0.5)).abs() < 1e-6);
        assert_eq!(v.status, "running");
        // eta is Some while progress is inside (0.04, 0.999).
        assert!(v.eta_sec.is_some());
    }

    #[test]
    fn handle_done_sets_terminal_snapshot() {
        let r = reg();
        let h = r.start("item1", "transcribe", None);
        h.done("dl-123");
        let v = &r.views_for("item1")[0];
        assert_eq!(v.status, "done");
        assert_eq!(v.stage, "done");
        assert_eq!(v.progress, 1.0);
        assert_eq!(v.sub_id.as_deref(), Some("dl-123"));
        assert!(v.eta_sec.is_none()); // not running
    }

    #[test]
    fn handle_fail_sets_error() {
        let r = reg();
        let h = r.start("item1", "translate", None);
        h.progress(1, 4);
        h.stage("translate");
        h.progress(1, 4);
        h.fail("provider down");
        let v = &r.views_for("item1")[0];
        assert_eq!(v.status, "error");
        assert_eq!(v.error.as_deref(), Some("provider down"));
    }

    #[test]
    fn cancel_sets_flag_and_reports_found() {
        let r = reg();
        let h = r.start("item1", "transcribe", None);
        assert!(!h.cancelled());
        assert!(r.cancel(h.id()));
        assert!(h.cancelled());
        assert!(h.cancel_flag().load(Ordering::Relaxed));
        // Unknown id -> false.
        assert!(!r.cancel("nope"));
    }

    #[test]
    fn find_running_matches_and_ignores_finished() {
        let r = reg();
        let h = r.start("item1", "translate", Some("French".into()));
        assert_eq!(r.find_running("item1", "translate", "French").as_deref(), Some("gen0"));
        assert!(r.find_running("item1", "translate", "English").is_none());
        assert!(r.find_running("other", "translate", "French").is_none());
        // Once finished it is no longer "running".
        h.done("x");
        assert!(r.find_running("item1", "translate", "French").is_none());
    }

    #[test]
    fn views_for_prunes_finished_after_linger() {
        let r = reg();
        let h = r.start("item1", "transcribe", None);
        h.done("x");
        // Age the finished timestamp beyond LINGER.
        {
            let mut map = r.inner.lock().unwrap();
            let e = map.get_mut("gen0").unwrap();
            e.finished = Some(Instant::now() - LINGER - Duration::from_secs(5));
        }
        assert!(r.views_for("item1").is_empty());
    }
}
