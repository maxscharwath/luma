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
use crate::model::{Kind, MediaItem, Show};

use super::generate::slug;

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
                title_fr: Some(name.to_string()),
                title_en: Some(name.to_string()),
                reason_fr: Some(format!("Réalisé par {name}")),
                reason_en: Some(format!("Directed by {name}")),
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

/// Build the (system, user) prompt asking the model to curate editorial
/// collections **from the supplied library titles only**.
pub fn build_curate_prompt(catalog: &[&CatalogEntry]) -> (String, String) {
    let system = format!(
        "You are the editorial curator of a personal film & TV library. From the catalog \
         below you assemble compelling themed collections a cinephile would browse.\n\
         Reply with STRICT JSON only no prose, no markdown, no code fences an array shaped:\n\
         [{{\"titleFr\":string,\"titleEn\":string,\"reasonFr\":string,\"reasonEn\":string,\"members\":[string]}}]\n\
         Rules:\n\
         - Curate a VARIED mix: genre showcases (\"Best Horror\"), acclaimed/Top-list style \
         (\"Modern Classics\", \"IMDb Top\"), franchises & sagas, a decade/era, and a mood.\n\
         - \"members\" MUST be titles copied EXACTLY from the catalog below never invent titles. \
         8-30 members each; only include a collection if it has at least {MIN_ITEMS} real members.\n\
         - \"titleFr\"/\"reasonFr\" in French, \"titleEn\"/\"reasonEn\" in English. Title < 5 words; \
         reason is one short clause.\n\
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
    let system = format!(
        "You are the editorial curator of a personal film & TV library. You have tools to \
         explore it: list_genres, list_people, find_titles (filter by genre / director / actor / \
         keyword / kind / year / rating, with sort + limit) and get_title. Use them to discover \
         what the library actually contains, then assemble compelling, VARIED themed collections \
         a cinephile would browse.\n\
         Plan: call list_genres and list_people to see what's available, then call find_titles to \
         gather the members for each collection idea.\n\
         When done, reply with STRICT JSON only no prose, no markdown, no code fences an array:\n\
         [{{\"titleFr\":string,\"titleEn\":string,\"reasonFr\":string,\"reasonEn\":string,\"members\":[string]}}]\n\
         Rules:\n\
         - \"members\" MUST be catalog **ids** returned by the tools (each title's \"id\" field) \
         never titles, never invented ids.\n\
         - Curate a varied mix: genre showcases (\"Best Horror\"), acclaimed/top-list style \
         (\"Modern Classics\"), a director or actor spotlight, a franchise/saga, a decade/era, and \
         a mood. {MIN_ITEMS}-30 members each; only keep a collection with at least {MIN_ITEMS} real \
         members.\n\
         - \"titleFr\"/\"reasonFr\" in French, \"titleEn\"/\"reasonEn\" in English. Title < 5 words; \
         reason is one short clause.\n\
         - Return between 6 and {MAX_LLM} distinct collections."
    );
    let user = String::from(
        "Explore the library with the tools, then return the JSON array of collections \
         with members as catalog ids.",
    );
    (system, user)
}

/// One editorial collection as the model returned it (titles, pre-resolution).
#[derive(Debug, Deserialize)]
pub struct CuratedSpec {
    #[serde(default)]
    pub title_fr: String,
    #[serde(default)]
    pub title_en: String,
    #[serde(default)]
    pub reason_fr: String,
    #[serde(default)]
    pub reason_en: String,
    #[serde(default)]
    pub members: Vec<String>,
}

/// Parse a model reply (tolerant of fences/prose) into curated specs.
pub fn parse_curate(text: &str) -> anyhow::Result<Vec<CuratedSpec>> {
    let json = extract_json_array(text).ok_or_else(|| anyhow::anyhow!("no JSON array in reply"))?;
    // camelCase keys from the prompt → snake_case fields.
    let raw: Vec<serde_json::Value> = serde_json::from_str(json)?;
    let specs = raw
        .into_iter()
        .filter_map(|v| serde_json::from_value::<CuratedSpec>(rename_keys(v)).ok())
        .take(MAX_LLM)
        .collect();
    Ok(specs)
}

/// Map an LLM spec object's camelCase keys to our field names.
fn rename_keys(v: serde_json::Value) -> serde_json::Value {
    let serde_json::Value::Object(map) = v else { return v };
    let mut out = serde_json::Map::new();
    for (k, val) in map {
        let key = match k.as_str() {
            "titleFr" => "title_fr",
            "titleEn" => "title_en",
            "reasonFr" => "reason_fr",
            "reasonEn" => "reason_en",
            other => other,
        };
        out.insert(key.to_string(), val);
    }
    serde_json::Value::Object(out)
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
/// ids: pick a slug key from the English (else French) title and reject empty or
/// duplicate keys. Shared by both resolvers.
fn build_row(spec: &CuratedSpec, member_ids: Vec<String>, seen_keys: &mut HashSet<String>) -> Option<CuratedRow> {
    let title = if !spec.title_en.is_empty() { &spec.title_en } else { &spec.title_fr };
    let key = slug(title);
    if key.is_empty() || !seen_keys.insert(key.clone()) {
        return None;
    }
    Some(CuratedRow {
        key,
        rank: 0,
        source: "llm".to_string(),
        item_ids: member_ids,
        title_fr: non_empty(&spec.title_fr),
        title_en: non_empty(&spec.title_en),
        reason_fr: non_empty(&spec.reason_fr),
        reason_en: non_empty(&spec.reason_en),
    })
}

fn non_empty(s: &str) -> Option<String> {
    let t = s.trim();
    (!t.is_empty()).then(|| t.to_string())
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
        assert_eq!(rows[0].title_en.as_deref(), Some("Denis Villeneuve"));
        assert_eq!(rows[0].item_ids.len(), 6);
        assert_eq!(rows[0].item_ids[0], "m5"); // highest-rated first
    }

    #[test]
    fn parse_and_resolve_matches_titles() {
        let reply = r#"Here:```json
        [{"titleFr":"Horreur","titleEn":"Best Horror","reasonFr":"r","reasonEn":"scary",
          "members":["The Shining","Hereditary","Made Up Film","The Mask","It","Alien"]}]```"#;
        let specs = parse_curate(reply).unwrap();
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].title_en, "Best Horror");
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
        let specs = parse_curate(r#"[{"titleEn":"Tiny","members":["A","B"]}]"#).unwrap();
        let cat = vec![entry("a", "A", 1.0, ""), entry("b", "B", 1.0, "")];
        let (rows, dropped) = resolve_members(&specs, &cat);
        assert!(rows.is_empty());
        assert_eq!(dropped, 1);
    }

    #[test]
    fn resolve_by_id_matches_catalog_ids() {
        // Tool-driven path: members are catalog ids; unknown ids are dropped.
        let specs =
            parse_curate(r#"[{"titleEn":"Nolan","titleFr":"Nolan","members":["a","b","c","zzz","d","e"]}]"#)
                .unwrap();
        let cat: Vec<CatalogEntry> = ["a", "b", "c", "d", "e"].iter().map(|id| entry(id, id, 5.0, "")).collect();
        let (rows, dropped) = resolve_members_by_id(&specs, &cat);
        assert_eq!(dropped, 0);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].item_ids, ["a", "b", "c", "d", "e"]); // "zzz" not in catalog
        assert_eq!(rows[0].key, "nolan");
    }
}
