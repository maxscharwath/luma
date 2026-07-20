//! The DB half of the hero pick: the candidate catalog, the viewer's "already
//! seen" set and the household trending rank. Every read is best-effort, but
//! never *silently* so: a failure degrades the signal and logs, because an
//! unreadable catalog and an empty one look identical to the client.

use std::collections::{HashMap, HashSet};

use crate::db::{self, Pool};
use crate::model::SectionItem;

/// How deep the trending rank signal looks.
const TREND_DEPTH: usize = 50;

/// Everything the hero can be picked from: every movie and every show (episodes
/// are never a hero, they belong to their show's page).
pub(super) fn catalog(pool: &Pool) -> Vec<SectionItem> {
    let mut all: Vec<SectionItem> = Vec::new();
    match db::list_movies(pool, None) {
        Ok(movies) => {
            all.extend(movies.into_iter().map(|m| SectionItem::Movie { item: Box::new(m) }));
        }
        Err(e) => tracing::error!(target: "sections", "featured: reading movies failed: {e:#}"),
    }
    match db::list_shows(pool, None) {
        Ok(shows) => {
            all.extend(shows.into_iter().map(|s| SectionItem::Show { show: Box::new(s) }));
        }
        Err(e) => tracing::error!(target: "sections", "featured: reading shows failed: {e:#}"),
    }
    all
}

/// Everything the user already has in flight or behind them: explicit watched
/// marks, resume positions, the `recent` plays the caller already read for the
/// taste centroid, plus the parent show of any episode in those sets (the show
/// belongs to "Continue watching", not the hero).
pub(super) fn seen_ids(pool: &Pool, user_id: &str, recent: &[String]) -> HashSet<String> {
    let mut out: HashSet<String> =
        db::list_watched(pool, user_id).unwrap_or_default().into_iter().collect();
    out.extend(db::list_progress(pool, user_id).unwrap_or_default().into_iter().map(|p| p.item_id));
    out.extend(recent.iter().cloned());
    // Project the (already deduplicated) ids onto their parent shows with a lean
    // `show_id` query: hydrating a whole watch history to read one column costs
    // the metadata blob plus a files/markers batch for nothing.
    let parents = {
        let refs: Vec<&str> = out.iter().map(String::as_str).collect();
        db::show_ids_for(pool, &refs)
    };
    match parents {
        Ok(shows) => out.extend(shows),
        Err(e) => tracing::warn!(target: "sections", "featured: parent shows failed: {e:#}"),
    }
    out
}

/// Trending rank -> 0..1 score (top of the list ~1.0, absent 0.0). Goes through
/// the shared [`crate::services::sections::trending_ids`] so a failing query is
/// logged instead of silently zeroing this signal for every candidate.
pub(super) fn trend_scores(pool: &Pool) -> HashMap<String, f32> {
    rank_scores(super::super::trending_ids(pool, TREND_DEPTH))
}

/// Rank -> score: first place scores 1.0 and the tail scales linearly down to
/// `1/len`, so the signal reads the same in a library of any size.
fn rank_scores(ids: Vec<String>) -> HashMap<String, f32> {
    let len = ids.len() as f32;
    ids.into_iter().enumerate().map(|(i, id)| (id, (len - i as f32) / len)).collect()
}

#[cfg(test)]
mod tests {
    use super::super::fixtures::seed_user;
    use super::*;
    use crate::test_support;

    #[test]
    fn catalog_gathers_movies_and_shows_but_not_episodes() {
        let state = test_support::test_state();
        test_support::seed_movie(&state, "m1");
        test_support::seed_show_episode(&state, "sh1", "e1");
        let ids: Vec<String> = catalog(&state.db).iter().map(|e| e.id().to_string()).collect();
        assert!(ids.contains(&"m1".to_string()));
        assert!(ids.contains(&"sh1".to_string()));
        assert!(!ids.contains(&"e1".to_string()));
    }

    #[test]
    fn catalog_is_empty_on_a_bare_db() {
        let state = test_support::test_state();
        assert!(catalog(&state.db).is_empty());
    }

    #[test]
    fn seen_ids_folds_history_and_parent_shows() {
        let state = test_support::test_state();
        test_support::seed_movie(&state, "m1");
        let (show, ep) = test_support::seed_show_episode(&state, "sh1", "e1");
        seed_user(&state, "u1");
        db::mark_watched(&state.db, "u1", &ep).unwrap();

        // The watched episode drags its whole show into the exclusion set.
        let seen = seen_ids(&state.db, "u1", &[]);
        assert!(seen.contains(&ep) && seen.contains(&show));
        assert!(!seen.contains("m1"));

        // Recent plays come from the caller and land in the same set (a movie has
        // no parent, so it adds nothing else).
        let seen = seen_ids(&state.db, "u1", &["m1".to_string()]);
        assert!(seen.contains("m1"));
    }

    #[test]
    fn rank_scores_normalize_the_trending_rank_to_0_1() {
        let scores = rank_scores(vec!["hot".into(), "warm".into(), "cool".into()]);
        // Top of the list scores 1.0, the tail scales down by rank, and anything
        // never played is simply absent (the blend reads that as 0.0).
        assert_eq!(scores.get("hot").copied(), Some(1.0));
        assert!(scores["warm"] < 1.0 && scores["cool"] < scores["warm"]);
        assert!(scores["cool"] > 0.0);
        assert!(!scores.contains_key("ghost"));
        // An empty trending list yields no scores (and no division by zero).
        assert!(rank_scores(Vec::new()).is_empty());
    }
}
