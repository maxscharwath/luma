//! The **catalog connector** LUMA's library exposed to an LLM as callable
//! tools ([`crate::infra::llm::ToolBox`]).
//!
//! Instead of stuffing hundreds of titles into a prompt, the model *asks* for
//! exactly what it needs list titles by genre / director / actor, fetch a
//! title, enumerate genres / people each answered by a read-only
//! [`db::catalog_query`](crate::db) query. Tools are registered in one [`SPECS`]
//! table (name → JSON-Schema → handler), so adding a capability is a single
//! entry. This is the reusable foundation any LLM feature can build on (curate
//! today; library chat / tool-driven personalize next).

use anyhow::{anyhow, Result};
use serde_json::{json, Value};

use crate::db::{self, Pool, TitleFilter};
use crate::infra::llm::{ToolBox, ToolDef};

/// The catalog connector. Holds the DB pool and an optional logger (so a job can
/// surface each tool call in its Tâches run log via
/// [`JobContext::debug_logger`](crate::services::jobs::JobContext::debug_logger)).
pub struct CatalogTools {
    pool: Pool,
    log: Option<Box<dyn Fn(String) + Send + Sync>>,
}

impl CatalogTools {
    /// `log: Some(..)` logs every tool call (name + args + result size); `None`
    /// runs silently. Jobs pass a [`JobContext::debug_logger`] so calls land in
    /// the Tâches run view.
    pub fn new(pool: Pool, log: Option<Box<dyn Fn(String) + Send + Sync>>) -> Self {
        Self { pool, log }
    }
}

impl ToolBox for CatalogTools {
    fn defs(&self) -> Vec<ToolDef> {
        SPECS
            .iter()
            .map(|s| ToolDef { name: s.name.to_string(), description: s.description.to_string(), schema: (s.schema)() })
            .collect()
    }

    fn call(&self, name: &str, args: &Value) -> Result<String> {
        let spec = SPECS.iter().find(|s| s.name == name).ok_or_else(|| anyhow!("unknown tool '{name}'"))?;
        let out = (spec.handler)(self, args)?;
        if let Some(log) = &self.log {
            log(format!("tool {name} {args} → {} bytes", out.len()));
        }
        Ok(out)
    }
}

/// One registered tool: wire name, model-facing description, its argument
/// JSON-Schema, and the handler that runs it. Adding a tool = one entry here.
struct ToolSpec {
    name: &'static str,
    description: &'static str,
    schema: fn() -> Value,
    handler: fn(&CatalogTools, &Value) -> Result<String>,
}

const SPECS: &[ToolSpec] = &[
    ToolSpec {
        name: "find_titles",
        description: "List movies/shows from the library matching filters (genre, director, actor, \
                      keyword, kind, year range, min rating). Returns each title's id, title, year, \
                      kind, rating and genres. Use the returned ids as collection members.",
        schema: schema_find_titles,
        handler: handle_find_titles,
    },
    ToolSpec {
        name: "get_title",
        description: "Fetch one title's full data (rating, genres, directors, cast, synopsis, tagline) \
                      by id (preferred) or exact title.",
        schema: schema_get_title,
        handler: handle_get_title,
    },
    ToolSpec {
        name: "list_genres",
        description: "List every genre in the library with how many titles carry it (most common first). \
                      Use the exact names returned when filtering find_titles by genre.",
        schema: schema_empty,
        handler: handle_list_genres,
    },
    ToolSpec {
        name: "list_people",
        description: "List the most-credited people and their title counts directors/creators \
                      (role=director, default) or actors (role=actor).",
        schema: schema_list_people,
        handler: handle_list_people,
    },
];

// ----- handlers ---------------------------------------------------------------

fn handle_find_titles(t: &CatalogTools, args: &Value) -> Result<String> {
    let filter = TitleFilter {
        genre: arg_str(args, "genre"),
        director: arg_str(args, "director"),
        actor: arg_str(args, "actor"),
        keyword: arg_str(args, "keyword"),
        kind: arg_str(args, "kind"),
        year_min: arg_u32(args, "yearMin"),
        year_max: arg_u32(args, "yearMax"),
        min_rating: arg_f32(args, "minRating"),
        sort: arg_str(args, "sort"),
        limit: arg_usize(args, "limit"),
    };
    let titles = db::find_titles(&t.pool, &filter)?;
    let rows: Vec<Value> = titles
        .iter()
        .map(|x| json!({ "id": x.id, "title": x.title, "year": x.year, "kind": x.kind, "rating": x.rating, "genres": x.genres }))
        .collect();
    Ok(json!({ "count": rows.len(), "titles": rows }).to_string())
}

fn handle_get_title(t: &CatalogTools, args: &Value) -> Result<String> {
    let q = arg_str(args, "id").or_else(|| arg_str(args, "title")).unwrap_or_default();
    match db::get_title(&t.pool, &q)? {
        Some(x) => Ok(json!({
            "id": x.id, "title": x.title, "year": x.year, "kind": x.kind, "rating": x.rating,
            "genres": x.genres, "directors": x.directors, "cast": x.cast,
            "overview": x.overview, "tagline": x.tagline,
        })
        .to_string()),
        None => Ok(json!({ "found": false, "query": q }).to_string()),
    }
}

fn handle_list_genres(t: &CatalogTools, _args: &Value) -> Result<String> {
    let counts = db::genre_counts(&t.pool)?;
    let rows: Vec<Value> = counts.iter().map(|(g, n)| json!({ "genre": g, "count": n })).collect();
    Ok(json!({ "genres": rows }).to_string())
}

fn handle_list_people(t: &CatalogTools, args: &Value) -> Result<String> {
    let role = arg_str(args, "role").unwrap_or_else(|| "director".into());
    let limit = arg_usize(args, "limit").unwrap_or(20);
    let counts = db::people_counts(&t.pool, &role, limit)?;
    let rows: Vec<Value> = counts.iter().map(|(name, n)| json!({ "name": name, "count": n })).collect();
    Ok(json!({ "role": role, "people": rows }).to_string())
}

// ----- schemas ----------------------------------------------------------------

fn schema_find_titles() -> Value {
    json!({
        "type": "object",
        "properties": {
            "genre": { "type": "string", "description": "Exact genre name (see list_genres)" },
            "director": { "type": "string", "description": "Director/creator full name" },
            "actor": { "type": "string", "description": "Cast member full name" },
            "keyword": { "type": "string", "description": "Free-text match over title and synopsis" },
            "kind": { "type": "string", "enum": ["movie", "show"], "description": "Restrict to movies or shows" },
            "yearMin": { "type": "integer" },
            "yearMax": { "type": "integer" },
            "minRating": { "type": "number", "description": "Minimum rating, 0-10" },
            "sort": { "type": "string", "enum": ["rating", "year", "title"], "description": "Default: rating" },
            "limit": { "type": "integer", "description": "Max rows (default 25, max 50)" }
        }
    })
}

fn schema_get_title() -> Value {
    json!({
        "type": "object",
        "properties": {
            "id": { "type": "string", "description": "Catalog id (preferred)" },
            "title": { "type": "string", "description": "Exact title, if the id is unknown" }
        }
    })
}

fn schema_empty() -> Value {
    json!({ "type": "object", "properties": {} })
}

fn schema_list_people() -> Value {
    json!({
        "type": "object",
        "properties": {
            "role": { "type": "string", "enum": ["director", "actor"], "description": "Default: director" },
            "limit": { "type": "integer", "description": "Max people (default 20)" }
        }
    })
}

// ----- arg coercion (tolerant of stringly-typed numbers) ----------------------

fn arg_str(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).map(str::to_string)
}
fn arg_u32(v: &Value, k: &str) -> Option<u32> {
    num_of(v.get(k)?).map(|n| n.max(0.0) as u32)
}
fn arg_f32(v: &Value, k: &str) -> Option<f32> {
    num_of(v.get(k)?).map(|n| n as f32)
}
fn arg_usize(v: &Value, k: &str) -> Option<usize> {
    num_of(v.get(k)?).map(|n| n.max(0.0) as usize)
}
/// A JSON number, tolerating a numeric string (models sometimes send `"2015"`).
fn num_of(v: &Value) -> Option<f64> {
    v.as_f64().or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))
}
