//! The element-centric pipeline view: the whole catalog (films / series /
//! episodes) with, per element, the status of each treatment applied to it and an
//! overall roll-up, filtered / searched / paginated + full-catalog counts.
//!
//! Computed in BULK from cheap signals so it scales to thousands of items: a few
//! set / map queries (probed items, items with markers, items with a vector, each
//! stage's ledger tasks) + LEAN item/show rows (poster/genre/has-metadata pulled
//! from the JSON via `json_extract`, never a full metadata deserialize) folded in
//! memory. No per-item disk stats. The ledger overlays running/failed/pending
//! (with the error); `storyboard`/`markers` are assumed done when no ledger task
//! exists (their absence isn't cheaply detectable and shouldn't spam the view).

use std::collections::{HashMap, HashSet};

use anyhow::Result;

use crate::db;
use crate::db::pipeline::RawItem;
use crate::model::{ElementCounts, ElementRow, EpStats, PipelineElements, Treatment};
use crate::state::SharedState;

/// Query for [`list`].
pub struct Filter {
    /// `"all" | "attention" | "ok" | "pending" | "running" | "failed"`.
    pub status: String,
    /// `"all" | "film" | "series" | "episode"`.
    pub kind: String,
    pub query: String,
    pub page: i64,
    pub limit: i64,
}

type Ledger = HashMap<String, (String, Option<String>)>;

fn tr(key: &str, status: &str, error: Option<String>) -> Treatment {
    Treatment { key: key.to_string(), status: status.to_string(), error }
}

/// The single source of truth for "what status does this (subject, stage) show".
/// Shared by the elements list ([`status_of`]) and the per-element drawer
/// (`crate::api::admin::pipeline::combine`) so the two never disagree: a present
/// ledger state always wins; with no ledger row, `artifact_done`/`assume_done`
/// decide done vs pending (there is deliberately no "missing" any stage the list
/// assumes done when unledgered must read the same in the drawer).
pub fn resolve_status(ledger_status: Option<&str>, artifact_done: bool, assume_done: bool) -> &'static str {
    match ledger_status {
        Some("failed") => "failed",
        Some("running") => "running",
        Some("pending") => "pending",
        Some(_) => "done",
        None if artifact_done || assume_done => "done",
        None => "pending",
    }
}

fn status_of(
    ledger: Option<&(String, Option<String>)>,
    artifact_done: bool,
    assume_done: bool,
) -> (&'static str, Option<String>) {
    let status = resolve_status(ledger.map(|(s, _)| s.as_str()), artifact_done, assume_done);
    let err = if status == "failed" { ledger.and_then(|(_, e)| e.clone()) } else { None };
    (status, err)
}

fn overall_of(treatments: &[Treatment]) -> &'static str {
    let mut fail = false;
    let mut run = false;
    let mut pend = false;
    for t in treatments {
        match t.status.as_str() {
            "failed" => fail = true,
            "running" => run = true,
            "pending" | "missing" => pend = true,
            _ => {}
        }
    }
    if fail {
        "failed"
    } else if run {
        "running"
    } else if pend {
        "pending"
    } else {
        "ok"
    }
}

fn element_title(it: &RawItem) -> String {
    if it.kind != "episode" {
        return it.title.clone();
    }
    let show = it.show_title.clone().unwrap_or_default();
    let code = match (it.season, it.episode) {
        (Some(s), Some(e)) => format!("S{s:02}E{e:02}"),
        _ => String::new(),
    };
    let base = match (show.is_empty(), code.is_empty()) {
        (false, false) => format!("{show} - {code}"),
        (false, true) => show,
        (true, false) => code,
        (true, true) => it.title.clone(),
    };
    match &it.episode_title {
        Some(t) if !t.is_empty() => format!("{base} « {t} »"),
        _ => base,
    }
}

/// Build the filtered, paginated element page + full-catalog counts.
pub fn list(state: &SharedState, f: &Filter) -> Result<PipelineElements> {
    let db = &state.db;
    let items = db::pipeline::raw_items(db)?;
    let shows = db::pipeline::raw_shows(db)?;
    let probed: HashSet<String> = db::probed_item_ids(db)?;
    let markset: HashSet<String> = db::item_ids_with_markers(db)?;
    let vecset: HashSet<String> = db::item_ids_with_vector(db)?;
    let meta_l: Ledger = db::pipeline::stage_statuses(db, "metadata")?;
    let story_l: Ledger = db::pipeline::stage_statuses(db, "storyboard")?;
    let subs_l: Ledger = db::pipeline::stage_statuses(db, "subtitles")?;
    let embed_l: Ledger = db::pipeline::stage_statuses(db, "embed")?;
    let mark_l: Ledger = db::pipeline::stage_statuses(db, "markers")?;

    // Episodes grouped by show (for the series aggregate + posters).
    let ep_by_show = group_episodes_by_show(&items);
    let show_poster: HashMap<&str, Option<String>> =
        shows.iter().map(|s| (s.id.as_str(), s.poster.clone())).collect();

    let lg = Ledgers {
        probed: &probed,
        markset: &markset,
        vecset: &vecset,
        meta_l: &meta_l,
        story_l: &story_l,
        subs_l: &subs_l,
        embed_l: &embed_l,
        mark_l: &mark_l,
        show_poster: &show_poster,
    };

    let mut all: Vec<ElementRow> = Vec::with_capacity(items.len() + shows.len());

    for it in &items {
        let (treatments, kind, poster) = item_treatments(it, &lg);
        let overall = overall_of(&treatments).to_string();
        all.push(ElementRow {
            id: it.id.clone(),
            kind: kind.to_string(),
            title: element_title(it),
            poster,
            year: it.year.map(|y| y as u32),
            genre: it.genre.clone(),
            duration_ms: it.duration_ms.map(|d| d as u64),
            season_count: None,
            treatments,
            overall,
            ep_stats: None,
        });
    }

    for sh in &shows {
        let (m, me) = status_of(meta_l.get(&sh.id), sh.has_meta, false);
        let (e, ee) = status_of(embed_l.get(&sh.id), vecset.contains(&sh.id), false);
        let treatments = vec![tr("metadata", m, me), tr("embed", e, ee)];
        let eps = ep_by_show.get(sh.id.as_str()).cloned().unwrap_or_default();
        let seasons: HashSet<i64> = eps.iter().filter_map(|e| e.season).collect();
        let marker_seasons = seasons
            .iter()
            .filter(|n| mark_l.get(&format!("{}#{}", sh.id, n)).map(|(s, _)| s == "done").unwrap_or(false))
            .count() as i64;
        let ep_stats = EpStats {
            episodes: eps.len() as i64,
            probed: eps.iter().filter(|e| probed.contains(&e.id)).count() as i64,
            storyboarded: eps
                .iter()
                .filter(|e| story_l.get(&e.id).map(|(s, _)| s == "done").unwrap_or(false))
                .count() as i64,
            seasons: seasons.len() as i64,
            marker_seasons,
        };
        let overall = overall_of(&treatments).to_string();
        all.push(ElementRow {
            id: sh.id.clone(),
            kind: "series".to_string(),
            title: sh.title.clone(),
            poster: sh.poster.clone(),
            year: sh.year.map(|y| y as u32),
            genre: sh.genre.clone(),
            duration_ms: None,
            season_count: Some(seasons.len() as u32),
            treatments,
            overall,
            ep_stats: Some(ep_stats),
        });
    }

    // Counts over the full (unfiltered) set.
    let counts = tally_counts(&all);

    // Filter.
    let q = f.query.trim().to_lowercase();
    let filtered: Vec<&ElementRow> =
        all.iter().filter(|el| matches_filter(el, f, &q)).collect();

    let total = filtered.len() as i64;
    let limit = f.limit.clamp(1, 100);
    let pages = ((total + limit - 1) / limit).max(1);
    let page = f.page.clamp(0, pages - 1);
    let start = (page * limit) as usize;
    let elements: Vec<ElementRow> =
        filtered.into_iter().skip(start).take(limit as usize).cloned().collect();

    Ok(PipelineElements { total, page, pages, counts, elements })
}

/// The bulk lookups shared while building each element's per-treatment status,
/// bundled so the row builders take one context instead of a dozen params.
struct Ledgers<'a, 'k> {
    probed: &'a HashSet<String>,
    markset: &'a HashSet<String>,
    vecset: &'a HashSet<String>,
    meta_l: &'a Ledger,
    story_l: &'a Ledger,
    subs_l: &'a Ledger,
    embed_l: &'a Ledger,
    mark_l: &'a Ledger,
    show_poster: &'a HashMap<&'k str, Option<String>>,
}

/// Episodes grouped by their show id (for the series aggregate + posters).
fn group_episodes_by_show(items: &[RawItem]) -> HashMap<&str, Vec<&RawItem>> {
    let mut ep_by_show: HashMap<&str, Vec<&RawItem>> = HashMap::new();
    for it in items {
        if it.kind == "episode" {
            if let Some(sid) = &it.show_id {
                ep_by_show.entry(sid.as_str()).or_default().push(it);
            }
        }
    }
    ep_by_show
}

/// The `(treatments, kind, poster)` for one item row: an episode gets probe /
/// storyboard / subtitles / markers; a film/video gets probe / metadata /
/// storyboard / subtitles / embed.
fn item_treatments(it: &RawItem, lg: &Ledgers<'_, '_>) -> (Vec<Treatment>, &'static str, Option<String>) {
    if it.kind == "episode" {
        let (p, _) = status_of(None, lg.probed.contains(&it.id), false);
        let (s, se) = status_of(lg.story_l.get(&it.id), false, true);
        let season_key = match (it.show_id.as_deref(), it.season) {
            (Some(sh), Some(n)) => Some(format!("{sh}#{n}")),
            _ => None,
        };
        let (mk, mke) = status_of(
            season_key.as_deref().and_then(|k| lg.mark_l.get(k)),
            lg.markset.contains(&it.id),
            true,
        );
        let (st, ste) = status_of(lg.subs_l.get(&it.id), false, true);
        let poster = it.show_id.as_deref().and_then(|sid| lg.show_poster.get(sid).cloned().flatten());
        (
            vec![
                tr("probe", p, None),
                tr("storyboard", s, se),
                tr("subtitles", st, ste),
                tr("markers", mk, mke),
            ],
            "episode",
            poster,
        )
    } else {
        let (p, _) = status_of(None, lg.probed.contains(&it.id), false);
        let (m, me) = status_of(lg.meta_l.get(&it.id), it.has_meta, false);
        let (s, se) = status_of(lg.story_l.get(&it.id), false, true);
        let (st, ste) = status_of(lg.subs_l.get(&it.id), false, true);
        let (e, ee) = status_of(lg.embed_l.get(&it.id), lg.vecset.contains(&it.id), false);
        (
            vec![
                tr("probe", p, None),
                tr("metadata", m, me),
                tr("storyboard", s, se),
                tr("subtitles", st, ste),
                tr("embed", e, ee),
            ],
            "film",
            it.poster.clone(),
        )
    }
}

/// Roll up per-status and per-kind counts over the full (unfiltered) set.
fn tally_counts(all: &[ElementRow]) -> ElementCounts {
    let mut counts = ElementCounts { total: all.len() as i64, ..Default::default() };
    for el in all {
        match el.overall.as_str() {
            "ok" => counts.ok += 1,
            "pending" => counts.pending += 1,
            "running" => counts.running += 1,
            "failed" => counts.failed += 1,
            _ => {}
        }
        match el.kind.as_str() {
            "film" => counts.film += 1,
            "series" => counts.series += 1,
            "episode" => counts.episode += 1,
            _ => {}
        }
    }
    counts
}

/// Whether one row passes the query / kind / status filter (`q` is already
/// trimmed + lowercased).
fn matches_filter(el: &ElementRow, f: &Filter, q: &str) -> bool {
    if !q.is_empty() && !el.title.to_lowercase().contains(q) {
        return false;
    }
    if f.kind != "all" && el.kind != f.kind {
        return false;
    }
    match f.status.as_str() {
        "all" => true,
        "attention" => el.overall != "ok",
        other => el.overall == other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw_item(kind: &str) -> RawItem {
        RawItem {
            id: "id1".into(),
            kind: kind.into(),
            title: "Some Title".into(),
            year: Some(2020),
            duration_ms: Some(6_000_000),
            show_id: None,
            show_title: None,
            season: None,
            episode: None,
            episode_title: None,
            has_meta: false,
            poster: None,
            genre: None,
        }
    }

    fn row(kind: &str, overall: &str) -> ElementRow {
        ElementRow {
            id: "id1".into(),
            kind: kind.into(),
            title: "The Matrix".into(),
            poster: None,
            year: None,
            genre: None,
            duration_ms: None,
            season_count: None,
            treatments: Vec::new(),
            overall: overall.into(),
            ep_stats: None,
        }
    }

    #[test]
    fn resolve_status_ledger_wins_then_artifacts() {
        // A present ledger state always wins over artifact flags.
        assert_eq!(resolve_status(Some("failed"), true, true), "failed");
        assert_eq!(resolve_status(Some("running"), false, false), "running");
        assert_eq!(resolve_status(Some("pending"), true, false), "pending");
        // Any other ledger value (e.g. "done") reads done.
        assert_eq!(resolve_status(Some("done"), false, false), "done");
        // No ledger: artifact_done OR assume_done => done, else pending.
        assert_eq!(resolve_status(None, true, false), "done");
        assert_eq!(resolve_status(None, false, true), "done");
        assert_eq!(resolve_status(None, false, false), "pending");
    }

    #[test]
    fn status_of_surfaces_error_only_when_failed() {
        let failed = ("failed".to_string(), Some("boom".to_string()));
        assert_eq!(status_of(Some(&failed), false, false), ("failed", Some("boom".to_string())));
        // A running ledger with a stale error message does not leak the error.
        let running = ("running".to_string(), Some("stale".to_string()));
        assert_eq!(status_of(Some(&running), false, false), ("running", None));
        // No ledger, artifact present -> done, no error.
        assert_eq!(status_of(None, true, false), ("done", None));
    }

    #[test]
    fn overall_of_precedence_failed_running_pending_ok() {
        let mk = |s: &str| tr("x", s, None);
        assert_eq!(overall_of(&[mk("done"), mk("done")]), "ok");
        assert_eq!(overall_of(&[mk("done"), mk("pending")]), "pending");
        assert_eq!(overall_of(&[mk("missing"), mk("done")]), "pending");
        assert_eq!(overall_of(&[mk("pending"), mk("running")]), "running");
        assert_eq!(overall_of(&[mk("running"), mk("failed")]), "failed");
        assert_eq!(overall_of(&[]), "ok");
    }

    #[test]
    fn element_title_movie_is_plain_title() {
        assert_eq!(element_title(&raw_item("film")), "Some Title");
    }

    #[test]
    fn element_title_episode_composes_show_code_and_name() {
        let mut ep = raw_item("episode");
        ep.show_title = Some("Breaking Bad".into());
        ep.season = Some(2);
        ep.episode = Some(5);
        ep.episode_title = Some("Breakage".into());
        assert_eq!(element_title(&ep), "Breaking Bad - S02E05 « Breakage »");

        // No episode title -> just "show - code".
        ep.episode_title = None;
        assert_eq!(element_title(&ep), "Breaking Bad - S02E05");

        // Missing season/episode -> just the show.
        ep.season = None;
        ep.episode = None;
        assert_eq!(element_title(&ep), "Breaking Bad");

        // No show + no code -> falls back to the raw title.
        ep.show_title = None;
        assert_eq!(element_title(&ep), "Some Title");
    }

    #[test]
    fn group_episodes_by_show_keys_on_show_id() {
        let mut a = raw_item("episode");
        a.id = "a".into();
        a.show_id = Some("show1".into());
        let mut b = raw_item("episode");
        b.id = "b".into();
        b.show_id = Some("show1".into());
        let mut movie = raw_item("film");
        movie.id = "m".into();
        // An episode with no show_id is dropped.
        let mut orphan = raw_item("episode");
        orphan.id = "o".into();
        orphan.show_id = None;

        let items = vec![a, b, movie, orphan];
        let grouped = group_episodes_by_show(&items);
        assert_eq!(grouped.len(), 1);
        assert_eq!(grouped.get("show1").map(|v| v.len()), Some(2));
    }

    #[test]
    fn tally_counts_rolls_up_status_and_kind() {
        let all = vec![
            row("film", "ok"),
            row("film", "failed"),
            row("series", "pending"),
            row("episode", "running"),
        ];
        let c = tally_counts(&all);
        assert_eq!(c.total, 4);
        assert_eq!((c.ok, c.failed, c.pending, c.running), (1, 1, 1, 1));
        assert_eq!((c.film, c.series, c.episode), (2, 1, 1));
    }

    #[test]
    fn item_treatments_film_carries_five_stages_with_ledger_and_artifacts() {
        use std::collections::{HashMap, HashSet};
        let probed: HashSet<String> = ["m1".to_string()].into_iter().collect();
        let markset: HashSet<String> = HashSet::new();
        let vecset: HashSet<String> = ["m1".to_string()].into_iter().collect();
        let empty: Ledger = HashMap::new();
        // A failed metadata ledger row surfaces its error; other stages fall back
        // to artifact/assume-done.
        let meta_l: Ledger =
            [("m1".to_string(), ("failed".to_string(), Some("boom".to_string())))].into_iter().collect();
        let show_poster: HashMap<&str, Option<String>> = HashMap::new();
        let lg = Ledgers {
            probed: &probed,
            markset: &markset,
            vecset: &vecset,
            meta_l: &meta_l,
            story_l: &empty,
            subs_l: &empty,
            embed_l: &empty,
            mark_l: &empty,
            show_poster: &show_poster,
        };

        let mut movie = raw_item("film");
        movie.id = "m1".into();
        movie.poster = Some("p.jpg".into());
        let (treatments, kind, poster) = item_treatments(&movie, &lg);
        assert_eq!(kind, "film");
        assert_eq!(poster.as_deref(), Some("p.jpg"));
        // Film row: probe / metadata / storyboard / subtitles / embed.
        let keys: Vec<&str> = treatments.iter().map(|t| t.key.as_str()).collect();
        assert_eq!(keys, vec!["probe", "metadata", "storyboard", "subtitles", "embed"]);
        let by = |k: &str| treatments.iter().find(|t| t.key == k).unwrap();
        assert_eq!(by("probe").status, "done"); // in probed set
        assert_eq!(by("metadata").status, "failed");
        assert_eq!(by("metadata").error.as_deref(), Some("boom"));
        assert_eq!(by("embed").status, "done"); // in vecset
        assert_eq!(by("storyboard").status, "done"); // no ledger, assume_done
    }

    #[test]
    fn item_treatments_episode_has_four_stages_and_show_poster() {
        use std::collections::{HashMap, HashSet};
        let probed: HashSet<String> = HashSet::new();
        let markset: HashSet<String> = HashSet::new();
        let vecset: HashSet<String> = HashSet::new();
        let empty: Ledger = HashMap::new();
        let show_poster: HashMap<&str, Option<String>> =
            [("s1", Some("show.jpg".to_string()))].into_iter().collect();
        let lg = Ledgers {
            probed: &probed,
            markset: &markset,
            vecset: &vecset,
            meta_l: &empty,
            story_l: &empty,
            subs_l: &empty,
            embed_l: &empty,
            mark_l: &empty,
            show_poster: &show_poster,
        };

        let mut ep = raw_item("episode");
        ep.id = "e1".into();
        ep.show_id = Some("s1".into());
        ep.season = Some(1);
        let (treatments, kind, poster) = item_treatments(&ep, &lg);
        assert_eq!(kind, "episode");
        // The episode inherits its show's poster.
        assert_eq!(poster.as_deref(), Some("show.jpg"));
        let keys: Vec<&str> = treatments.iter().map(|t| t.key.as_str()).collect();
        assert_eq!(keys, vec!["probe", "storyboard", "subtitles", "markers"]);
        let by = |k: &str| treatments.iter().find(|t| t.key == k).unwrap();
        // No ledger + no probe artifact -> probe pending; storyboard/subtitles/markers assume done.
        assert_eq!(by("probe").status, "pending");
        assert_eq!(by("storyboard").status, "done");
        assert_eq!(by("markers").status, "done");
    }

    #[test]
    fn matches_filter_query_kind_and_status() {
        let f = |status: &str, kind: &str, query: &str| Filter {
            status: status.into(),
            kind: kind.into(),
            query: query.into(),
            page: 0,
            limit: 50,
        };
        let el = row("film", "failed"); // title "The Matrix"

        // Query is a case-insensitive substring of the title (already lowercased by caller).
        assert!(matches_filter(&el, &f("all", "all", ""), ""));
        assert!(matches_filter(&el, &f("all", "all", "matrix"), "matrix"));
        assert!(!matches_filter(&el, &f("all", "all", "inception"), "inception"));

        // Kind filter.
        assert!(!matches_filter(&el, &f("all", "series", ""), ""));
        assert!(matches_filter(&el, &f("all", "film", ""), ""));

        // Status filter: exact match, plus "attention" = anything not ok.
        assert!(matches_filter(&el, &f("failed", "all", ""), ""));
        assert!(!matches_filter(&el, &f("ok", "all", ""), ""));
        assert!(matches_filter(&el, &f("attention", "all", ""), ""));
        assert!(!matches_filter(&row("film", "ok"), &f("attention", "all", ""), ""));
    }
}
