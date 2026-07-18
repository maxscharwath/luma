//! Editorial **curated collections** catalog-wide rows shown to everyone
//! ("Spielberg", "Best Horror", "Top IMDb", franchises, decades, moods). Two
//! producers, both emitting [`CuratedRow`]:
//!  * [`director_collections`] **deterministic**: group titles by their crew
//!    director (accurate, no model needed), and
//!  * the **LLM** path ([`build_curate_prompt`] → [`parse_curate`] →
//!    [`resolve_members`]) the model curates from the library titles we hand
//!    it; members are matched back to real ids, so nothing it invents surfaces.
//!
//! Pure + tested; the orchestration (load catalog, call the model, persist) is
//! the `sections.curate` job in [`crate::services::jobs`].

use std::collections::{HashMap, HashSet};

use serde::Deserialize;

use crate::db::CuratedRow;
use crate::i18n;
use crate::model::{Kind, MediaItem, Show};

use super::generate::slug;

/// Build a `locale -> value` map over every [`i18n::SUPPORTED_LOCALES`] entry.
fn per_locale(mut f: impl FnMut(&str) -> String) -> HashMap<String, String> {
    i18n::SUPPORTED_LOCALES.iter().map(|&l| (l.to_string(), f(l))).collect()
}

/// Minimum members for a collection to be worth keeping.
pub const MIN_ITEMS: usize = 5;
/// Cap on deterministic director collections (most prolific directors first).
const MAX_DIRECTORS: usize = 12;
/// Cap on LLM editorial collections kept from one reply.
const MAX_LLM: usize = 14;
/// Crew jobs that count as "directed/created by" for director collections.
const DIRECTING_JOBS: &[&str] = &["Director", "Creator"];

/// A recommendable entity (movie or show) flattened for curation.
pub struct CatalogEntry {
    pub id: String,
    pub title: String,
    pub year: Option<u32>,
    pub rating: f32,
    pub genres: Vec<String>,
    pub directors: Vec<String>,
}

/// Flatten movies/videos (skip episodes) + shows into catalog entries.
pub fn build_catalog(items: &[MediaItem], shows: &[Show]) -> Vec<CatalogEntry> {
    let mut out = Vec::new();
    for it in items {
        if it.kind == Kind::Episode {
            continue;
        }
        let (rating, genres, directors) = meta_bits(it.metadata.as_ref());
        out.push(CatalogEntry { id: it.id.clone(), title: it.title.clone(), year: it.year, rating, genres, directors });
    }
    for s in shows {
        let (rating, genres, directors) = meta_bits(s.metadata.as_ref());
        out.push(CatalogEntry { id: s.id.clone(), title: s.title.clone(), year: s.year, rating, genres, directors });
    }
    out
}

fn meta_bits(meta: Option<&crate::model::Metadata>) -> (f32, Vec<String>, Vec<String>) {
    match meta {
        Some(m) => {
            let directors = m
                .crew
                .iter()
                .filter(|c| DIRECTING_JOBS.contains(&c.job.as_str()))
                .map(|c| c.name.clone())
                .collect();
            (m.rating.unwrap_or(0.0), m.genres.clone(), directors)
        }
        None => (0.0, Vec::new(), Vec::new()),
    }
}

/// Group the catalog by director and emit a collection per director with at
/// least [`MIN_ITEMS`] titles (members highest-rated first). Most prolific
/// directors first, capped at [`MAX_DIRECTORS`].
pub fn director_collections(catalog: &[CatalogEntry]) -> Vec<CuratedRow> {
    let mut by_director: HashMap<&str, Vec<&CatalogEntry>> = HashMap::new();
    for e in catalog {
        for d in &e.directors {
            by_director.entry(d.as_str()).or_default().push(e);
        }
    }
    let mut groups: Vec<(&str, Vec<&CatalogEntry>)> =
        by_director.into_iter().filter(|(_, v)| v.len() >= MIN_ITEMS).collect();
    // Biggest filmographies first; name as a stable tiebreak.
    groups.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then_with(|| a.0.cmp(b.0)));

    groups
        .into_iter()
        .take(MAX_DIRECTORS)
        .map(|(name, mut films)| {
            films.sort_by(|a, b| b.rating.total_cmp(&a.rating));
            CuratedRow {
                key: slug(&format!("dir-{name}")),
                rank: 0,
                source: "director".to_string(),
                item_ids: films.iter().map(|e| e.id.clone()).collect(),
                // The director's name is language-invariant; the reason is
                // localized per supported language via the shared i18n catalog.
                titles: per_locale(|_| name.to_string()),
                reasons: per_locale(|l| i18n::t(l, "content.directedBy", &[("name", name)])),
            }
        })
        .collect()
}

/// Sort the catalog by rating (then recency) and keep the top `max` the slice
/// of titles handed to the model so the prompt stays within a sane token budget.
pub fn prune_for_prompt(catalog: &[CatalogEntry], max: usize) -> Vec<&CatalogEntry> {
    let mut refs: Vec<&CatalogEntry> = catalog.iter().collect();
    refs.sort_by(|a, b| {
        b.rating.total_cmp(&a.rating).then_with(|| b.year.unwrap_or(0).cmp(&a.year.unwrap_or(0)))
    });
    refs.truncate(max);
    refs
}

/// JSON shape fragment for one collection, with `title`/`reason` as objects keyed
/// by every supported language code (e.g. `"title":{"en":string,"fr":string}`).
fn collection_shape() -> String {
    let fields = i18n::SUPPORTED_LOCALES
        .iter()
        .map(|l| format!("\"{l}\":string"))
        .collect::<Vec<_>>()
        .join(",");
    format!("{{\"title\":{{{fields}}},\"reason\":{{{fields}}},\"members\":[string]}}")
}

/// The language rule shared by both curate prompts.
fn language_rule() -> String {
    let codes = i18n::SUPPORTED_LOCALES.join(", ");
    format!(
        "- \"title\" and \"reason\" are objects keyed by language code ({codes}) \
         provide EVERY listed language. Title < 5 words; reason is one short clause."
    )
}

/// Build the (system, user) prompt asking the model to curate editorial
/// collections **from the supplied library titles only**.
pub fn build_curate_prompt(catalog: &[&CatalogEntry]) -> (String, String) {
    let shape = collection_shape();
    let lang_rule = language_rule();
    let system = format!(
        "You are the editorial curator of a personal film & TV library. From the catalog \
         below you assemble compelling themed collections a cinephile would browse.\n\
         Reply with STRICT JSON only no prose, no markdown, no code fences an array shaped:\n\
         [{shape}]\n\
         Rules:\n\
         - Curate a VARIED mix: genre showcases (\"Best Horror\"), acclaimed/Top-list style \
         (\"Modern Classics\", \"IMDb Top\"), franchises & sagas, a decade/era, and a mood.\n\
         - \"members\" MUST be titles copied EXACTLY from the catalog below never invent titles. \
         8-30 members each; only include a collection if it has at least {MIN_ITEMS} real members.\n\
         {lang_rule}\n\
         - Return between 6 and {MAX_LLM} distinct collections."
    );

    let mut user = String::from("Catalog (title (year) genres):\n");
    for e in catalog {
        let year = e.year.map(|y| y.to_string()).unwrap_or_default();
        let genres = e.genres.iter().take(2).cloned().collect::<Vec<_>>().join(", ");
        user.push_str(&format!("- {} ({}) {}\n", e.title, year, genres));
    }
    user.push_str("\nReturn the JSON array now.");
    (system, user)
}

/// (system, user) for the **tool-driven** curate flow. The model explores the
/// library with the catalog tools (`list_genres`, `list_people`, `find_titles`,
/// `get_title`) instead of reading a fixed slice, then returns the same JSON
/// shape as [`build_curate_prompt`] except `members` are catalog **ids** the
/// tools returned (resolved by [`resolve_members_by_id`]), so resolution is exact.
pub fn tool_curate_prompt() -> (String, String) {
    let shape = collection_shape();
    let lang_rule = language_rule();
    let system = format!(
        "You are the editorial curator of a personal film & TV library. You have tools to \
         explore it: list_genres, list_people, find_titles (filter by genre / director / actor / \
         keyword / kind / year / rating, with sort + limit) and get_title. Use them to discover \
         what the library actually contains, then assemble compelling, VARIED themed collections \
         a cinephile would browse.\n\
         Plan: call list_genres and list_people to see what's available, then call find_titles to \
         gather the members for each collection idea.\n\
         When done, reply with STRICT JSON only no prose, no markdown, no code fences an array:\n\
         [{shape}]\n\
         Rules:\n\
         - \"members\" MUST be catalog **ids** returned by the tools (each title's \"id\" field) \
         never titles, never invented ids.\n\
         - Curate a varied mix: genre showcases (\"Best Horror\"), acclaimed/top-list style \
         (\"Modern Classics\"), a director or actor spotlight, a franchise/saga, a decade/era, and \
         a mood. {MIN_ITEMS}-30 members each; only keep a collection with at least {MIN_ITEMS} real \
         members.\n\
         {lang_rule}\n\
         - Return between 6 and {MAX_LLM} distinct collections."
    );
    let user = String::from(
        "Explore the library with the tools, then return the JSON array of collections \
         with members as catalog ids.",
    );
    (system, user)
}

/// One editorial collection as the model returned it (pre-resolution). `title`
/// and `reason` are locale-keyed objects (`{"en":…,"fr":…}`) over the supported
/// languages the model was asked to fill.
#[derive(Debug, Deserialize)]
pub struct CuratedSpec {
    #[serde(default)]
    pub title: HashMap<String, String>,
    #[serde(default)]
    pub reason: HashMap<String, String>,
    #[serde(default)]
    pub members: Vec<String>,
}

/// Parse a model reply (tolerant of fences/prose) into curated specs.
pub fn parse_curate(text: &str) -> anyhow::Result<Vec<CuratedSpec>> {
    let json = extract_json_array(text).ok_or_else(|| anyhow::anyhow!("no JSON array in reply"))?;
    let specs = serde_json::from_str::<Vec<CuratedSpec>>(json)?.into_iter().take(MAX_LLM).collect();
    Ok(specs)
}

/// Resolve each spec's member **titles** to real catalog ids (normalized match)
/// the catalog-in-prompt path, where the model echoes titles. Keeps collections
/// with ≥ [`MIN_ITEMS`] matches. Returns `(rows, dropped)`.
pub fn resolve_members(specs: &[CuratedSpec], catalog: &[CatalogEntry]) -> (Vec<CuratedRow>, usize) {
    let mut index: HashMap<String, &str> = HashMap::new();
    for e in catalog {
        index.entry(normalize_title(&e.title)).or_insert(&e.id);
    }
    resolve(specs, |m| index.get(&normalize_title(m)).map(|id| (*id).to_string()))
}

/// Resolve each spec's member **ids** against the catalog (exact id match) the
/// tool-driven path, where members are catalog ids the tools returned, so nothing
/// the model invents resolves. Keeps collections with ≥ [`MIN_ITEMS`] valid ids.
/// Returns `(rows, dropped)`.
pub fn resolve_members_by_id(specs: &[CuratedSpec], catalog: &[CatalogEntry]) -> (Vec<CuratedRow>, usize) {
    let ids: HashSet<&str> = catalog.iter().map(|e| e.id.as_str()).collect();
    resolve(specs, |m| {
        let id = m.trim();
        ids.contains(id).then(|| id.to_string())
    })
}

/// Shared resolver for both paths: map each spec's members to catalog ids via
/// `resolve_one` (per member → its id or `None`), dedup within a collection, drop
/// collections under [`MIN_ITEMS`], and assemble unique-keyed rows. They differ
/// only in `resolve_one` (title-normalized lookup vs exact id membership).
fn resolve(specs: &[CuratedSpec], mut resolve_one: impl FnMut(&str) -> Option<String>) -> (Vec<CuratedRow>, usize) {
    let mut rows = Vec::new();
    let mut dropped = 0usize;
    let mut seen_keys = HashSet::new();
    for spec in specs {
        let mut ids = Vec::new();
        let mut seen = HashSet::new();
        for m in &spec.members {
            if let Some(id) = resolve_one(m) {
                if seen.insert(id.clone()) {
                    ids.push(id);
                }
            }
        }
        if ids.len() < MIN_ITEMS {
            dropped += 1;
            continue;
        }
        if let Some(row) = build_row(spec, ids, &mut seen_keys) {
            rows.push(row);
        }
    }
    (rows, dropped)
}

/// Assemble one row from a spec and its (already ≥ [`MIN_ITEMS`]) resolved member
/// ids: slug the key from the English (else any) title, reject empty/duplicate
/// keys, and keep the non-empty localized title/reason maps. Shared by both
/// resolvers.
fn build_row(spec: &CuratedSpec, member_ids: Vec<String>, seen_keys: &mut HashSet<String>) -> Option<CuratedRow> {
    let title_for_slug = spec
        .title
        .get("en")
        .filter(|s| !s.trim().is_empty())
        .or_else(|| spec.title.values().find(|s| !s.trim().is_empty()))?;
    let key = slug(title_for_slug);
    if key.is_empty() || !seen_keys.insert(key.clone()) {
        return None;
    }
    let titles = clean_map(&spec.title);
    if titles.is_empty() {
        return None;
    }
    Some(CuratedRow {
        key,
        rank: 0,
        source: "llm".to_string(),
        item_ids: member_ids,
        titles,
        reasons: clean_map(&spec.reason),
    })
}

/// Trim and drop empty-string entries from a locale -> string map.
fn clean_map(m: &HashMap<String, String>) -> HashMap<String, String> {
    m.iter()
        .filter(|(_, v)| !v.trim().is_empty())
        .map(|(k, v)| (k.clone(), v.trim().to_string()))
        .collect()
}

/// Normalize a title for matching: drop a trailing `(year)` the model may have
/// copied from the catalog listing, then lowercase and keep only alphanumerics.
/// `"Le Parrain (1972)"` and `"Le Parrain"` both → `"leparrain"`; `"E.T. the
/// Extra-Terrestrial"` → `"etheextraterrestrial"`. (Bare-number titles like
/// `"1917"` keep their digits only a *parenthesized* 4-digit year is dropped.)
fn normalize_title(s: &str) -> String {
    strip_year(s).chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

/// Strip a trailing ` (YYYY)` from a title, if present.
fn strip_year(s: &str) -> &str {
    let t = s.trim_end();
    if let Some(open) = t.strip_suffix(')').and_then(|head| head.rfind('(')) {
        let inner = &t[open + 1..t.len() - 1];
        if inner.len() == 4 && inner.bytes().all(|b| b.is_ascii_digit()) {
            return t[..open].trim_end();
        }
    }
    t
}

/// Outermost JSON array in `text` (handles ```json fences / preamble).
fn extract_json_array(text: &str) -> Option<&str> {
    let start = text.find('[')?;
    let end = text.rfind(']')?;
    (end > start).then(|| &text[start..=end])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, title: &str, rating: f32, director: &str) -> CatalogEntry {
        CatalogEntry {
            id: id.into(),
            title: title.into(),
            year: Some(2000),
            rating,
            genres: vec!["Drama".into()],
            directors: if director.is_empty() { vec![] } else { vec![director.into()] },
        }
    }

    #[test]
    fn director_collection_groups_min_items() {
        let cat: Vec<CatalogEntry> = (0..6)
            .map(|i| entry(&format!("m{i}"), &format!("Film {i}"), i as f32, "Denis Villeneuve"))
            .chain(std::iter::once(entry("x", "Solo", 9.0, "Someone Else")))
            .collect();
        let rows = director_collections(&cat);
        assert_eq!(rows.len(), 1); // only Villeneuve clears MIN_ITEMS
        assert_eq!(rows[0].titles.get("en").map(String::as_str), Some("Denis Villeneuve"));
        assert_eq!(rows[0].titles.get("fr").map(String::as_str), Some("Denis Villeneuve"));
        assert_eq!(rows[0].item_ids.len(), 6);
        assert_eq!(rows[0].item_ids[0], "m5"); // highest-rated first
    }

    #[test]
    fn parse_and_resolve_matches_titles() {
        let reply = r#"Here:```json
        [{"title":{"fr":"Horreur","en":"Best Horror"},"reason":{"fr":"r","en":"scary"},
          "members":["The Shining","Hereditary","Made Up Film","The Mask","It","Alien"]}]```"#;
        let specs = parse_curate(reply).unwrap();
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].title.get("en").map(String::as_str), Some("Best Horror"));
        let cat = vec![
            entry("a", "The Shining", 8.0, ""),
            entry("b", "Hereditary", 7.0, ""),
            entry("c", "The Mask", 6.0, ""),
            entry("d", "It", 7.0, ""),
            entry("e", "Alien", 8.0, ""),
        ];
        let (rows, dropped) = resolve_members(&specs, &cat);
        assert_eq!(dropped, 0);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].key, "best-horror");
        assert_eq!(rows[0].item_ids.len(), 5); // "Made Up Film" dropped (not in catalog)
    }

    #[test]
    fn collection_below_min_is_dropped() {
        let specs = parse_curate(r#"[{"title":{"en":"Tiny"},"members":["A","B"]}]"#).unwrap();
        let cat = vec![entry("a", "A", 1.0, ""), entry("b", "B", 1.0, "")];
        let (rows, dropped) = resolve_members(&specs, &cat);
        assert!(rows.is_empty());
        assert_eq!(dropped, 1);
    }

    #[test]
    fn resolve_by_id_matches_catalog_ids() {
        // Tool-driven path: members are catalog ids; unknown ids are dropped.
        let specs =
            parse_curate(r#"[{"title":{"en":"Nolan","fr":"Nolan"},"members":["a","b","c","zzz","d","e"]}]"#)
                .unwrap();
        let cat: Vec<CatalogEntry> = ["a", "b", "c", "d", "e"].iter().map(|id| entry(id, id, 5.0, "")).collect();
        let (rows, dropped) = resolve_members_by_id(&specs, &cat);
        assert_eq!(dropped, 0);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].item_ids, ["a", "b", "c", "d", "e"]); // "zzz" not in catalog
        assert_eq!(rows[0].key, "nolan");
    }

    fn meta(rating: f32, genres: &[&str], directors: &[&str]) -> crate::model::Metadata {
        crate::model::Metadata {
            provider: "tmdb",
            tmdb_id: 1,
            imdb_id: None,
            title: None,
            tagline: None,
            overview: None,
            release_date: None,
            genres: genres.iter().map(|s| s.to_string()).collect(),
            rating: Some(rating),
            poster_url: None,
            backdrop_url: None,
            logo_url: None,
            theme_url: None,
            cast: Vec::new(),
            crew: directors
                .iter()
                .map(|d| crate::model::CrewMember { name: d.to_string(), job: "Director".to_string(), profile_url: None })
                .collect(),
            keywords: Vec::new(),
            tvdb_id: None,
            tmdb_url: String::new(),
        }
    }

    fn item(id: &str, title: &str, kind: Kind, m: Option<crate::model::Metadata>) -> MediaItem {
        MediaItem {
            id: id.into(),
            title: title.into(),
            kind,
            year: Some(2001),
            duration_ms: None,
            container: String::new(),
            video: None,
            audio: None,
            audio_tracks: Vec::new(),
            subtitles: Vec::new(),
            library: "lib".into(),
            show_id: None,
            show_title: None,
            season: None,
            episode: None,
            episode_end: None,
            episode_title: None,
            rel_path: None,
            added_at: "t".into(),
            metadata: m,
            abs_path: None,
            files: Vec::new(),
            default_file_id: None,
            markers: Vec::new(),
            audio_analysis: None,
        }
    }

    #[test]
    fn build_catalog_skips_episodes_and_flattens_shows() {
        let items = vec![
            item("m1", "Movie One", Kind::Movie, Some(meta(8.0, &["Drama"], &["Kubrick"]))),
            item("e1", "Episode", Kind::Episode, None), // skipped
            item("v1", "Clip", Kind::Video, None), // kept (not an episode)
        ];
        let shows = vec![Show {
            id: "s1".into(),
            title: "Show One".into(),
            year: Some(2010),
            library: "lib".into(),
            season_count: 1,
            episode_count: 3,
            video: None,
            added_at: "t".into(),
            metadata: Some(meta(7.5, &["Comedy"], &["Someone"])),
            progress: None,
        }];
        let cat = build_catalog(&items, &shows);
        let ids: Vec<&str> = cat.iter().map(|e| e.id.as_str()).collect();
        assert!(ids.contains(&"m1"));
        assert!(ids.contains(&"v1"));
        assert!(ids.contains(&"s1"));
        assert!(!ids.contains(&"e1")); // episode excluded
        let m1 = cat.iter().find(|e| e.id == "m1").unwrap();
        assert_eq!(m1.rating, 8.0);
        assert_eq!(m1.directors, vec!["Kubrick"]);
        // No metadata -> zeroed bits.
        let v1 = cat.iter().find(|e| e.id == "v1").unwrap();
        assert_eq!(v1.rating, 0.0);
        assert!(v1.genres.is_empty());
    }

    #[test]
    fn prune_for_prompt_sorts_by_rating_then_year_and_truncates() {
        let cat = vec![
            entry_year("a", "A", 5.0, 1990),
            entry_year("b", "B", 9.0, 2000),
            entry_year("c", "C", 9.0, 2020), // same rating as b, newer year
            entry_year("d", "D", 1.0, 2024),
        ];
        let pruned = prune_for_prompt(&cat, 3);
        let ids: Vec<&str> = pruned.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, vec!["c", "b", "a"]); // 9.0/2020, 9.0/2000, 5.0, and "d" truncated
    }

    #[test]
    fn normalize_title_and_strip_year() {
        assert_eq!(normalize_title("Le Parrain (1972)"), "leparrain");
        assert_eq!(normalize_title("Le Parrain"), "leparrain");
        // Punctuation and separators are stripped; letters lowercased.
        assert_eq!(normalize_title("The Godfather: Part II"), "thegodfatherpartii");
        // A bare-number title keeps its digits (only a parenthesized year is dropped).
        assert_eq!(normalize_title("1917"), "1917");
        assert_eq!(strip_year("Alien (1979)"), "Alien");
        assert_eq!(strip_year("Se7en"), "Se7en");
        // Non-4-digit parenthetical is not treated as a year.
        assert_eq!(strip_year("Movie (12)"), "Movie (12)");
    }

    #[test]
    fn extract_json_array_handles_fences_and_missing() {
        assert_eq!(extract_json_array("noise [1,2] tail"), Some("[1,2]"));
        assert_eq!(extract_json_array("```json\n[{}]\n```"), Some("[{}]"));
        assert!(extract_json_array("no array here").is_none());
        assert!(extract_json_array("]before[").is_none()); // end <= start
    }

    #[test]
    fn parse_curate_rejects_non_array() {
        assert!(parse_curate("just prose, no json").is_err());
    }

    #[test]
    fn resolve_members_dedups_within_a_collection() {
        // Same id twice in members -> counted once.
        let specs = parse_curate(
            r#"[{"title":{"en":"Dup"},"members":["a","a","b","c","d","e"]}]"#,
        )
        .unwrap();
        let cat: Vec<CatalogEntry> = ["a", "b", "c", "d", "e"].iter().map(|id| entry(id, id, 1.0, "")).collect();
        let (rows, _) = resolve_members_by_id(&specs, &cat);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].item_ids.len(), 5); // "a" deduped
    }

    #[test]
    fn prompts_mention_locales_and_min_items() {
        let cat = vec![entry("a", "A", 1.0, "")];
        let refs: Vec<&CatalogEntry> = cat.iter().collect();
        let (system, user) = build_curate_prompt(&refs);
        assert!(system.contains("\"en\":string"));
        assert!(system.contains("\"fr\":string"));
        assert!(user.contains("A ("));
        let (tsystem, _tuser) = tool_curate_prompt();
        assert!(tsystem.contains("list_genres"));
        assert!(tsystem.contains("catalog **ids**"));
    }

    // Extra helper: a catalog entry with an explicit year (the base `entry` fixes 2000).
    fn entry_year(id: &str, title: &str, rating: f32, year: u32) -> CatalogEntry {
        CatalogEntry {
            id: id.into(),
            title: title.into(),
            year: Some(year),
            rating,
            genres: vec!["Drama".into()],
            directors: vec![],
        }
    }
}
