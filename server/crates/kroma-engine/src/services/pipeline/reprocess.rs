//! Force-reprocess one catalog element through the pipeline. Because the ledger's
//! done-ness is keyed on `input_sig` (decoupled from the artifact on disk/DB),
//! forcing a rebuild means both clearing the element's derived artifacts AND
//! resetting its ledger tasks to `pending`, then kicking the stages so it runs
//! now (at HIGH priority, ahead of the routine backlog).

use anyhow::{bail, Result};

use crate::db;
use crate::model::Kind;
use crate::services::jobs::now_ms;
use crate::state::SharedState;

/// Reprocessed elements jump ahead of the nightly backlog.
const HIGH: i64 = 100;

/// What a reprocess kicked off.
pub struct Outcome {
    /// Ledger tasks (re)queued across all stages.
    pub subjects: usize,
    /// Full keys of the stage drains triggered.
    pub stages: Vec<&'static str>,
}

/// Reprocess one element. `kind` is `"item"` (a movie or a single episode) or
/// `"show"` (the whole series). Clears the element's derived artifacts, resets its
/// ledger tasks, and triggers the relevant stage drains.
pub fn reprocess(state: &SharedState, kind: &str, id: &str) -> Result<Outcome> {
    let db = &state.db;
    let now = now_ms();
    let mut subjects = 0usize;
    let stages: Vec<&'static str> = match kind {
        "item" => reprocess_item(state, db, id, now, &mut subjects)?,
        "show" => reprocess_show(state, db, id, now, &mut subjects)?,
        other => bail!("unknown element kind {other:?}"),
    };

    // Kick the drains now; highest-priority tasks (ours) run first. A stage
    // already running just absorbs the new pending tasks on its next batch.
    for key in stages.iter().copied() {
        if let Some(job) = state.jobs.resolve(key) {
            let _ = state.jobs.trigger(state.clone(), job, "reprocess");
        }
    }
    Ok(Outcome { subjects, stages })
}

/// Force ONE stage to re-run for one element: clear that stage's artifact for the
/// element's subject(s), requeue them HIGH, and kick the stage. Used by the
/// per-treatment "retry this stage" action in the element drawer.
pub fn stage_for(state: &SharedState, kind: &str, id: &str, stage: &str) -> Result<()> {
    let db = &state.db;
    let now = now_ms();
    match stage {
        "metadata" => {
            if kind == "show" {
                db::clear_show_metadata(db, id)?;
            } else {
                db::clear_item_metadata(db, id)?;
            }
            db::pipeline::enqueue(db, "metadata", "item", id, HIGH, now)?;
        }
        "embed" => {
            db::clear_item_vector(db, id)?;
            db::pipeline::enqueue(db, "embed", "item", id, HIGH, now)?;
        }
        "storyboard" => {
            if kind == "show" {
                // Storyboards live per episode, not on the show id: fan out just
                // like `reprocess_show` so each episode is actually rebuilt.
                for ep in show_episodes(db, id)? {
                    state.storyboard.invalidate(&ep);
                    db::pipeline::enqueue(db, "storyboard", "item", &ep.id, HIGH, now)?;
                }
            } else {
                if let Some(item) = db::get_item(db, id)? {
                    state.storyboard.invalidate(&item);
                }
                db::pipeline::enqueue(db, "storyboard", "item", id, HIGH, now)?;
            }
        }
        "subtitles" => {
            if kind == "show" {
                // Subtitles live per episode: fan out to the show's episodes.
                for ep in show_episodes(db, id)? {
                    if let Some(abs) = ep.abs_path.as_deref() {
                        crate::infra::subtitles::invalidate(&state.config.data_dir, abs, &ep.subtitles);
                    }
                    db::pipeline::enqueue(db, "subtitles", "item", &ep.id, HIGH, now)?;
                }
            } else {
                if let Some(item) = db::get_item(db, id)? {
                    if let Some(abs) = item.abs_path.as_deref() {
                        crate::infra::subtitles::invalidate(&state.config.data_dir, abs, &item.subtitles);
                    }
                }
                db::pipeline::enqueue(db, "subtitles", "item", id, HIGH, now)?;
            }
        }
        "probe" => {
            if kind == "show" {
                // Files hang off episodes, not the show: fan out per episode.
                for ep in show_episodes(db, id)? {
                    db::unprobe_item_files(db, &ep.id)?;
                    for file_id in db::file_ids_for_item(db, &ep.id)? {
                        db::pipeline::enqueue(db, "probe", "file", &file_id, HIGH, now)?;
                    }
                }
            } else {
                db::unprobe_item_files(db, id)?;
                for file_id in db::file_ids_for_item(db, id)? {
                    db::pipeline::enqueue(db, "probe", "file", &file_id, HIGH, now)?;
                }
            }
        }
        "markers" => {
            let seasons: Vec<String> = if kind == "show" {
                db::get_show(db, id)?
                    .map(|d| d.seasons.iter().map(|s| format!("{id}#{}", s.number)).collect())
                    .unwrap_or_default()
            } else {
                db::get_item(db, id)?
                    .and_then(|it| match (it.show_id, it.season) {
                        (Some(sh), Some(n)) => Some(vec![format!("{sh}#{n}")]),
                        _ => None,
                    })
                    .unwrap_or_default()
            };
            for s in &seasons {
                db::pipeline::enqueue(db, "markers", "season", s, HIGH, now)?;
            }
        }
        other => bail!("unknown stage {other:?}"),
    }
    if let Some(job) = state.jobs.resolve(&format!("pipeline.{stage}")) {
        let _ = state.jobs.trigger(state.clone(), job, "reprocess");
    }
    Ok(())
}

/// Every episode of a show, flattened across seasons. Used to fan a show-level
/// per-stage retry (storyboard/subtitles/probe) out to the episodes/files that
/// actually carry those artifacts (a show id has none of its own).
fn show_episodes(db: &db::Pool, id: &str) -> Result<Vec<crate::model::MediaItem>> {
    Ok(db::get_show(db, id)?
        .map(|d| d.seasons.into_iter().flat_map(|s| s.episodes).collect())
        .unwrap_or_default())
}

/// Queue every processing subject for one item + reset its artifacts.
fn reprocess_item(
    state: &SharedState,
    db: &db::Pool,
    id: &str,
    now: i64,
    subjects: &mut usize,
) -> Result<Vec<&'static str>> {
    let Some(item) = db::get_item(db, id)? else {
        bail!("unknown item {id}");
    };
    // Clear artifacts so the re-run actually rebuilds.
    db::unprobe_item_files(db, id)?;
    db::clear_item_vector(db, id)?;
    state.storyboard.invalidate(&item);
    if let Some(abs) = item.abs_path.as_deref() {
        crate::infra::subtitles::invalidate(&state.config.data_dir, abs, &item.subtitles);
    }

    for file_id in db::file_ids_for_item(db, id)? {
        db::pipeline::enqueue(db, "probe", "file", &file_id, HIGH, now)?;
        *subjects += 1;
    }
    db::pipeline::enqueue(db, "storyboard", "item", id, HIGH, now)?;
    db::pipeline::enqueue(db, "subtitles", "item", id, HIGH, now)?;
    *subjects += 2;

    if matches!(item.kind, Kind::Movie | Kind::Video) {
        db::clear_item_metadata(db, id)?;
        db::pipeline::enqueue(db, "metadata", "item", id, HIGH, now)?;
        db::pipeline::enqueue(db, "embed", "item", id, HIGH, now)?;
        *subjects += 2;
        Ok(vec![
            "pipeline.probe",
            "pipeline.metadata",
            "pipeline.storyboard",
            "pipeline.subtitles",
            "pipeline.embed",
        ])
    } else {
        // An episode: markers are per season, and metadata/embed live on the show.
        if let (Some(show_id), Some(season)) = (item.show_id.clone(), item.season) {
            db::pipeline::enqueue(db, "markers", "season", &format!("{show_id}#{season}"), HIGH, now)?;
            *subjects += 1;
        }
        Ok(vec!["pipeline.probe", "pipeline.storyboard", "pipeline.subtitles", "pipeline.markers"])
    }
}

/// Queue every processing subject for a whole show + reset its artifacts.
fn reprocess_show(
    state: &SharedState,
    db: &db::Pool,
    id: &str,
    now: i64,
    subjects: &mut usize,
) -> Result<Vec<&'static str>> {
    let Some(detail) = db::get_show(db, id)? else {
        bail!("unknown show {id}");
    };
    db::clear_show_metadata(db, id)?;
    db::clear_item_vector(db, id)?;
    db::pipeline::enqueue(db, "metadata", "item", id, HIGH, now)?;
    db::pipeline::enqueue(db, "embed", "item", id, HIGH, now)?;
    *subjects += 2;

    for season in &detail.seasons {
        db::pipeline::enqueue(db, "markers", "season", &format!("{id}#{}", season.number), HIGH, now)?;
        *subjects += 1;
        for ep in &season.episodes {
            db::unprobe_item_files(db, &ep.id)?;
            state.storyboard.invalidate(ep);
            if let Some(abs) = ep.abs_path.as_deref() {
                crate::infra::subtitles::invalidate(&state.config.data_dir, abs, &ep.subtitles);
            }
            for file_id in db::file_ids_for_item(db, &ep.id)? {
                db::pipeline::enqueue(db, "probe", "file", &file_id, HIGH, now)?;
                *subjects += 1;
            }
            db::pipeline::enqueue(db, "storyboard", "item", &ep.id, HIGH, now)?;
            db::pipeline::enqueue(db, "subtitles", "item", &ep.id, HIGH, now)?;
            *subjects += 2;
        }
    }
    Ok(vec![
        "pipeline.probe",
        "pipeline.metadata",
        "pipeline.storyboard",
        "pipeline.subtitles",
        "pipeline.markers",
        "pipeline.embed",
    ])
}
