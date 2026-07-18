//! Pipeline stage `markers`: detect intro/credits segments, one **season** at a
//! time (chromaprint aligns a season's episodes pairwise, so the season is the
//! natural unit). Wraps [`crate::services::markers::job::detect_season`]; the
//! ledger makes it incremental (a season whose episode files are unchanged is
//! skipped) and per-season failures visible, replacing the old whole-library
//! re-fingerprint that took hours every run.

use anyhow::{anyhow, Result};

use crate::services::jobs::{JobContext, JobKey, Trigger};
use crate::state::SharedState;

use super::common::stage;

// One season at a time; `detect_season` parallelizes the episode decode internally
// and yields to playback there, so the dispatcher does not. Nightly, and chained
// after `subtitles` (the tail of the storyboard -> subtitles -> markers heavy-stage
// chain, so they run one at a time rather than all firing on the same library
// change). Also manual.
stage! {
    short: "markers",
    subject_kind: "season",
    concurrency: 1,
    pause_for_playback: false,
    schedule: Some("30 3 * * *"),
    triggers: &[Trigger::AfterJob(JobKey("pipeline.subtitles"))],
}

/// One subject per season that has at least one probed episode. Subject id is
/// `"{show_id}#{season}"`; signature = detection mode + every episode file's
/// `mtime:size`, so a replaced episode or a mode change re-runs just that season.
/// When detection is off, nothing is in scope (existing tasks are then purged).
fn enumerate(state: &SharedState) -> Result<Vec<(String, String)>> {
    let mode = state.settings.get_str("introDetection", "chapters");
    if mode == "off" {
        return Ok(Vec::new());
    }
    let shows = crate::db::list_shows(&state.db, None)?;
    let mut out = Vec::new();
    for show in &shows {
        let Some(detail) = crate::db::get_show(&state.db, &show.id)? else {
            continue;
        };
        for season in &detail.seasons {
            if let Some(sig) = season_signature(&mode, season) {
                out.push((format!("{}#{}", show.id, season.number), sig));
            }
        }
    }
    Ok(out)
}

/// The ledger signature for one season: detection mode + every playable episode
/// file's `mtime:size`. `None` when the season has no probed episodes yet (wait
/// for probe/scan). An unreadable episode (mount blip) collapses the whole season
/// to [`UNREADABLE_SIG`](crate::db::pipeline::UNREADABLE_SIG) so `reconcile` skips
/// it rather than re-fingerprinting on every flap.
fn season_signature(mode: &str, season: &crate::model::Season) -> Option<String> {
    let mut parts = vec![mode.to_string()];
    let mut playable = 0usize;
    let mut unreadable = false;
    for ep in &season.episodes {
        if let (Some(abs), Some(d)) = (ep.abs_path.as_deref(), ep.duration_ms) {
            if d > 0 {
                playable += 1;
                let sig = super::sig_for_path(abs);
                unreadable |= sig == crate::db::pipeline::UNREADABLE_SIG;
                parts.push(sig);
            }
        }
    }
    if playable == 0 {
        return None; // no probed episodes yet: wait for probe/scan
    }
    if unreadable {
        Some(crate::db::pipeline::UNREADABLE_SIG.to_string())
    } else {
        Some(crate::services::scan::short_hash(&parts.join("|")))
    }
}

fn process(ctx: &JobContext, subject_id: &str) -> Result<()> {
    let (show_id, season_num) = subject_id
        .rsplit_once('#')
        .ok_or_else(|| anyhow!("malformed season subject id {subject_id}"))?;
    let season_num: u32 = season_num
        .parse()
        .map_err(|_| anyhow!("malformed season number in {subject_id}"))?;
    let detail = crate::db::get_show(&ctx.state.db, show_id)?
        .ok_or_else(|| anyhow!("show {show_id} no longer exists"))?;
    let season = detail
        .seasons
        .iter()
        .find(|s| s.number == season_num)
        .ok_or_else(|| anyhow!("season {season_num} of {show_id} no longer exists"))?;
    crate::services::markers::job::detect_season(ctx, season)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::season_signature;
    use crate::model::{Kind, MediaItem, Season};

    /// A bare episode item; only the fields `season_signature` reads
    /// (`abs_path`, `duration_ms`) vary per test.
    fn episode(abs_path: Option<&str>, duration_ms: Option<u64>) -> MediaItem {
        MediaItem {
            id: "e".into(),
            title: "E".into(),
            kind: Kind::Episode,
            year: None,
            duration_ms,
            container: "mkv".into(),
            video: None,
            audio: None,
            audio_tracks: Vec::new(),
            subtitles: Vec::new(),
            library: "lib1".into(),
            show_id: Some("s1".into()),
            show_title: Some("Show".into()),
            season: Some(1),
            episode: Some(1),
            episode_end: None,
            episode_title: None,
            rel_path: None,
            added_at: "now".into(),
            metadata: None,
            abs_path: abs_path.map(str::to_string),
            files: Vec::new(),
            default_file_id: None,
            markers: Vec::new(),
            audio_analysis: None,
        }
    }

    fn season(episodes: Vec<MediaItem>) -> Season {
        Season { number: 1, episodes, cast: Vec::new() }
    }

    #[test]
    fn season_signature_none_without_probed_episodes() {
        // No abs_path or a zero/missing duration means "not yet probed".
        assert_eq!(season_signature("chapters", &season(vec![episode(None, Some(1000))])), None);
        assert_eq!(
            season_signature("chapters", &season(vec![episode(Some("/x.mkv"), None)])),
            None
        );
        assert_eq!(
            season_signature("chapters", &season(vec![episode(Some("/x.mkv"), Some(0))])),
            None
        );
        assert_eq!(season_signature("chapters", &season(vec![])), None);
    }

    #[test]
    fn season_signature_unreadable_file_collapses_to_sentinel() {
        // A playable episode pointing at a missing path stats as unreadable, so the
        // whole season collapses to the UNREADABLE sentinel (reconcile leaves it be).
        let sig = season_signature(
            "chapters",
            &season(vec![episode(Some("/no/such/kroma/file.mkv"), Some(1000))]),
        );
        assert_eq!(sig.as_deref(), Some(crate::db::pipeline::UNREADABLE_SIG));
    }

    #[test]
    fn season_signature_hashes_readable_files_and_depends_on_mode() {
        use std::sync::atomic::{AtomicU32, Ordering};
        static SEQ: AtomicU32 = AtomicU32::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("kroma-seasonsig-{}-{n}.mkv", std::process::id()));
        std::fs::write(&path, b"hello").unwrap();
        let abs = path.to_string_lossy().to_string();

        let s = season(vec![episode(Some(&abs), Some(1000))]);
        let a = season_signature("chapters", &s).unwrap();
        // A real, readable file yields a stable non-sentinel hash.
        assert_ne!(a, crate::db::pipeline::UNREADABLE_SIG);
        assert_eq!(a, season_signature("chapters", &s).unwrap());
        // The detection mode is folded into the signature, so it changes with mode.
        assert_ne!(a, season_signature("silence", &s).unwrap());

        let _ = std::fs::remove_file(&path);
    }
}
