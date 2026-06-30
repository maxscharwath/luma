//! `sections.curate` the editorial curation job: deterministic director
//! collections (from crew metadata) + LLM-curated genre/list/franchise/decade/
//! mood rows, tool-driven when the provider can function-call, else a
//! catalog-in-prompt fallback. Members are grounded to the real catalog either
//! way. The curation *logic* lives in `services::sections::curate`; this is the
//! job that orchestrates it.

use super::prelude::*;

/// How many catalog titles to hand the model when curating editorial collections
/// (highest-rated/most-recent first), bounding the prompt's token budget. Only
/// used by the catalog-in-prompt fallback the tool-driven path queries instead.
const MAX_CURATE_CATALOG: usize = 600;

/// Max model turns in the tool-driven curate loop. A model that batches tool
/// calls finishes in a handful of turns; a one-call-per-turn model needs ~2
/// (genres+people) + up to `MAX_LLM` (14, in `curate.rs`) `find_titles` + a few
/// `get_title` + the final JSON reply, so 24 keeps real headroom before it bails.
const MAX_TOOL_STEPS: usize = 24;

pub(super) fn run(ctx: &JobContext) -> Result<()> {
    use crate::infra::events::ServerEvent;
    use crate::services::sections::curate;
    let state = &ctx.state;

    let items = crate::db::list_items(&state.db, None)?;
    let shows = crate::db::list_shows(&state.db, None)?;
    let catalog = curate::build_catalog(&items, &shows);
    let movies = items.iter().filter(|i| !matches!(i.kind, crate::model::Kind::Episode)).count();
    ctx.info(format!("catalog: {} entries ({movies} movies/videos + {} shows)", catalog.len(), shows.len()));

    // 1) Deterministic director collections (accurate, from crew metadata).
    let director_rows = curate::director_collections(&catalog);
    ctx.debug(format!("director collections: {}", director_rows.len()));
    if director_rows.is_empty() {
        ctx.debug("no director collections crew metadata may be missing; re-run metadata.enrich");
    }

    // 2) LLM editorial collections tool-driven when the provider supports
    //    function calling (the model queries the library directly), else the
    //    catalog-in-prompt fallback.
    let mut llm_rows = Vec::new();
    let llm = crate::infra::llm::from_settings(&state.settings);
    if llm.available() {
        // Curating many collections × many members is a large reply far bigger
        // than the per-user naming task so floor the budget well above the
        // provider's default to avoid a truncated (unparseable) JSON array.
        let max_tokens = crate::services::settings::default_provider(&state.settings)
            .map(|p| p.max_tokens)
            .unwrap_or(900)
            .clamp(4096, 8192) as u32;
        if llm.supports_tools() {
            match curate_with_tools(ctx, &*llm, &catalog, max_tokens) {
                Ok(rows) if !rows.is_empty() => llm_rows = rows,
                Ok(_) => {
                    ctx.warn("tool-driven curate returned no usable collections falling back to catalog-in-prompt");
                    llm_rows = curate_with_prompt(ctx, &*llm, &catalog, max_tokens);
                }
                Err(e) => {
                    ctx.error(format!("tool-driven curate failed: {e:#} falling back to catalog-in-prompt"));
                    llm_rows = curate_with_prompt(ctx, &*llm, &catalog, max_tokens);
                }
            }
        } else {
            ctx.debug("LLM does not support tool calling using catalog-in-prompt curate");
            llm_rows = curate_with_prompt(ctx, &*llm, &catalog, max_tokens);
        }
    } else {
        ctx.warn("no LLM configured only deterministic director collections (enable one under Admin → IA)");
    }

    // Interleave the two sources for variety, assign rank, persist (replace-all).
    let mut rows = interleave(director_rows.into_iter(), llm_rows.into_iter());
    for (i, r) in rows.iter_mut().enumerate() {
        r.rank = i as i64;
    }
    let n = rows.len();
    crate::db::set_curated(&state.db, &rows)?;
    ctx.info(format!("curated {n} collections"));
    state.events.publish(ServerEvent::LibraryUpdated);
    Ok(())
}

/// Tool-driven editorial curation: hand the model the catalog connector and let
/// it query the library directly, then resolve its **id**-based members exactly.
/// Errors so the caller can fall back to the catalog-in-prompt path.
fn curate_with_tools(
    ctx: &JobContext,
    llm: &dyn crate::infra::llm::LlmClient,
    catalog: &[crate::services::sections::curate::CatalogEntry],
    max_tokens: u32,
) -> Result<Vec<crate::db::CuratedRow>> {
    use crate::infra::llm::ToolBox;
    use crate::services::llm::CatalogTools;
    use crate::services::sections::curate;

    // Logger so each tool call shows in the Tâches run log (name + args + size).
    let tools = CatalogTools::new(ctx.state.db.clone(), Some(ctx.debug_logger()));
    let defs = tools.defs();
    let (system, user) = curate::tool_curate_prompt();
    ctx.debug(format!(
        "tool-driven curate via {} ({} tools, ≤{} steps)",
        llm.describe(),
        defs.len(),
        MAX_TOOL_STEPS
    ));
    let reply = llm.run_tools(&system, &user, &defs, &tools, max_tokens, MAX_TOOL_STEPS)?;
    let specs = curate::parse_curate(&reply)?;
    let (rows, dropped) = curate::resolve_members_by_id(&specs, catalog);
    ctx.info(format!(
        "LLM tool collections: {} kept, {} dropped (< {} valid ids)",
        rows.len(),
        dropped,
        curate::MIN_ITEMS
    ));
    Ok(rows)
}

/// Catalog-in-prompt editorial curation (fallback): hand the model a pruned slice
/// of titles in the prompt and match its **title**-based members back to ids.
/// Logs and returns whatever it produced (empty on request/parse failure).
fn curate_with_prompt(
    ctx: &JobContext,
    llm: &dyn crate::infra::llm::LlmClient,
    catalog: &[crate::services::sections::curate::CatalogEntry],
    max_tokens: u32,
) -> Vec<crate::db::CuratedRow> {
    use crate::services::sections::curate;

    let pruned = curate::prune_for_prompt(catalog, MAX_CURATE_CATALOG);
    let (system, user) = curate::build_curate_prompt(&pruned);
    ctx.debug(format!(
        "catalog-in-prompt curate: {} titles, {} chars → {}",
        pruned.len(),
        system.len() + user.len(),
        llm.describe()
    ));
    match llm.complete(&system, &user, max_tokens) {
        Ok(reply) => match curate::parse_curate(&reply) {
            Ok(specs) => {
                let (rows, dropped) = curate::resolve_members(&specs, catalog);
                ctx.info(format!(
                    "LLM collections: {} kept, {} dropped (< {} catalog matches)",
                    rows.len(),
                    dropped,
                    curate::MIN_ITEMS
                ));
                rows
            }
            Err(e) => {
                ctx.error(format!("could not parse model reply: {e} reply: {}", snippet(&reply)));
                Vec::new()
            }
        },
        Err(e) => {
            ctx.error(format!("LLM request failed: {e:#}"));
            Vec::new()
        }
    }
}

/// Alternate two iterators (a, b, a, b, …) so a mix of both sources surfaces even
/// when one dominates.
fn interleave<T>(mut a: impl Iterator<Item = T>, mut b: impl Iterator<Item = T>) -> Vec<T> {
    let mut out = Vec::new();
    loop {
        match (a.next(), b.next()) {
            (Some(x), Some(y)) => {
                out.push(x);
                out.push(y);
            }
            (Some(x), None) => out.push(x),
            (None, Some(y)) => out.push(y),
            (None, None) => break,
        }
    }
    out
}
