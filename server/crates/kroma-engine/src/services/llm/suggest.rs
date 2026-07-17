//! Per-title **AI suggestions** for the detail page: hand the model the seed
//! title's data + the catalog connector and let it pick library titles a fan
//! would enjoy, returning resolved member ids. The API caches the result
//! (`db::item_suggestions`); this is just the generation logic.

use std::collections::HashMap;

use anyhow::Result;
use serde::Deserialize;

use crate::db::TitleFull;
use crate::i18n;
use crate::infra::llm::ToolBox;
use crate::services::llm::CatalogTools;
use crate::state::SharedState;

/// Max model turns in the suggestion tool loop (a few `find_titles`, then the
/// JSON reply smaller than curate's catalog-wide pass).
const MAX_TOOL_STEPS: usize = 12;
/// Minimum members for a suggestion section to be worth showing.
const MIN_MEMBERS: usize = 4;

/// A generated suggestion: a localized one-line reason per language + resolved
/// member ids.
pub struct Suggestion {
    pub reasons: HashMap<String, String>,
    pub ids: Vec<String>,
}

/// Generate AI suggestions for one seed item. `Ok(Some)` on success; `Ok(None)`
/// when the LLM is unconfigured / can't tool-call, the seed is unknown, or the
/// reply was unusable / too thin; `Err` only on a hard LLM failure so the
/// caller can cache `None` as a terminal "nothing" but retry on `Err`.
pub fn suggest_for(state: &SharedState, seed_id: &str, max_tokens: u32) -> Result<Option<Suggestion>> {
    let pool = &state.db;
    let Some(seed) = crate::db::get_title(pool, seed_id)? else {
        return Ok(None);
    };
    let llm = crate::infra::llm::from_settings(&state.settings);
    if !llm.available() || !llm.supports_tools() {
        return Ok(None); // needs tool calling to browse the catalog
    }

    let (system, user) = build_prompt(&seed);
    let tools = CatalogTools::new(pool.clone(), None);
    let reply = llm.run_tools(&system, &user, &tools.defs(), &tools, max_tokens, MAX_TOOL_STEPS)?;

    let Some(spec) = parse(&reply) else {
        return Ok(None);
    };
    let mut seen = std::collections::HashSet::new();
    let claimed: Vec<String> = spec
        .members
        .into_iter()
        .map(|m| m.trim().to_string())
        .filter(|m| !m.is_empty() && m != seed_id && seen.insert(m.clone()))
        .collect();
    // Resolve against the real catalog (movies *and* shows) and gate on the
    // resolved count the model can echo titles or stale/invented ids, which
    // would otherwise pass the raw-count gate, get cached as terminal, then be
    // silently dropped at hydration, leaving the rail permanently empty.
    let refs: Vec<&str> = claimed.iter().map(String::as_str).collect();
    let ids: Vec<String> =
        crate::db::entities_by_ids(pool, &refs)?.iter().map(|e| e.id().to_string()).collect();
    if ids.len() < MIN_MEMBERS {
        return Ok(None);
    }
    let reasons = spec
        .reason
        .into_iter()
        .filter(|(_, v)| !v.trim().is_empty())
        .map(|(k, v)| (k, v.trim().to_string()))
        .collect();
    Ok(Some(Suggestion { reasons, ids }))
}

/// (system, user) prompt: describe the seed, ask for library titles a fan would
/// enjoy, members returned as catalog ids from the tools.
fn build_prompt(s: &TitleFull) -> (String, String) {
    let reason_fields =
        i18n::SUPPORTED_LOCALES.iter().map(|l| format!("\"{l}\":string")).collect::<Vec<_>>().join(",");
    let codes = i18n::SUPPORTED_LOCALES.join(", ");
    let system = format!(
        "You are the resident film & TV concierge of a personal library. Given one title the viewer \
         is looking at, suggest OTHER titles from this library a fan of it would enjoy same director \
         or cast, kindred genre, era or mood. You have tools: list_genres, list_people, find_titles \
         (filter by genre / director / actor / year / rating) and get_title. Use find_titles to gather \
         candidates (try the seed's director, its lead actors, and its genres).\n\
         When done, reply with STRICT JSON only no prose, no markdown, no fences:\n\
         {{\"reason\":{{{reason_fields}}},\"members\":[string]}}\n\
         - \"members\": 8-15 catalog **ids** returned by the tools (each title's \"id\" field), \
         excluding the seed; never invent ids.\n\
         - \"reason\" is an object keyed by language code ({codes}) provide every listed language, \
         ONE short clause each on what ties them to the seed."
    );
    let directors = s.directors.join(", ");
    let cast = s.cast.iter().take(5).cloned().collect::<Vec<_>>().join(", ");
    let genres = s.genres.join(", ");
    let overview: String = s.overview.as_deref().unwrap_or("").chars().take(280).collect();
    let dash = |s: &str| if s.trim().is_empty() { "-".to_string() } else { s.to_string() };
    let user = format!(
        "Seed (id {}): \"{}\"{}\n- genres: {}\n- director: {}\n- cast: {}\n- synopsis: {}\n\n\
         Suggest library titles a fan of this would enjoy. Return the JSON now.",
        s.id,
        s.title,
        s.year.map(|y| format!(" ({y})")).unwrap_or_default(),
        dash(&genres),
        dash(&directors),
        dash(&cast),
        dash(&overview),
    );
    (system, user)
}

/// One suggestion object as the model returned it. `reason` is a locale-keyed
/// object (`{"en":…,"fr":…}`) over the supported languages.
#[derive(Deserialize, Default)]
#[serde(default)]
struct Spec {
    reason: HashMap<String, String>,
    members: Vec<String>,
}

/// Parse the outermost JSON object from a (possibly fenced / prefixed) reply.
fn parse(text: &str) -> Option<Spec> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    if end <= start {
        return None;
    }
    serde_json::from_str(&text[start..=end]).ok()
}
