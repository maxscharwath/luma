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

    let is_ep = |it: &RawItem| it.kind == "episode";

    // Episodes grouped by show (for the series aggregate + posters).
    let mut ep_by_show: HashMap<&str, Vec<&RawItem>> = HashMap::new();
    for it in &items {
        if is_ep(it) {
            if let Some(sid) = &it.show_id {
                ep_by_show.entry(sid.as_str()).or_default().push(it);
            }
        }
    }
    let show_poster: HashMap<&str, Option<String>> =
        shows.iter().map(|s| (s.id.as_str(), s.poster.clone())).collect();

    let mut all: Vec<ElementRow> = Vec::with_capacity(items.len() + shows.len());

    for it in &items {
        let (treatments, kind, poster) = if is_ep(it) {
            let (p, _) = status_of(None, probed.contains(&it.id), false);
            let (s, se) = status_of(story_l.get(&it.id), false, true);
            let season_key = match (it.show_id.as_deref(), it.season) {
                (Some(sh), Some(n)) => Some(format!("{sh}#{n}")),
                _ => None,
            };
            let (mk, mke) = status_of(
                season_key.as_deref().and_then(|k| mark_l.get(k)),
                markset.contains(&it.id),
                true,
            );
            let (st, ste) = status_of(subs_l.get(&it.id), false, true);
            let poster = it.show_id.as_deref().and_then(|sid| show_poster.get(sid).cloned().flatten());
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
            let (p, _) = status_of(None, probed.contains(&it.id), false);
            let (m, me) = status_of(meta_l.get(&it.id), it.has_meta, false);
            let (s, se) = status_of(story_l.get(&it.id), false, true);
            let (st, ste) = status_of(subs_l.get(&it.id), false, true);
            let (e, ee) = status_of(embed_l.get(&it.id), vecset.contains(&it.id), false);
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
        };
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
    let mut counts = ElementCounts { total: all.len() as i64, ..Default::default() };
    for el in &all {
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

    // Filter.
    let q = f.query.trim().to_lowercase();
    let filtered: Vec<&ElementRow> = all
        .iter()
        .filter(|el| {
            if !q.is_empty() && !el.title.to_lowercase().contains(&q) {
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
        })
        .collect();

    let total = filtered.len() as i64;
    let limit = f.limit.clamp(1, 100);
    let pages = ((total + limit - 1) / limit).max(1);
    let page = f.page.clamp(0, pages - 1);
    let start = (page * limit) as usize;
    let elements: Vec<ElementRow> =
        filtered.into_iter().skip(start).take(limit as usize).cloned().collect();

    Ok(PipelineElements { total, page, pages, counts, elements })
}
