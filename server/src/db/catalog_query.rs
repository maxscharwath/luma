//! Read-only catalog queries powering the **LLM connector** (`services::llm`).
//!
//! Where the rest of `db` serves the UI, these answer the questions a model asks
//! while curating: *"horror titles rated ≥7, newest first"*, *"everything Nolan
//! directed"*, *"which genres exist"*. All of it runs over the `metadata` JSON
//! column with SQLite's JSON1 (`json_each` / `json_extract`), across **movies and
//! shows** at once via a small `cat` CTE so a tool query spans the whole
//! library without the caller stitching two tables together.

use super::*;

use rusqlite::types::Value as SqlValue;
use rusqlite::OptionalExtension;

use crate::model::Metadata;

/// Hard cap on rows a single `find_titles` returns (keeps tool results which
/// re-enter the model's context bounded).
const MAX_LIMIT: usize = 50;
const DEFAULT_LIMIT: usize = 25;

/// Movies (sans episodes) + shows unioned into one `(id,title,year,kind,metadata)`
/// relation. Every query below selects `FROM cat`. Trailing space is intentional
/// (the per-query `SELECT …` is concatenated after it).
const CAT_CTE: &str = "WITH cat(id,title,year,kind,metadata) AS (\
    SELECT id,title,year,'movie',metadata FROM items WHERE kind != 'episode' \
    UNION ALL SELECT id,title,year,'show',metadata FROM shows) ";

/// Crew jobs that count as "directed/created by" (matches the deterministic
/// director collections).
const DIRECTING_JOBS_SQL: &str = "('Director','Creator')";

/// A title in brief form (one `find_titles` row).
pub struct TitleBrief {
    pub id: String,
    pub title: String,
    pub year: Option<u32>,
    /// `"movie"` | `"show"`.
    pub kind: String,
    pub rating: Option<f32>,
    pub genres: Vec<String>,
}

/// A title in full form (`get_title`) adds people, synopsis, tagline.
pub struct TitleFull {
    pub id: String,
    pub title: String,
    pub year: Option<u32>,
    pub kind: String,
    pub rating: Option<f32>,
    pub genres: Vec<String>,
    pub directors: Vec<String>,
    pub cast: Vec<String>,
    pub overview: Option<String>,
    pub tagline: Option<String>,
}

/// Composable `find_titles` filters all optional, AND-ed together.
#[derive(Default)]
pub struct TitleFilter {
    pub genre: Option<String>,
    pub director: Option<String>,
    pub actor: Option<String>,
    /// Free-text match over title + overview (TMDB keywords aren't persisted).
    pub keyword: Option<String>,
    /// `"movie"` | `"show"` (synonyms: series/tv → show).
    pub kind: Option<String>,
    pub year_min: Option<u32>,
    pub year_max: Option<u32>,
    pub min_rating: Option<f32>,
    /// `"rating"` (default) | `"year"` | `"title"`.
    pub sort: Option<String>,
    pub limit: Option<usize>,
}

/// List titles matching `filter`, ordered + capped. The heavy lifting is in
/// SQL; genres/rating are pulled from the parsed metadata so the struct stays
/// faithful to the model types.
pub fn find_titles(pool: &Pool, filter: &TitleFilter) -> Result<Vec<TitleBrief>> {
    let mut sql = String::from(CAT_CTE);
    sql.push_str("SELECT id,title,year,kind,metadata FROM cat WHERE 1=1");
    let mut p: Vec<SqlValue> = Vec::new();

    if let Some(g) = clean(&filter.genre) {
        sql.push_str(" AND EXISTS (SELECT 1 FROM json_each(cat.metadata,'$.genres') g WHERE g.value = ? COLLATE NOCASE)");
        p.push(SqlValue::Text(g));
    }
    if let Some(d) = clean(&filter.director) {
        sql.push_str(&format!(
            " AND EXISTS (SELECT 1 FROM json_each(cat.metadata,'$.crew') c \
              WHERE json_extract(c.value,'$.name') = ? COLLATE NOCASE \
              AND json_extract(c.value,'$.job') IN {DIRECTING_JOBS_SQL})"
        ));
        p.push(SqlValue::Text(d));
    }
    if let Some(a) = clean(&filter.actor) {
        sql.push_str(" AND EXISTS (SELECT 1 FROM json_each(cat.metadata,'$.cast') c WHERE json_extract(c.value,'$.name') = ? COLLATE NOCASE)");
        p.push(SqlValue::Text(a));
    }
    if let Some(k) = clean(&filter.keyword) {
        sql.push_str(" AND (cat.title LIKE ? COLLATE NOCASE OR IFNULL(json_extract(cat.metadata,'$.overview'),'') LIKE ? COLLATE NOCASE)");
        let like = format!("%{k}%");
        p.push(SqlValue::Text(like.clone()));
        p.push(SqlValue::Text(like));
    }
    if let Some(k) = clean(&filter.kind) {
        sql.push_str(" AND cat.kind = ?");
        p.push(SqlValue::Text(normalize_kind(&k)));
    }
    if let Some(y) = filter.year_min {
        sql.push_str(" AND cat.year >= ?");
        p.push(SqlValue::Integer(y as i64));
    }
    if let Some(y) = filter.year_max {
        sql.push_str(" AND cat.year <= ?");
        p.push(SqlValue::Integer(y as i64));
    }
    if let Some(r) = filter.min_rating {
        sql.push_str(" AND CAST(json_extract(cat.metadata,'$.rating') AS REAL) >= ?");
        p.push(SqlValue::Real(r as f64));
    }

    sql.push(' ');
    sql.push_str(order_clause(filter.sort.as_deref()));
    let lim = filter.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    sql.push_str(&format!(" LIMIT {lim}"));

    let conn = pool.get()?;
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(p.iter()), |r| {
            let meta = parse_metadata(r.get::<_, Option<String>>(4)?);
            Ok(TitleBrief {
                id: r.get(0)?,
                title: r.get(1)?,
                year: r.get(2)?,
                kind: r.get(3)?,
                rating: meta.as_ref().and_then(|m| m.rating),
                genres: meta.map(|m| m.genres).unwrap_or_default(),
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// Fetch one title's full data by **id first, else exact title** (case-
/// insensitive; highest-rated on a tie). `None` when nothing matches.
pub fn get_title(pool: &Pool, query: &str) -> Result<Option<TitleFull>> {
    let q = query.trim();
    if q.is_empty() {
        return Ok(None);
    }
    let conn = pool.get()?;
    let sql = format!(
        "{CAT_CTE}SELECT id,title,year,kind,metadata FROM cat \
         WHERE id = ?1 OR title = ?1 COLLATE NOCASE \
         ORDER BY (id = ?1) DESC, CAST(json_extract(cat.metadata,'$.rating') AS REAL) DESC LIMIT 1"
    );
    let row = conn
        .query_row(&sql, params![q], |r| {
            let meta = parse_metadata(r.get::<_, Option<String>>(4)?);
            Ok(full_from(r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, meta))
        })
        .optional()?;
    Ok(row)
}

/// Every genre present, with how many titles carry it (most common first).
pub fn genre_counts(pool: &Pool) -> Result<Vec<(String, usize)>> {
    let conn = pool.get()?;
    let sql = format!(
        "{CAT_CTE}SELECT g.value AS genre, COUNT(*) AS n \
         FROM cat, json_each(cat.metadata,'$.genres') g \
         GROUP BY genre ORDER BY n DESC, genre"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as usize)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// Most-credited people for a `role` (`"director"` default | `"actor"`), with
/// their title counts. Capped at `limit` (1..=100).
pub fn people_counts(pool: &Pool, role: &str, limit: usize) -> Result<Vec<(String, usize)>> {
    let lim = limit.clamp(1, 100);
    let sql = match role.trim().to_ascii_lowercase().as_str() {
        "actor" | "cast" => format!(
            "{CAT_CTE}SELECT json_extract(c.value,'$.name') AS name, COUNT(*) AS n \
             FROM cat, json_each(cat.metadata,'$.cast') c \
             WHERE name IS NOT NULL AND name != '' \
             GROUP BY name ORDER BY n DESC, name LIMIT {lim}"
        ),
        _ => format!(
            "{CAT_CTE}SELECT json_extract(c.value,'$.name') AS name, COUNT(*) AS n \
             FROM cat, json_each(cat.metadata,'$.crew') c \
             WHERE json_extract(c.value,'$.job') IN {DIRECTING_JOBS_SQL} AND name IS NOT NULL AND name != '' \
             GROUP BY name ORDER BY n DESC, name LIMIT {lim}"
        ),
    };
    let conn = pool.get()?;
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as usize)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

// ----- helpers ----------------------------------------------------------------

fn full_from(id: String, title: String, year: Option<u32>, kind: String, meta: Option<Metadata>) -> TitleFull {
    let Some(m) = meta else {
        return TitleFull {
            id, title, year, kind,
            rating: None, genres: Vec::new(), directors: Vec::new(),
            cast: Vec::new(), overview: None, tagline: None,
        };
    };
    let directors = m
        .crew
        .iter()
        .filter(|c| matches!(c.job.as_str(), "Director" | "Creator"))
        .map(|c| c.name.clone())
        .collect();
    let cast = m.cast.iter().take(10).map(|c| c.name.clone()).collect();
    TitleFull {
        id, title, year, kind,
        rating: m.rating,
        genres: m.genres,
        directors,
        cast,
        overview: m.overview,
        tagline: m.tagline,
    }
}

/// `ORDER BY` for the requested sort. Unrated titles sink last under `rating`
/// (SQLite sorts NULL last under `DESC`).
fn order_clause(sort: Option<&str>) -> &'static str {
    match sort.map(str::trim).unwrap_or("rating") {
        "year" => "ORDER BY cat.year DESC",
        "title" => "ORDER BY cat.title COLLATE NOCASE ASC",
        _ => "ORDER BY CAST(json_extract(cat.metadata,'$.rating') AS REAL) DESC, cat.year DESC",
    }
}

fn normalize_kind(k: &str) -> String {
    match k.trim().to_ascii_lowercase().as_str() {
        "show" | "series" | "tv" | "serie" => "show".to_string(),
        _ => "movie".to_string(),
    }
}

/// Trim a filter string and treat blank as absent.
fn clean(s: &Option<String>) -> Option<String> {
    s.as_deref().map(str::trim).filter(|t| !t.is_empty()).map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    static SEQ: AtomicU32 = AtomicU32::new(0);

    /// A fresh temp-file DB seeded with a small movies+shows catalog.
    fn seeded_pool() -> Pool {
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("luma-catq-{}-{n}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let pool = crate::db::init(&path).unwrap();
        let conn = pool.get().unwrap();
        conn.execute(
            "INSERT INTO libraries (id,name,kind,path,added_at) VALUES ('lib','L','movie','/x','t')",
            [],
        )
        .unwrap();

        let meta = |genres: &[&str], rating: f64, director: &str, actor: &str| {
            serde_json::json!({
                "tmdbId": 1, "tmdbUrl": "x",
                "genres": genres,
                "rating": rating,
                "overview": "a film about things",
                "crew": [{"name": director, "job": "Director"}],
                "cast": [{"name": actor}],
            })
            .to_string()
        };
        let movie = |id: &str, title: &str, year: i64, m: String| {
            conn.execute(
                "INSERT INTO items (id,kind,title,year,container,library,added_at,metadata) \
                 VALUES (?1,'movie',?2,?3,'mkv','lib','t',?4)",
                params![id, title, year, m],
            )
            .unwrap();
        };
        movie("m1", "Dune", 2021, meta(&["Science Fiction"], 8.0, "Denis Villeneuve", "Timothée Chalamet"));
        movie("m2", "Sicario", 2015, meta(&["Thriller", "Crime"], 7.6, "Denis Villeneuve", "Emily Blunt"));
        movie("m3", "The Shining", 1980, meta(&["Horror"], 8.4, "Stanley Kubrick", "Jack Nicholson"));
        movie("m4", "Hereditary", 2018, meta(&["Horror"], 7.3, "Ari Aster", "Toni Collette"));
        movie("m5", "Old Unrated", 1990, "{\"tmdbId\":2,\"tmdbUrl\":\"x\",\"genres\":[\"Drama\"]}".to_string());

        // A show + an episode (the episode must be excluded from the catalog).
        conn.execute(
            "INSERT INTO shows (id,library,title,year,added_at,metadata) VALUES ('s1','lib','Severance',2022,'t',?1)",
            params![meta(&["Science Fiction", "Drama"], 8.7, "Ben Stiller", "Adam Scott")],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO items (id,kind,title,container,library,show_id,season,episode,added_at) \
             VALUES ('e1','episode','Ep1','mkv','lib','s1',1,1,'t')",
            [],
        )
        .unwrap();
        drop(conn);
        pool
    }

    #[test]
    fn genre_filter_and_kind() {
        let pool = seeded_pool();
        let horror = find_titles(&pool, &TitleFilter { genre: Some("Horror".into()), ..Default::default() }).unwrap();
        let ids: Vec<&str> = horror.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, ["m3", "m4"]); // rating DESC: Shining 8.4 then Hereditary 7.3

        // Sci-fi spans a movie + a show; kind filter narrows to the show.
        let scifi = find_titles(&pool, &TitleFilter { genre: Some("science fiction".into()), ..Default::default() }).unwrap();
        assert_eq!(scifi.len(), 2);
        let show_only =
            find_titles(&pool, &TitleFilter { genre: Some("Science Fiction".into()), kind: Some("series".into()), ..Default::default() })
                .unwrap();
        assert_eq!(show_only.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), ["s1"]);
        assert_eq!(show_only[0].kind, "show");
    }

    #[test]
    fn director_and_actor_filters() {
        let pool = seeded_pool();
        let dv = find_titles(&pool, &TitleFilter { director: Some("Denis Villeneuve".into()), ..Default::default() }).unwrap();
        assert_eq!(dv.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), ["m1", "m2"]);
        let blunt = find_titles(&pool, &TitleFilter { actor: Some("Emily Blunt".into()), ..Default::default() }).unwrap();
        assert_eq!(blunt.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), ["m2"]);
    }

    #[test]
    fn rating_year_sort_and_limit() {
        let pool = seeded_pool();
        let top = find_titles(&pool, &TitleFilter { min_rating: Some(8.0), ..Default::default() }).unwrap();
        // ≥8.0: Severance 8.7, Shining 8.4, Dune 8.0 newest sort would differ.
        assert_eq!(top.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), ["s1", "m3", "m1"]);

        let newest = find_titles(&pool, &TitleFilter { sort: Some("year".into()), limit: Some(2), ..Default::default() }).unwrap();
        assert_eq!(newest.len(), 2);
        assert_eq!(newest[0].id, "s1"); // 2022 newest

        // Episodes never appear in the catalog.
        let all = find_titles(&pool, &TitleFilter { limit: Some(50), ..Default::default() }).unwrap();
        assert!(all.iter().all(|t| t.id != "e1"));
        assert_eq!(all.len(), 6); // 5 movies + 1 show
    }

    #[test]
    fn get_title_and_counts() {
        let pool = seeded_pool();
        let dune = get_title(&pool, "Dune").unwrap().unwrap();
        assert_eq!(dune.id, "m1");
        assert_eq!(dune.directors, ["Denis Villeneuve"]);
        assert_eq!(dune.cast, ["Timothée Chalamet"]);
        assert_eq!(get_title(&pool, "m3").unwrap().unwrap().title, "The Shining");
        assert!(get_title(&pool, "Nonexistent").unwrap().is_none());

        let genres = genre_counts(&pool).unwrap();
        let horror = genres.iter().find(|(g, _)| g == "Horror").unwrap();
        assert_eq!(horror.1, 2);

        let directors = people_counts(&pool, "director", 10).unwrap();
        assert_eq!(directors[0], ("Denis Villeneuve".to_string(), 2)); // most prolific first
    }

    #[test]
    fn titles_by_person_spans_cast_and_crew() {
        let pool = seeded_pool();

        // Crew credit (and case-insensitive): the two films Villeneuve directed.
        let (mut movies, shows) = crate::db::titles_by_person(&pool, "denis villeneuve").unwrap();
        movies.sort();
        assert_eq!(movies, ["m1", "m2"]);
        assert!(shows.is_empty());

        // Cast credit on a show (and the episode is never returned on its own).
        let (movies, shows) = crate::db::titles_by_person(&pool, "Adam Scott").unwrap();
        assert!(movies.is_empty());
        assert_eq!(shows, ["s1"]);

        // Cast credit on a movie.
        let (movies, _) = crate::db::titles_by_person(&pool, "Timothée Chalamet").unwrap();
        assert_eq!(movies, ["m1"]);

        // Unknown person / blank name → nothing.
        assert_eq!(crate::db::titles_by_person(&pool, "Nobody").unwrap(), (vec![], vec![]));
        assert_eq!(crate::db::titles_by_person(&pool, "  ").unwrap(), (vec![], vec![]));
    }
}
