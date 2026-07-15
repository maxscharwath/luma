//! `GET /api/items/:id/ai-suggest` the per-title "Suggestions IA" rail on the
//! detail page. Cached per item; on a cache miss it kicks off **background** LLM
//! generation and returns `null`, so the page never blocks on the (slow) model
//! the client re-fetches until the cached row appears.

use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use axum::extract::{Path, State};
use axum::response::{IntoResponse, Response};
use axum::Json;

use crate::api::extract::AuthUser;
use crate::api::util::query;
use crate::db;
use crate::i18n::{self, ReqLocale};
use crate::model::Section;
use crate::state::SharedState;
use axum::routing::get;
use axum::Router;

/// `GET /api/items/:id/ai-suggest`.
pub fn routes() -> Router<SharedState> {
    Router::new().route("/items/{id}/ai-suggest", get(ai_suggest))
}

/// Seeds currently generating de-dupes concurrent requests for the same item
/// while the LLM runs (the client polls every few seconds).
fn in_flight() -> &'static Mutex<HashSet<String>> {
    static IN_FLIGHT: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    IN_FLIGHT.get_or_init(|| Mutex::new(HashSet::new()))
}

/// Last hard-failure time per seed. A hard LLM failure caches nothing (so it can
/// retry), but the detail page keeps polling, so without a cooldown every poll
/// would launch a fresh (possibly paid) generation. We retry at most once per
/// [`RETRY_COOLDOWN`] while the provider is erroring.
fn cooldowns() -> &'static Mutex<HashMap<String, Instant>> {
    static COOLDOWNS: OnceLock<Mutex<HashMap<String, Instant>>> = OnceLock::new();
    COOLDOWNS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Minimum gap between generation attempts for a seed that just hard-failed.
const RETRY_COOLDOWN: Duration = Duration::from_secs(60);

/// `GET /api/items/:id/ai-suggest` (Bearer) → `Section | null`. `null` means it's
/// generating (the client polls); a `Section` possibly with empty `items`
/// means ready (render it, or hide it when empty, and stop polling).
pub async fn ai_suggest(
    State(state): State<SharedState>,
    AuthUser(_user): AuthUser,
    ReqLocale(locale): ReqLocale,
    Path(id): Path<String>,
) -> Response {
    // Cached? Resolve the row + hydrate its ids in one blocking hop.
    let lookup_id = id.clone();
    let result = query(&state.db, move |pool| {
        let Some(row) = db::get_suggestion(&pool, &lookup_id)? else {
            return Ok(None);
        };
        let refs: Vec<&str> = row.item_ids.iter().map(String::as_str).collect();
        let items = db::entities_by_ids(&pool, &refs)?;
        Ok(Some((row, items)))
    })
    .await;

    match result {
        Ok(Some((row, items))) => {
            let title = i18n::t(locale, "content.aiSuggestions", &[]);
            let reason = pick_lang(&row.reasons, locale);
            Json(Some(Section { id: "ai:suggest".to_string(), title, reason, items })).into_response()
        }
        // Cache miss → start background generation (once), tell the client to wait.
        Ok(None) => {
            spawn_generation(state.clone(), id);
            Json::<Option<Section>>(None).into_response()
        }
        Err(resp) => resp,
    }
}

/// Generate + cache suggestions for `id` off the request path. Guarded so only
/// one generation runs per seed at a time.
fn spawn_generation(state: SharedState, id: String) {
    // Back off if a recent attempt for this seed hard-failed, so a persistently
    // erroring provider isn't hit on every poll.
    if cooldowns()
        .lock()
        .unwrap()
        .get(&id)
        .is_some_and(|t| t.elapsed() < RETRY_COOLDOWN)
    {
        return;
    }
    if !in_flight().lock().unwrap().insert(id.clone()) {
        return; // already generating
    }
    tokio::task::spawn_blocking(move || {
        // A panic in generation must NOT leak the in-flight reservation, or the
        // guard above would block every future attempt for this seed (the client
        // then polls forever) until a restart. catch_unwind + a single removal
        // below guarantee the slot is always released.
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| generate(&state, &id)));
        in_flight().lock().unwrap().remove(&id);
        if outcome.is_err() {
            tracing::error!(item = %id, "AI suggestion generation panicked");
        }
    });
}

/// The generation body, isolated so a panic can't skip releasing the in-flight
/// slot. Caches a terminal result (possibly empty) on success; leaves nothing
/// cached on a hard LLM failure so a later view retries.
fn generate(state: &SharedState, id: &str) {
    let llm = crate::infra::llm::from_settings(&state.settings);
    if !llm.available() {
        return; // no LLM → don't cache, so it retries once one is configured
    }
    // A small reply (one object: a few ids + two short reasons); floor the
    // budget so the tool turns + final JSON aren't truncated.
    let max_tokens = crate::services::settings::default_provider(&state.settings)
        .map(|p| p.max_tokens)
        .unwrap_or(900)
        .clamp(2048, 8192) as u32;
    match crate::services::llm::suggest_for(state, id, max_tokens) {
        Ok(Some(s)) => {
            cooldowns().lock().unwrap().remove(id);
            let _ = db::set_suggestion(&state.db, id, &s.ids, &s.reasons);
        }
        // Tried, nothing usable → cache empty (terminal; stops the client polling).
        Ok(None) => {
            cooldowns().lock().unwrap().remove(id);
            let _ = db::set_suggestion(&state.db, id, &[], &HashMap::new());
        }
        // Hard LLM failure → don't cache (so a later view retries), but record the
        // time so polls back off to one attempt per RETRY_COOLDOWN.
        Err(e) => {
            cooldowns().lock().unwrap().insert(id.to_string(), Instant::now());
            tracing::warn!(item = %id, error = %e, "AI suggestion generation failed");
        }
    }
}

/// Pick a locale's reason from a `locale -> string` map, falling back requested
/// -> `en` -> any available.
fn pick_lang(map: &HashMap<String, String>, locale: &str) -> Option<String> {
    map.get(locale).or_else(|| map.get("en")).or_else(|| map.values().next()).cloned()
}
