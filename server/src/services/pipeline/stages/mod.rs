//! The concrete pipeline stages. Each owns its `Stage` descriptor + drain
//! `Builtin` (`SPEC`) next to its `enumerate`/`process`, mirroring the job roster
//! pattern. The stages wrap existing processing code (`infra::storyboard`,
//! `services::markers`) so the ffmpeg/chromaprint logic stays put; only the
//! iteration/skip/retry moves onto the ledger.
//!
//! v1 ships the two heaviest jobs (storyboard, markers). Adding another stage
//! (probe, metadata, embed) is just another file here + one roster entry: give it
//! an `enumerate` (the incremental scope) and a `process` (wrap the existing
//! per-subject code).

pub mod embed;
pub mod markers;
pub mod metadata;
pub mod probe;
pub mod storyboard;
pub mod subtitles;

/// A cheap change-signature for a file: `mtime:size`. Changes when the file is
/// replaced, so the ledger re-queues that subject. Returns
/// [`crate::db::pipeline::UNREADABLE_SIG`] when the file can't be stat'd (e.g. the
/// media mount is briefly offline), which `reconcile` treats as "leave the task
/// alone" rather than a changed input, so a flapping mount does not re-queue the
/// whole library.
pub(crate) fn sig_for_path(abs: &str) -> String {
    match std::fs::metadata(abs) {
        Ok(m) => {
            let mtime = m
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            format!("{mtime}:{}", m.len())
        }
        Err(_) => crate::db::pipeline::UNREADABLE_SIG.to_string(),
    }
}
