//! The automatic wanted-list search: what makes an approved request download
//! itself. Runs as the `acquisition.search` job (cron + on-approve trigger):
//! due wanted rows -> per-request targets (season packs first) -> indexer
//! sweep -> best accepted release above zero -> grab. Every due row gets its
//! `last_search_at` stamped whatever happens, so retries rotate fairly.

use std::collections::HashSet;

use anyhow::Result;

use kroma_module_sdk::engine::services::requests::today_ymd;
use kroma_module_sdk::db;

use crate::search::{score_release, targets_for_wanted, wanted_ids_by};

/// How many wanted rows one pass considers (bounds runtime; the cron loops).
const BATCH: usize = 40;
/// How many distinct requests one pass searches.
const MAX_REQUESTS: usize = 5;

#[derive(Debug, Default)]
pub struct AutoSummary {
    pub requests: usize,
    pub targets: usize,
    pub grabbed: usize,
    pub errors: Vec<String>,
}

pub fn auto_search_pass<S: kroma_module_sdk::host::HostCtx>(state: &S, log: &dyn Fn(String), cancelled: &dyn Fn() -> bool) -> Result<AutoSummary> {
    let mut summary = AutoSummary::default();
    if !state.setting_bool("acqEnabled", false) {
        log("automatic acquisition is disabled (acqEnabled)".into());
        return Ok(summary);
    }
    if !crate::downloads(state).gate_open() {
        log("VPN kill switch is closed; skipping the search pass".into());
        return Ok(summary);
    }

    let conn = state.db().get()?;
    let due = db::wanted_searchable(&conn, &today_ymd(), BATCH)?;
    let indexers = kroma_module_sdk::host::resolve_port::<dyn kroma_module_sdk::ports::IndexerDbPort>(state).ok_or_else(|| anyhow::anyhow!("indexer module unavailable"))?.enabled_indexers(state)?;
    drop(conn);
    if due.is_empty() {
        log("nothing wanted right now".into());
        return Ok(summary);
    }
    if indexers.is_empty() {
        log("no enabled indexer; configure one under Indexeurs".into());
        return Ok(summary);
    }

    let mut request_ids: Vec<String> = Vec::new();
    for w in &due {
        if !request_ids.contains(&w.request_id) {
            request_ids.push(w.request_id.clone());
        }
    }
    request_ids.truncate(MAX_REQUESTS);
    let profile = crate::profile_from_settings(state);

    for request_id in &request_ids {
        if cancelled() {
            break;
        }
        let conn = state.db().get()?;
        let Some(req) = db::get_request(&conn, request_id)? else { continue };
        let wanted = db::wanted_for_request(&conn, request_id)?;
        drop(conn);

        summary.requests += 1;
        let targets = targets_for_wanted(req.kind, &wanted, &today_ymd());
        // Rows a pack grab already covered this pass (skip episode targets).
        let mut covered: HashSet<String> = HashSet::new();
        for st in &targets {
            if cancelled() {
                break;
            }
            let target_rows = wanted_row_ids(&wanted, st);
            if target_rows.is_empty() || target_rows.iter().all(|id| covered.contains(id)) {
                continue;
            }
            summary.targets += 1;

            // Sweep the indexers for this target and keep the best candidate.
            let mut best: Option<(crate::search::CachedRelease, i32)> = None;
            for indexer in &indexers {
                let found = match crate::search_indexer(state, indexer, &st.query) {
                    Ok(f) => f,
                    Err(e) => {
                        summary.errors.push(format!("{}: {e:#}", indexer.name));
                        continue;
                    }
                };
                for release in found {
                    let view = score_release(&release, indexer, st, &profile);
                    let Some(score) = view.score else { continue };
                    let magnet_or_url =
                        release.magnet.clone().or_else(|| release.link.clone()).unwrap_or_default();
                    if magnet_or_url.is_empty() {
                        continue;
                    }
                    if best.as_ref().is_none_or(|(_, s)| score > *s) {
                        best = Some((
                            crate::search::CachedRelease { view, magnet_or_url, tmdb_id: req.tmdb_id },
                            score,
                        ));
                    }
                }
            }

            if let Some((candidate, score)) = best {
                log(format!(
                    "grabbing \"{}\" (score {score}) for \"{}\"",
                    candidate.view.title, req.title
                ));
                let spec = crate::search::grab_spec_from_release(
                    &candidate.view,
                    &candidate.magnet_or_url,
                    candidate.tmdb_id,
                    Some(req.title.clone()),
                    req.year,
                    Some(request_id.clone()),
                    target_rows.clone(),
                );
                match crate::downloads(state).grab(state, spec) {
                    Ok(row) => {
                        // Background job: fine to add synchronously here.
                        crate::downloads(state).activate(state, &row);
                        summary.grabbed += 1;
                        covered.extend(target_rows);
                    }
                    Err(e) => summary.errors.push(format!("grab failed: {e:#}")),
                }
            }
        }
    }

    // Stamp every due row so the next pass rotates to the least recently
    // searched, grabbed or not.
    let stamp: Vec<String> = due.iter().map(|w| w.id.clone()).collect();
    db::stamp_wanted_searched(state.db(), &stamp, kroma_module_sdk::engine::services::jobs::now_ms())?;
    Ok(summary)
}

fn wanted_row_ids(wanted: &[db::WantedRow], st: &crate::search::SearchTarget) -> Vec<String> {
    // Reuse the coverage rule the grab path uses, driven by the target shape.
    wanted_ids_by(wanted, st.kind, st.season, st.episodes.as_deref())
        .into_iter()
        .filter(|id| wanted.iter().any(|w| &w.id == id && w.status == "wanted"))
        .collect()
}
