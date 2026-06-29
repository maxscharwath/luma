//! The home-screen **section generator**: turns the current context (date,
//! daypart, the viewer's recent history) + the embedding cache + a phrase bank
//! into an ordered, de-duplicated list of [`Section`]s. The client renders the
//! result generically — all the "what rows, in what order" logic lives here.
//!
//! Pipeline per request: refresh the vector cache (if stale) → build context →
//! emit candidate sections in priority order, each resolved to items, capped,
//! de-duplicated against earlier rows, and dropped if too thin (quality gate).

mod cache;
mod context;
mod phrases;

pub use cache::VectorCache;

use std::collections::HashSet;

use crate::db::{self, Pool};
use crate::i18n;
use crate::model::Section;
use crate::state::SharedState;

use context::Context;

/// Items per rail.
const SECTION_CAP: usize = 20;
/// Over-fetch margin so a row still fills after cross-row de-duplication.
const FETCH: usize = SECTION_CAP + 16;
/// A row needs at least this many items (after dedupe) to be worth showing.
const MIN_ITEMS: usize = 5;
/// Hard cap on rows returned.
const MAX_SECTIONS: usize = 9;
/// At most this many *themed* rows in one home (the bank has more than fit).
const MAX_THEMED: usize = 4;
/// Sentinel: no relevance floor (For You / trending / recently-added rows, which
/// aren't gated on themed-query similarity).
const NO_FLOOR: f32 = f32::NEG_INFINITY;

/// Build the ordered, de-duplicated home for one user. Infallible: any step that
/// fails (or comes up thin) simply contributes no section.
pub fn build_home(state: &SharedState, pool: &Pool, locale: &str, user_id: &str) -> Vec<Section> {
    let _ = state.vectors.refresh_if_stale(pool);
    let ctx = Context::build(pool, user_id);
    // Themed rows must clear this cosine floor or they're noise (the lexical
    // backend's "christmas" → random-classics row is what this kills).
    let floor = state.embedder.relevance_floor();

    let mut out = Builder { pool, sections: Vec::new(), seen: HashSet::new() };

    // 1) For You — personalized taste centroid (no floor: it reflects your taste
    //    even loosely, and is always wanted when you have history).
    if !ctx.watched.is_empty() {
        let ranked = state.vectors.for_you(&ctx.watched, FETCH);
        out.push("for-you", i18n::t(locale, "content.forYou", &[]), None, ranked, NO_FLOOR);
    }

    // 2) Because you watched <the last thing>.
    if let Some(last) = &ctx.last_played {
        if let Some(title) = last_title(pool, last) {
            let heading = i18n::t(locale, "content.becauseYouWatched", &[("title", &title)]);
            let ranked = state.vectors.similar(last, FETCH);
            out.push("because", heading, None, ranked, NO_FLOOR);
        }
    }

    // 3) Themed rows — eligible phrases, FLOORED: a phrase only becomes a row if
    //    the library actually has matches above the noise level.
    let mut themed = 0;
    for phrase in phrases::eligible(&ctx) {
        if themed >= MAX_THEMED || out.sections.len() >= MAX_SECTIONS {
            break;
        }
        let query = state.embedder.embed(phrase.query);
        let ranked = state.vectors.nearest(&query, FETCH, &HashSet::new());
        if out.push(&format!("themed:{}", phrase.key), i18n::t(locale, phrase.title_key, &[]), None, ranked, floor) {
            themed += 1;
        }
    }

    // 4) Trending in your library (recency-weighted plays) — SQL, unscored.
    let trending = unscored(db::trending_ids(pool, FETCH).unwrap_or_default());
    out.push("trending", i18n::t(locale, "content.trending", &[]), None, trending, NO_FLOOR);

    // 5) Recently added — SQL, unscored.
    let recent = unscored(db::recently_added_ids(pool, FETCH).unwrap_or_default());
    out.push("recent", i18n::t(locale, "content.recentlyAdded", &[]), None, recent, NO_FLOOR);

    out.sections
}

/// Wrap SQL-sourced ids (trending / recently-added) as `(id, score)` so they flow
/// through the same [`Builder::push`]; they carry no similarity, so `NO_FLOOR`.
fn unscored(ids: Vec<String>) -> Vec<(String, f32)> {
    ids.into_iter().map(|id| (id, 0.0)).collect()
}

/// Accumulates sections while enforcing the caps, the quality gate, and the
/// cross-row de-duplication (a title shows in at most one row).
struct Builder<'a> {
    pool: &'a Pool,
    sections: Vec<Section>,
    seen: HashSet<String>,
}

impl Builder<'_> {
    /// Resolve scored `ranked` ids into a section; returns whether one was added.
    /// Items below `floor` are dropped before the count gate, so a row that's all
    /// weak matches simply never appears.
    fn push(&mut self, id: &str, title: String, reason: Option<String>, ranked: Vec<(String, f32)>, floor: f32) -> bool {
        if self.sections.len() >= MAX_SECTIONS {
            return false;
        }
        let fresh: Vec<&str> = ranked
            .iter()
            .filter(|(_, score)| *score >= floor)
            .map(|(id, _)| id.as_str())
            .filter(|i| !self.seen.contains(*i))
            .take(SECTION_CAP)
            .collect();
        if fresh.len() < MIN_ITEMS {
            return false;
        }
        let items = match db::items_by_ids(self.pool, &fresh) {
            Ok(v) if v.len() >= MIN_ITEMS => v,
            _ => return false,
        };
        for it in &items {
            self.seen.insert(it.id.clone());
        }
        self.sections.push(Section { id: id.to_string(), title, reason, items });
        true
    }
}

/// Title of one item id (for the "Because you watched …" heading).
fn last_title(pool: &Pool, id: &str) -> Option<String> {
    db::items_by_ids(pool, &[id]).ok()?.into_iter().next().map(|i| i.title)
}
