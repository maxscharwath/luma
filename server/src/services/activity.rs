//! Live scan / enrichment activity, exposed at `GET /api/status`.
//!
//! Complements the `/api/events` WebSocket (which streams deltas) with a
//! queryable *snapshot*, so a client that connects mid-scan can still show
//! current progress (a Plex-style "Activity" panel). Behind a `std::sync::RwLock`
//! because the enrichment workers are plain threads.

use std::sync::{Arc, RwLock};

use serde::Serialize;

/// A snapshot of what the server is doing.
#[derive(Debug, Clone, Serialize)]
pub struct Activity {
    /// `idle` | `scanning` | `enriching` | `ready`.
    pub phase: &'static str,
    pub scanning: bool,
    pub libraries: usize,
    pub shows: usize,
    pub items: usize,
    #[serde(rename = "enrichDone")]
    pub enrich_done: usize,
    #[serde(rename = "enrichTotal")]
    pub enrich_total: usize,
    /// Background per-file probing (phase 2) progress.
    #[serde(rename = "probeDone")]
    pub probe_done: usize,
    #[serde(rename = "probeTotal")]
    pub probe_total: usize,
    #[serde(rename = "lastScanAt")]
    pub last_scan_at: Option<String>,
}

impl Default for Activity {
    fn default() -> Self {
        Self {
            phase: "idle",
            scanning: false,
            libraries: 0,
            shows: 0,
            items: 0,
            enrich_done: 0,
            enrich_total: 0,
            probe_done: 0,
            probe_total: 0,
            last_scan_at: None,
        }
    }
}

/// Cheap-to-clone shared handle.
pub type Shared = Arc<RwLock<Activity>>;

pub fn new() -> Shared {
    Arc::new(RwLock::new(Activity::default()))
}

pub fn snapshot(a: &Shared) -> Activity {
    a.read().unwrap().clone()
}

pub fn scan_started(a: &Shared) {
    let mut g = a.write().unwrap();
    g.phase = "scanning";
    g.scanning = true;
}

pub fn scan_completed(a: &Shared, libraries: usize, shows: usize, items: usize, when: String) {
    let mut g = a.write().unwrap();
    g.scanning = false;
    g.libraries = libraries;
    g.shows = shows;
    g.items = items;
    g.last_scan_at = Some(when);
    g.phase = "ready";
}

pub fn enrich_started(a: &Shared, total: usize) {
    let mut g = a.write().unwrap();
    g.phase = if total > 0 { "enriching" } else { "ready" };
    g.enrich_total = total;
    g.enrich_done = 0;
}

pub fn enrich_progress(a: &Shared, done: usize) {
    a.write().unwrap().enrich_done = done;
}

pub fn enrich_completed(a: &Shared) {
    let mut g = a.write().unwrap();
    g.enrich_done = g.enrich_total;
    g.phase = "ready";
}

pub fn probe_started(a: &Shared, total: usize) {
    let mut g = a.write().unwrap();
    if total > 0 {
        g.phase = "probing";
    }
    g.probe_total = total;
    g.probe_done = 0;
}

pub fn probe_progress(a: &Shared, done: usize) {
    a.write().unwrap().probe_done = done;
}

pub fn probe_completed(a: &Shared) {
    let mut g = a.write().unwrap();
    g.probe_done = g.probe_total;
    if g.phase == "probing" {
        g.phase = "ready";
    }
}
