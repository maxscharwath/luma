//! Library "missing" scan (Sonarr-style): for every library show that resolved a
//! TMDB id, diff TMDB's AIRED episode list against what is on disk and record the
//! gaps in `library_gaps`. This is what lets the Wanted/Missing view surface a
//! series with missing episodes even when it was never requested in-app (a show
//! added by scanning has no request ledger). Best-effort + per-show isolated: a
//! vanished TMDB id or a transient error skips that show, never the whole run.
//!
//! Only episodes with a known air date in the past/today count as "missing"
//! (matching Sonarr: an undated or future episode has not aired, so its absence
//! is not a gap). Movies are out of scope a movie is either present or not, with
//! no episode granularity to be "partially" missing.

use std::collections::HashSet;

use anyhow::{anyhow, Result};

use kroma_module_host::HostCtx;

use crate::db;
use crate::infra::metadata::{self, discover};
use crate::model::RequestKind;
use crate::services::jobs::now_ms;
use crate::services::requests::today_ymd;

/// One missing episode: `(season, episode, air_date)`.
type Gap = (u32, u32, Option<String>);

#[derive(Debug, Default)]
pub struct MissingScanSummary {
    /// Shows with a TMDB id that were scanned.
    pub shows: usize,
    /// Shows that had at least one missing aired episode.
    pub with_gaps: usize,
    /// Total missing aired episodes across the library.
    pub episodes: usize,
}

/// Scan every library show, recording each one's missing aired episodes. Reports
/// progress + honours cancellation between shows (a full scan is a lot of TMDB
/// calls). Rewrites `library_gaps` per show, so a show that is now complete has
/// its rows cleared.
pub fn scan<S: HostCtx>(
    state: &S,
    progress: &dyn Fn(usize, usize),
    cancelled: &dyn Fn() -> bool,
) -> Result<MissingScanSummary> {
    let key = state.tmdb_api_key().ok_or_else(|| anyhow!("TMDB is not configured"))?;
    let lang = state.metadata_language();
    let today = today_ymd();
    let shows = db::list_shows(state.db(), None)?;
    let total = shows.len();
    let mut summary = MissingScanSummary::default();

    for (i, show) in shows.iter().enumerate() {
        if cancelled() {
            break;
        }
        progress(i, total);
        let Some(tmdb_id) = show.metadata.as_ref().map(|m| m.tmdb_id) else {
            continue; // not enriched yet no TMDB episode list to diff against
        };
        summary.shows += 1;
        match scan_one(state, &key, &lang, &today, &show.id, tmdb_id) {
            Ok((poster, gaps)) => {
                if !gaps.is_empty() {
                    summary.with_gaps += 1;
                    summary.episodes += gaps.len();
                }
                // Rewrite even when empty, so a now-complete show is cleared.
                db::replace_show_gaps(
                    state.db(),
                    &show.id,
                    tmdb_id,
                    &show.title,
                    poster.as_deref(),
                    &gaps,
                    now_ms(),
                )?;
            }
            Err(e) => {
                tracing::warn!(target: "library", show = %show.id, "missing scan failed: {e:#}");
            }
        }
    }
    progress(total, total);
    Ok(summary)
}

/// Diff one show against TMDB: fetch its seasons + each season's episodes, keep
/// the aired ones not present on disk. Returns the show's poster (for the gap
/// rows) and the gaps.
fn scan_one<S: HostCtx>(
    state: &S,
    key: &str,
    lang: &str,
    today: &str,
    show_id: &str,
    tmdb_id: u64,
) -> Result<(Option<String>, Vec<Gap>)> {
    let detail = discover::detail(key, lang, RequestKind::Show, tmdb_id)
        .map_err(|()| anyhow!("TMDB lookup failed"))?
        .ok_or_else(|| anyhow!("show not found on TMDB"))?;

    let conn = state.db().get()?;
    let present: HashSet<(u32, u32)> =
        db::episodes_present(&conn, show_id)?.into_iter().collect();
    drop(conn);

    let mut gaps: Vec<Gap> = Vec::new();
    // `detail.seasons` already excludes specials (season 0).
    for s in &detail.seasons {
        let data = metadata::season_episodes(key, lang, tmdb_id, s.season);
        for ep in data.episodes {
            let aired = ep.air_date.as_deref().is_some_and(|d| d <= today);
            if aired && !present.contains(&(s.season, ep.episode)) {
                gaps.push((s.season, ep.episode, ep.air_date));
            }
        }
    }
    Ok((detail.poster_url, gaps))
}
