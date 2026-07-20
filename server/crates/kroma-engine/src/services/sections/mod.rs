//! The home-screen **section generator**: turns the current context (date,
//! daypart, the viewer's recent history) + the embedding cache + a phrase bank
//! into an ordered, de-duplicated list of [`Section`]s. The client renders the
//! result generically all the "what rows, in what order" logic lives here.
//!
//! Pipeline per request: refresh the vector cache (if stale) → build context →
//! emit candidate sections in priority order, each resolved to items, capped,
//! de-duplicated against earlier rows, and dropped if too thin (quality gate).

mod cache;
mod context;
pub mod curate;
pub mod featured;
pub mod generate;
mod math;
mod phrases;
pub mod taste;

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
/// At most this many *curated editorial* rows in one home (the job makes more;
/// a daily-rotated window shows a fresh slice each day).
const MAX_CURATED: usize = 3;
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

    // Reserve the tail slots for the baseline browse rows (Trending, plus
    // Recently-added when enabled) so heavy personalization can't crowd them out:
    // the discretionary rows below (AI / curated / themed) stop here, leaving room.
    let show_recent = state.settings.get_bool("showRecentHome", true);
    let discretionary_cap = MAX_SECTIONS.saturating_sub(1 + usize::from(show_recent));

    // 1) For You personalized taste centroid (no floor: it reflects your taste
    //    even loosely, and is always wanted when you have history).
    if !ctx.watched.is_empty() {
        let ranked = state.vectors.for_you(&ctx.watched, FETCH);
        out.push("for-you", i18n::t(locale, "content.forYou", &[]), None, ranked, NO_FLOOR);
    }

    // 2) Because you watched <the last thing>. Genre-guarded: the lexical embedder
    //    is weakly discriminative item↔item (the catalog clusters in a narrow
    //    cosine band, so a drama's "nearest" can be a horror film), and this row
    //    makes a *specific* similarity claim about one seed so require a shared
    //    genre with it. (No-op when the seed has no genres, or with a discriminative
    //    backend where the neighbours already share genres.)
    push_because(&mut out, state, pool, &ctx, locale);

    // 2.5) Personalized, LLM-named rows authored by the nightly
    //      `sections.personalize` job. The model only *names* each row; the items
    //      come from the embedder resolving its vibe `query`, so they're always
    //      real catalog titles. Floored like themed rows. Falls through to the
    //      static bank below when the user has none yet (no LLM / too little
    //      history).
    push_ai_rows(&mut out, state, pool, user_id, discretionary_cap, floor);

    // 2.6) Editorial curated collections global, same for everyone (director
    //      spotlights + LLM-curated genre/list/franchise/mood rows from the
    //      `sections.curate` job). Membership is explicit (resolved ids), so
    //      NO_FLOOR; a daily-rotated window keeps the home feeling fresh.
    push_curated_rows(&mut out, pool, locale, discretionary_cap);

    // 3) Themed rows eligible phrases, FLOORED: a phrase only becomes a row if
    //    the library actually has matches above the noise level.
    push_themed_rows(&mut out, state, &ctx, locale, discretionary_cap, floor);

    // 4) Trending in your library (recency-weighted plays) SQL, unscored.
    let trending = unscored(trending_ids(pool, FETCH));
    out.push("trending", i18n::t(locale, "content.trending", &[]), None, trending, NO_FLOOR);

    // 5) Recently added SQL, unscored. Gated by the admin "show recent on home"
    //    preference (on by default).
    if state.settings.get_bool("showRecentHome", true) {
        let recent = unscored(db::recently_added_ids(pool, FETCH).unwrap_or_default());
        out.push("recent", i18n::t(locale, "content.recentlyAdded", &[]), None, recent, NO_FLOOR);
    }

    // Overlay every row's items into the request locale (title/overview/genres).
    // Best-effort a translation-cache miss leaves the household-language blob.
    let mut sections = out.sections;
    for s in &mut sections {
        let _ = db::localize::overlay_section_items(pool, &mut s.items, locale);
    }
    sections
}

/// "Because you watched <the last thing>". Genre-guarded: the lexical embedder is
/// weakly discriminative item<->item, and this row makes a *specific* similarity
/// claim about one seed, so require a shared genre with it. (No-op when the seed
/// has no genres, or with a discriminative backend.)
fn push_because(out: &mut Builder, state: &SharedState, pool: &Pool, ctx: &Context, locale: &str) {
    if let Some(last) = &ctx.last_played {
        if let Some(title) = last_title(pool, last) {
            let heading = i18n::t(locale, "content.becauseYouWatched", &[("title", &title)]);
            let ranked = db::genre_guard(pool, last, state.vectors.similar(last, FETCH));
            out.push("because", heading, None, ranked, NO_FLOOR);
        }
    }
}

/// Personalized, LLM-named rows authored by the nightly `sections.personalize`
/// job. The model only *names* each row; the items come from the embedder
/// resolving its vibe `query`, so they're always real catalog titles. Floored
/// like themed rows. Empty when the user has none yet (no LLM / too little
/// history).
fn push_ai_rows(
    out: &mut Builder,
    state: &SharedState,
    pool: &Pool,
    user_id: &str,
    discretionary_cap: usize,
    floor: f32,
) {
    for gs in generate::load(pool, user_id) {
        if out.sections.len() >= discretionary_cap {
            break;
        }
        let query = state.embedder.embed(&gs.query);
        let ranked = state.vectors.nearest(&query, FETCH, &HashSet::new());
        let reason = (!gs.reason.is_empty()).then_some(gs.reason);
        out.push(&format!("ai:{}", gs.key), gs.title, reason, ranked, floor);
    }
}

/// Editorial curated collections global, same for everyone (director spotlights +
/// LLM-curated genre/list/franchise/mood rows). Membership is explicit (resolved
/// ids), so `NO_FLOOR`; a daily-rotated window keeps the home feeling fresh.
fn push_curated_rows(out: &mut Builder, pool: &Pool, locale: &str, discretionary_cap: usize) {
    for (key, title, reason, ids) in curated_rows(pool, locale) {
        if out.sections.len() >= discretionary_cap {
            break;
        }
        out.push(&format!("curated:{key}"), title, reason, unscored(ids), NO_FLOOR);
    }
}

/// Themed rows eligible phrases, FLOORED: a phrase only becomes a row if the
/// library actually has matches above the noise level.
fn push_themed_rows(
    out: &mut Builder,
    state: &SharedState,
    ctx: &Context,
    locale: &str,
    discretionary_cap: usize,
    floor: f32,
) {
    let mut themed = 0;
    for phrase in phrases::eligible(ctx) {
        if themed >= MAX_THEMED || out.sections.len() >= discretionary_cap {
            break;
        }
        let query = state.embedder.embed(phrase.query);
        let ranked = state.vectors.nearest(&query, FETCH, &HashSet::new());
        if out.push(&format!("themed:{}", phrase.key), i18n::t(locale, phrase.title_key, &[]), None, ranked, floor) {
            themed += 1;
        }
    }
}

/// Wrap SQL-sourced ids (trending / recently-added) as `(id, score)` so they flow
/// through the same [`Builder::push`]; they carry no similarity, so `NO_FLOOR`.
fn unscored(ids: Vec<String>) -> Vec<(String, f32)> {
    ids.into_iter().map(|id| (id, 0.0)).collect()
}

/// [`db::trending_ids`] with the failure *logged* before falling back to empty
/// (shared with [`featured::pick`]'s trending signal). Swallowing the error made
/// a broken query indistinguishable from "nobody watched anything": a missing
/// SQLite `POW()` emptied the Trending row on every request and nothing said so.
fn trending_ids(pool: &Pool, n: usize) -> Vec<String> {
    db::trending_ids(pool, n).unwrap_or_else(|e| {
        tracing::warn!(target: "sections", error = %e, "trending query failed; falling back to no trending items");
        Vec::new()
    })
}

/// The curated rows to show this request: the top [`MAX_CURATED`] from the
/// `curated_sections` table, on a daily-rotated offset, localized to `locale`.
/// Each is `(key, title, reason, member_ids)`.
fn curated_rows(pool: &Pool, locale: &str) -> Vec<(String, String, Option<String>, Vec<String>)> {
    let all = db::get_curated(pool).unwrap_or_default();
    if all.is_empty() {
        return Vec::new();
    }
    // Rotate the window once per day (no per-request RNG → stable within a day).
    let day = (crate::services::jobs::now_ms() / 86_400_000) as usize;
    let offset = day % all.len();
    let n = MAX_CURATED.min(all.len());
    (0..n)
        .map(|i| {
            let row = &all[(offset + i) % all.len()];
            let title = pick_lang(&row.titles, locale).unwrap_or_else(|| row.key.clone());
            let reason = pick_lang(&row.reasons, locale);
            (row.key.clone(), title, reason, row.item_ids.clone())
        })
        .collect()
}

/// Pick a locale's string from a `locale -> string` map, falling back requested
/// -> `en` -> any available.
fn pick_lang(map: &std::collections::HashMap<String, String>, locale: &str) -> Option<String> {
    map.get(locale).or_else(|| map.get("en")).or_else(|| map.values().next()).cloned()
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
        // Resolve to movies *or* shows so a row can mix films and séries.
        let items = match db::entities_by_ids(self.pool, &fresh) {
            Ok(v) if v.len() >= MIN_ITEMS => v,
            _ => return false,
        };
        for it in &items {
            self.seen.insert(it.id().to_string());
        }
        self.sections.push(Section { id: id.to_string(), title, reason, items });
        true
    }
}

/// Title of one item id (for the "Because you watched …" heading).
fn last_title(pool: &Pool, id: &str) -> Option<String> {
    db::items_by_ids(pool, &[id]).ok()?.into_iter().next().map(|i| i.title)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn test_pool() -> Pool {
        static SEQ: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("kroma-sections-mod-{}-{n}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        crate::db::init(&path).unwrap()
    }

    #[test]
    fn unscored_wraps_ids_with_zero_score() {
        let out = unscored(vec!["a".into(), "b".into()]);
        assert_eq!(out, vec![("a".to_string(), 0.0), ("b".to_string(), 0.0)]);
    }

    #[test]
    fn pick_lang_prefers_locale_then_en_then_any() {
        let mut m = HashMap::new();
        m.insert("fr".to_string(), "Bonjour".to_string());
        m.insert("en".to_string(), "Hello".to_string());
        assert_eq!(pick_lang(&m, "fr").as_deref(), Some("Bonjour"));
        // Missing locale -> English fallback.
        assert_eq!(pick_lang(&m, "de").as_deref(), Some("Hello"));
        // Only a non-en language present -> any value.
        let mut only_fr = HashMap::new();
        only_fr.insert("fr".to_string(), "Salut".to_string());
        assert_eq!(pick_lang(&only_fr, "en").as_deref(), Some("Salut"));
        // Empty -> None.
        assert!(pick_lang(&HashMap::new(), "en").is_none());
    }

    fn curated(key: &str) -> crate::db::CuratedRow {
        crate::db::CuratedRow {
            key: key.to_string(),
            rank: 0,
            source: "llm".to_string(),
            item_ids: vec!["x".to_string()],
            titles: HashMap::from([("en".to_string(), format!("Title {key}"))]),
            reasons: HashMap::from([("en".to_string(), "because".to_string())]),
        }
    }

    #[test]
    fn curated_rows_returns_capped_window() {
        let pool = test_pool();
        let rows = vec![curated("a"), curated("b"), curated("c"), curated("d")];
        crate::db::set_curated(&pool, &rows).unwrap();
        let out = curated_rows(&pool, "en");
        // MAX_CURATED (3) of the 4 stored rows.
        assert_eq!(out.len(), 3);
        // Each entry carries a localized title + reason.
        for (_key, title, reason, ids) in &out {
            assert!(title.starts_with("Title "));
            assert_eq!(reason.as_deref(), Some("because"));
            assert_eq!(ids, &vec!["x".to_string()]);
        }
    }

    #[test]
    fn curated_rows_empty_without_data() {
        let pool = test_pool();
        assert!(curated_rows(&pool, "en").is_empty());
    }

    fn seed_movies(pool: &Pool, ids: &[&str]) {
        let conn = pool.get().unwrap();
        conn.execute(
            "INSERT OR IGNORE INTO libraries (id,name,kind,path,added_at) VALUES ('lib','L','movies','/x','t')",
            [],
        )
        .unwrap();
        for id in ids {
            // Test-controlled literal ids, so inline them.
            conn.execute(
                &format!("INSERT INTO items (id,kind,title,container,library,added_at) VALUES ('{id}','movie','Title {id}','mkv','lib','t')"),
                [],
            )
            .unwrap();
        }
    }

    #[test]
    fn builder_push_adds_section_when_enough_fresh_items() {
        let pool = test_pool();
        seed_movies(&pool, &["a", "b", "c", "d", "e"]);
        let mut b = Builder { pool: &pool, sections: Vec::new(), seen: HashSet::new() };
        let ranked: Vec<(String, f32)> =
            ["a", "b", "c", "d", "e"].iter().map(|i| (i.to_string(), 1.0)).collect();
        assert!(b.push("row1", "Row".into(), None, ranked, NO_FLOOR));
        assert_eq!(b.sections.len(), 1);
        assert_eq!(b.sections[0].items.len(), 5);
        // Every resolved id is now marked seen (cross-row de-duplication).
        assert!(b.seen.contains("a") && b.seen.contains("e"));
    }

    #[test]
    fn builder_push_rejects_thin_row() {
        let pool = test_pool();
        seed_movies(&pool, &["a", "b"]);
        let mut b = Builder { pool: &pool, sections: Vec::new(), seen: HashSet::new() };
        let ranked: Vec<(String, f32)> = ["a", "b"].iter().map(|i| (i.to_string(), 1.0)).collect();
        // Fewer than MIN_ITEMS survivors -> no section.
        assert!(!b.push("row1", "Row".into(), None, ranked, NO_FLOOR));
        assert!(b.sections.is_empty());
    }

    #[test]
    fn builder_push_applies_floor_and_seen_filters() {
        let pool = test_pool();
        seed_movies(&pool, &["a", "b", "c", "d", "e", "f", "g"]);
        let mut b = Builder { pool: &pool, sections: Vec::new(), seen: HashSet::new() };
        b.seen.insert("a".to_string()); // already shown in an earlier row
        let ranked = vec![
            ("a".to_string(), 1.0), // seen -> dropped
            ("b".to_string(), 0.1), // below the 0.5 floor -> dropped
            ("c".to_string(), 0.9),
            ("d".to_string(), 0.9),
            ("e".to_string(), 0.9),
            ("f".to_string(), 0.9),
            ("g".to_string(), 0.9),
        ];
        // Five survive (c..g), so the row is produced without a or b.
        assert!(b.push("row1", "Row".into(), None, ranked, 0.5));
        let ids: Vec<&str> = b.sections[0].items.iter().map(|i| i.id()).collect();
        assert_eq!(ids.len(), 5);
        assert!(!ids.contains(&"a") && !ids.contains(&"b"));
    }

    #[test]
    fn builder_push_rejects_once_at_max_sections() {
        let pool = test_pool();
        seed_movies(&pool, &["a", "b", "c", "d", "e"]);
        let dummy = || Section { id: "x".into(), title: "x".into(), reason: None, items: Vec::new() };
        let mut b = Builder {
            pool: &pool,
            sections: (0..MAX_SECTIONS).map(|_| dummy()).collect(),
            seen: HashSet::new(),
        };
        let ranked: Vec<(String, f32)> =
            ["a", "b", "c", "d", "e"].iter().map(|i| (i.to_string(), 1.0)).collect();
        assert!(!b.push("row1", "Row".into(), None, ranked, NO_FLOOR));
    }

    #[test]
    fn last_title_returns_title_or_none() {
        let pool = test_pool();
        seed_movies(&pool, &["a"]);
        assert_eq!(last_title(&pool, "a").as_deref(), Some("Title a"));
        assert!(last_title(&pool, "ghost").is_none());
    }

    // ----- SharedState-backed home build. The no-op embedder yields no semantic
    // rows (for-you / because / ai / themed resolve to nothing), so the assertions
    // target the deterministic SQL-sourced rows (trending, recently-added) + a
    // direct push of a curated row. --------------------------------------------

    use crate::test_support;

    #[test]
    fn build_home_emits_the_trending_and_recently_added_rows_and_no_thin_rows() {
        let state = test_support::test_state();
        let ids: Vec<String> = (0..12).map(|i| format!("m{i}")).collect();
        let refs: Vec<&str> = ids.iter().map(|s| s.as_str()).collect();
        seed_movies(&state.db, &refs);
        // Recent plays of the first six, so `Context` resolves a last-played + watch
        // history (the "because you watched" branch runs, adding no row under the
        // no-op embedder) *and* the trending row has enough items to clear the gate.
        for id in refs.iter().take(6) {
            test_support::seed_play(&state, "u1", id, test_support::now_secs());
        }

        let sections = build_home(&state, &state.db, "en", "u1");

        // Trending and recently-added are the deterministic, SQL-sourced rows that
        // do not depend on the embedder. Trending carries the played titles; the
        // other six fall through to recently-added (cross-row de-duplication).
        let trending = sections.iter().find(|s| s.id == "trending").expect("trending row present");
        assert_eq!(trending.items.len(), 6);
        assert!(trending.items.iter().all(|i| refs[..6].contains(&i.id())));
        let recent = sections.iter().find(|s| s.id == "recent").expect("recent row present");
        assert_eq!(recent.items.len(), 6);
        // The quality gate means every emitted row clears MIN_ITEMS (nothing thin).
        assert!(sections.iter().all(|s| s.items.len() >= MIN_ITEMS));
    }

    #[test]
    fn push_curated_rows_resolves_membership_into_a_section() {
        let state = test_support::test_state();
        let ids = ["a", "b", "c", "d", "e"];
        seed_movies(&state.db, &ids);
        let row = crate::db::CuratedRow {
            key: "dir-spotlight".into(),
            rank: 0,
            source: "llm".into(),
            item_ids: ids.iter().map(|s| s.to_string()).collect(),
            titles: HashMap::from([("en".to_string(), "Director Spotlight".to_string())]),
            reasons: HashMap::from([("en".to_string(), "great films".to_string())]),
        };
        crate::db::set_curated(&state.db, &[row]).unwrap();

        let mut b = Builder { pool: &state.db, sections: Vec::new(), seen: HashSet::new() };
        push_curated_rows(&mut b, &state.db, "en", MAX_SECTIONS);
        assert_eq!(b.sections.len(), 1);
        assert_eq!(b.sections[0].id, "curated:dir-spotlight");
        assert_eq!(b.sections[0].title, "Director Spotlight");
        assert_eq!(b.sections[0].reason.as_deref(), Some("great films"));
        assert_eq!(b.sections[0].items.len(), 5);
    }
}
