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
        "metadata" => stage_metadata(db, kind, id, now)?,
        "embed" => stage_embed(db, id, now)?,
        "storyboard" => stage_storyboard(state, db, kind, id, now)?,
        "subtitles" => stage_subtitles(state, db, kind, id, now)?,
        "probe" => stage_probe(db, kind, id, now)?,
        "markers" => stage_markers(db, kind, id, now)?,
        other => bail!("unknown stage {other:?}"),
    }
    if let Some(job) = state.jobs.resolve(&format!("pipeline.{stage}")) {
        let _ = state.jobs.trigger(state.clone(), job, "reprocess");
    }
    Ok(())
}

/// Clear + requeue the metadata stage for one element.
fn stage_metadata(db: &db::Pool, kind: &str, id: &str, now: i64) -> Result<()> {
    if kind == "show" {
        db::clear_show_metadata(db, id)?;
    } else {
        db::clear_item_metadata(db, id)?;
    }
    db::pipeline::enqueue(db, "metadata", "item", id, HIGH, now)?;
    Ok(())
}

/// Clear + requeue the embed stage for one element.
fn stage_embed(db: &db::Pool, id: &str, now: i64) -> Result<()> {
    db::clear_item_vector(db, id)?;
    db::pipeline::enqueue(db, "embed", "item", id, HIGH, now)?;
    Ok(())
}

/// Storyboards live per episode, not on the show id: fan out just like
/// `reprocess_show` so each episode is actually rebuilt.
fn stage_storyboard(state: &SharedState, db: &db::Pool, kind: &str, id: &str, now: i64) -> Result<()> {
    if kind == "show" {
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
    Ok(())
}

/// Subtitles live per episode: fan out to the show's episodes.
fn stage_subtitles(state: &SharedState, db: &db::Pool, kind: &str, id: &str, now: i64) -> Result<()> {
    if kind == "show" {
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
    Ok(())
}

/// Files hang off episodes, not the show: fan out per episode.
fn stage_probe(db: &db::Pool, kind: &str, id: &str, now: i64) -> Result<()> {
    if kind == "show" {
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
    Ok(())
}

/// Markers are per season: enqueue every affected season key.
fn stage_markers(db: &db::Pool, kind: &str, id: &str, now: i64) -> Result<()> {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn test_pool() -> db::Pool {
        static SEQ: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("kroma-reproc-test-{}-{n}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        db::init(&path).unwrap()
    }

    /// Pending count for a stage after enqueue.
    fn pending(pool: &db::Pool, stage: &str) -> i64 {
        db::pipeline::counts(pool, stage).unwrap().0
    }

    #[test]
    fn stage_embed_queues_one_high_priority_item() {
        let pool = test_pool();
        // Clearing a vector for a non-existent id is a safe no-op; the enqueue is
        // the observable effect.
        stage_embed(&pool, "item-a", 1_000).unwrap();
        stage_embed(&pool, "item-b", 1_000).unwrap();
        assert_eq!(pending(&pool, "embed"), 2);
        // Re-enqueuing the same subject stays a single pending task (upsert).
        stage_embed(&pool, "item-a", 2_000).unwrap();
        assert_eq!(pending(&pool, "embed"), 2);
    }

    #[test]
    fn stage_metadata_queues_item_and_show() {
        let pool = test_pool();
        stage_metadata(&pool, "item", "movie-1", 1_000).unwrap();
        stage_metadata(&pool, "show", "show-1", 1_000).unwrap();
        assert_eq!(pending(&pool, "metadata"), 2);
    }

    #[test]
    fn stage_markers_noop_when_element_absent() {
        let pool = test_pool();
        // No show/item rows exist, so no season keys resolve and nothing is queued.
        stage_markers(&pool, "show", "ghost-show", 1_000).unwrap();
        stage_markers(&pool, "item", "ghost-item", 1_000).unwrap();
        assert_eq!(pending(&pool, "markers"), 0);
    }

    #[test]
    fn stage_markers_item_episode_queues_its_season_key() {
        let pool = test_pool();
        seed_library(&pool);
        let conn = pool.get().unwrap();
        conn.execute(
            "INSERT INTO shows (id,library,title,added_at) VALUES ('sh1','lib1','S','now')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO items (id,kind,title,container,library,show_id,season,episode,added_at) \
             VALUES ('ep1','episode','E','mkv','lib1','sh1',3,4,'now')",
            [],
        )
        .unwrap();
        // An episode with a show + season resolves exactly one `show#season` key.
        stage_markers(&pool, "item", "ep1", 1_000).unwrap();
        assert_eq!(pending(&pool, "markers"), 1);
    }

    fn seed_library(pool: &db::Pool) {
        pool.get()
            .unwrap()
            .execute(
                "INSERT INTO libraries (id,name,kind,path,added_at) VALUES ('lib1','L','shows','/x','now')",
                [],
            )
            .unwrap();
    }

    #[test]
    fn stage_probe_queues_one_task_per_file_of_the_item() {
        let pool = test_pool();
        seed_library(&pool);
        let conn = pool.get().unwrap();
        conn.execute(
            "INSERT INTO items (id,kind,title,container,library,added_at) VALUES ('it1','movie','T','mkv','lib1','now')",
            [],
        )
        .unwrap();
        conn.execute("INSERT INTO files (id,item_id,abs_path) VALUES ('f1','it1','/a/1.mkv')", []).unwrap();
        conn.execute("INSERT INTO files (id,item_id,abs_path) VALUES ('f2','it1','/a/2.mkv')", []).unwrap();
        drop(conn);

        stage_probe(&pool, "item", "it1", 1_000).unwrap();
        assert_eq!(pending(&pool, "probe"), 2);
        // An item with no files queues nothing.
        stage_probe(&pool, "item", "ghost", 1_000).unwrap();
        assert_eq!(pending(&pool, "probe"), 2);
    }

    #[test]
    fn stage_probe_show_fans_out_to_every_episode_file() {
        let pool = test_pool();
        seed_library(&pool);
        let conn = pool.get().unwrap();
        conn.execute(
            "INSERT INTO shows (id,library,title,added_at) VALUES ('s1','lib1','Show','now')",
            [],
        )
        .unwrap();
        for (id, season, ep) in [("e1", 1, 1), ("e2", 1, 2)] {
            conn.execute(
                &format!(
                    "INSERT INTO items (id,kind,title,container,library,show_id,season,episode,added_at) \
                     VALUES ('{id}','episode','E','mkv','lib1','s1',{season},{ep},'now')"
                ),
                [],
            )
            .unwrap();
            conn.execute(
                &format!("INSERT INTO files (id,item_id,abs_path) VALUES ('{id}f','{id}','/a/{id}.mkv')"),
                [],
            )
            .unwrap();
        }
        drop(conn);

        // A show-level probe fans out to each episode's files (one task per file).
        stage_probe(&pool, "show", "s1", 1_000).unwrap();
        assert_eq!(pending(&pool, "probe"), 2);
    }

    #[test]
    fn show_episodes_flattens_seasons_and_is_empty_for_unknown() {
        let pool = test_pool();
        assert!(show_episodes(&pool, "no-such-show").unwrap().is_empty());

        seed_library(&pool);
        let conn = pool.get().unwrap();
        conn.execute(
            "INSERT INTO shows (id,library,title,added_at) VALUES ('s1','lib1','Show','now')",
            [],
        )
        .unwrap();
        for (id, season, ep) in [("e1", 1, 1), ("e2", 1, 2), ("e3", 2, 1)] {
            // Test-controlled literal ints; no user input, so inline them.
            conn.execute(
                &format!(
                    "INSERT INTO items (id,kind,title,container,library,show_id,season,episode,added_at) \
                     VALUES ('{id}','episode','E','mkv','lib1','s1',{season},{ep},'now')"
                ),
                [],
            )
            .unwrap();
        }
        drop(conn);

        let eps = show_episodes(&pool, "s1").unwrap();
        assert_eq!(eps.len(), 3, "all episodes across both seasons are flattened");
    }

    // ----- SharedState-backed paths: the ledger effects of a whole-element or
    // per-stage reprocess. These enqueue tasks + invalidate caches but do NOT
    // trigger the drains (the pub `reprocess`/`stage_for` success path spawns real
    // ffmpeg stage drains via `state.jobs.trigger`, so only their pre-trigger
    // ledger effects and error branches are asserted here). ------------------------

    use crate::test_support;

    #[test]
    fn reprocess_item_movie_queues_all_five_stages() {
        let state = test_support::test_state();
        test_support::seed_movie(&state, "m1"); // one item + one file
        let mut subjects = 0usize;
        let stages = reprocess_item(&state, &state.db, "m1", now_ms(), &mut subjects).unwrap();
        // 1 file (probe) + storyboard + subtitles + metadata + embed = 5 subjects.
        assert_eq!(subjects, 5);
        assert_eq!(
            stages,
            vec![
                "pipeline.probe",
                "pipeline.metadata",
                "pipeline.storyboard",
                "pipeline.subtitles",
                "pipeline.embed",
            ]
        );
        for stage in ["probe", "metadata", "storyboard", "subtitles", "embed"] {
            assert_eq!(pending(&state.db, stage), 1, "{stage} queued one HIGH task");
        }
    }

    #[test]
    fn reprocess_item_episode_queues_probe_storyboard_subtitles_and_markers() {
        let state = test_support::test_state();
        test_support::seed_show_episode(&state, "sh1", "ep1");
        let mut subjects = 0usize;
        let stages = reprocess_item(&state, &state.db, "ep1", now_ms(), &mut subjects).unwrap();
        // 1 file (probe) + storyboard + subtitles + the season markers key.
        assert_eq!(subjects, 4);
        assert_eq!(
            stages,
            vec!["pipeline.probe", "pipeline.storyboard", "pipeline.subtitles", "pipeline.markers"]
        );
        assert_eq!(pending(&state.db, "markers"), 1, "the episode's season key is queued");
        // Episodes carry no item-level metadata/embed row.
        assert_eq!(pending(&state.db, "metadata"), 0);
        assert_eq!(pending(&state.db, "embed"), 0);
    }

    #[test]
    fn reprocess_item_unknown_id_errors() {
        let state = test_support::test_state();
        let mut subjects = 0usize;
        assert!(reprocess_item(&state, &state.db, "ghost", now_ms(), &mut subjects).is_err());
    }

    #[test]
    fn reprocess_show_queues_show_and_episode_subjects() {
        let state = test_support::test_state();
        test_support::seed_show_episode(&state, "sh1", "ep1");
        let mut subjects = 0usize;
        let stages = reprocess_show(&state, &state.db, "sh1", now_ms(), &mut subjects).unwrap();
        // metadata + embed (show) + markers (season) + probe + storyboard + subtitles (ep).
        assert_eq!(subjects, 6);
        assert_eq!(
            stages,
            vec![
                "pipeline.probe",
                "pipeline.metadata",
                "pipeline.storyboard",
                "pipeline.subtitles",
                "pipeline.markers",
                "pipeline.embed",
            ]
        );
        // Show-level metadata/embed target the show id; the rest target the episode.
        assert_eq!(pending(&state.db, "metadata"), 1);
        assert_eq!(pending(&state.db, "embed"), 1);
        assert_eq!(pending(&state.db, "markers"), 1);
        assert_eq!(pending(&state.db, "probe"), 1);
        assert_eq!(pending(&state.db, "storyboard"), 1);
        assert_eq!(pending(&state.db, "subtitles"), 1);
    }

    #[test]
    fn reprocess_show_unknown_id_errors() {
        let state = test_support::test_state();
        let mut subjects = 0usize;
        assert!(reprocess_show(&state, &state.db, "ghost", now_ms(), &mut subjects).is_err());
    }

    #[test]
    fn stage_storyboard_fans_show_to_episodes_and_targets_item_directly() {
        let state = test_support::test_state();
        test_support::seed_show_episode(&state, "sh1", "ep1");
        test_support::seed_movie(&state, "m1");
        // A show-level retry fans out to each episode (one storyboard task).
        stage_storyboard(&state, &state.db, "show", "sh1", now_ms()).unwrap();
        assert_eq!(pending(&state.db, "storyboard"), 1);
        // An item-level retry queues that item directly (now two distinct subjects).
        stage_storyboard(&state, &state.db, "item", "m1", now_ms()).unwrap();
        assert_eq!(pending(&state.db, "storyboard"), 2);
    }

    #[test]
    fn stage_subtitles_fans_show_to_episodes_and_targets_item_directly() {
        let state = test_support::test_state();
        test_support::seed_show_episode(&state, "sh1", "ep1");
        test_support::seed_movie(&state, "m1");
        stage_subtitles(&state, &state.db, "show", "sh1", now_ms()).unwrap();
        assert_eq!(pending(&state.db, "subtitles"), 1);
        stage_subtitles(&state, &state.db, "item", "m1", now_ms()).unwrap();
        assert_eq!(pending(&state.db, "subtitles"), 2);
    }

    #[test]
    fn reprocess_and_stage_for_reject_unknown_kinds_before_triggering() {
        let state = test_support::test_state();
        // Unknown element kind / unknown stage bail before any drain is triggered.
        assert!(reprocess(&state, "bogus", "x").is_err());
        assert!(stage_for(&state, "item", "x", "bogus-stage").is_err());
    }
}
