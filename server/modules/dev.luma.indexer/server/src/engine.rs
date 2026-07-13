//! The search engine core: turn a [`Query`] into concrete HTTP requests, and
//! turn a fetched response body into [`Release`]s. Pure and I/O-free - the
//! transport (fetch, login, download resolution) lives in [`crate::session`],
//! so everything here is unit-testable against a fixed body.

use std::collections::HashMap;

use scraper::ElementRef;

use crate::category;
use crate::context::Context;
use crate::definition::{Definition, Field};
use crate::selector;
use crate::template;
use crate::{filters, IndexerConfig, Query, Release};

/// One prepared search request against the tracker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchRequest {
    pub url: String,
    /// `GET` (default) or `POST`.
    pub method: String,
    /// Query params (GET) or form fields (POST), already rendered.
    pub inputs: Vec<(String, String)>,
    /// `html` (default) | `json` | `xml`.
    pub response_kind: String,
}

/// Build the ordered list of requests to run for `query`, restricted to the
/// wanted Newznab categories.
pub fn build_requests(
    def: &Definition,
    cfg: &IndexerConfig,
    query: &Query,
    wanted_cats: &[u32],
) -> Vec<SearchRequest> {
    let mut ctx = Context::with_config(def, cfg);
    ctx.query = query_attributes(query);
    // Keywords, after the definition's keywordsfilters.
    let base_keywords = query.keywords();
    ctx.keywords = base_keywords.clone();
    ctx.keywords = filters::apply(&base_keywords, &def.search.keywordsfilters, &ctx);
    ctx.query.insert("Keywords".to_string(), ctx.keywords.clone());
    ctx.categories = category::tracker_ids_for(def, wanted_cats);

    // The base link can itself be a template (`{{ .Config.apiurl }}` on
    // API/private trackers whose site URL is a user setting); render it and
    // expose the resolved value as `.Config.sitelink`.
    let base = template::render(&cfg.base_url, &ctx);
    ctx.config.insert("sitelink".to_string(), crate::context::Value::Str(base.clone()));

    let mut requests = Vec::new();
    for path in &def.search.paths {
        let rendered = template::render(&path.path, &ctx);
        let url = join_url(&base, &rendered);
        let mut inputs: Vec<(String, String)> = Vec::new();
        for (k, v) in def.search.inputs.iter().chain(path.inputs.iter()) {
            inputs.push((k.clone(), template::render(v, &ctx)));
        }
        let response_kind = path
            .response
            .as_ref()
            .map(|r| r.kind.clone())
            .filter(|k| !k.is_empty())
            .unwrap_or_else(|| "html".to_string());
        requests.push(SearchRequest {
            url,
            method: path.method.clone().unwrap_or_else(|| "get".to_string()).to_lowercase(),
            inputs,
            response_kind,
        });
    }
    requests
}

/// The `.Query.*` namespace for a query.
fn query_attributes(query: &Query) -> HashMap<String, String> {
    let mut m = HashMap::new();
    let mut set = |k: &str, v: String| {
        if !v.is_empty() {
            m.insert(k.to_string(), v);
        }
    };
    match query {
        Query::Movie { tmdb_id, imdb_id, title: _, year } => {
            set("Type", "movie".into());
            if let Some(id) = tmdb_id {
                set("TMDBID", id.to_string());
            }
            if let Some(imdb) = imdb_id {
                let bare = imdb.trim_start_matches("tt");
                set("IMDBID", format!("tt{bare}"));
                set("IMDBIDShort", bare.to_string());
            }
            if let Some(y) = year {
                set("Year", y.to_string());
            }
        }
        Query::Episode { tmdb_id, season, episode, .. } => {
            set("Type", "tv".into());
            if let Some(id) = tmdb_id {
                set("TMDBID", id.to_string());
            }
            set("Season", season.to_string());
            set("Ep", episode.to_string());
            set("Episode", episode.to_string());
        }
        Query::Season { tmdb_id, season, .. } => {
            set("Type", "tv".into());
            if let Some(id) = tmdb_id {
                set("TMDBID", id.to_string());
            }
            set("Season", season.to_string());
        }
        Query::Text { .. } => {
            set("Type", "search".into());
        }
    }
    m
}

/// Join a base URL with a (possibly absolute) path.
pub fn join_url(base: &str, path: &str) -> String {
    let p = path.trim();
    if p.starts_with("http://") || p.starts_with("https://") || p.starts_with("magnet:") {
        return p.to_string();
    }
    let base = base.trim_end_matches('/');
    let p = p.trim_start_matches('/');
    format!("{base}/{p}")
}

/// Apply the definition's `search.preprocessingfilters` to a raw response body
/// before it is parsed (e.g. strip a JSONP wrapper / leading junk). No-op when
/// none are declared.
pub fn preprocess(def: &Definition, cfg: &IndexerConfig, body: &str) -> String {
    if def.search.preprocessingfilters.is_empty() {
        return body.to_string();
    }
    let ctx = base_context(def, cfg);
    filters::apply(body, &def.search.preprocessingfilters, &ctx)
}

// ----- HTML result parsing --------------------------------------------------------

/// Does this definition select with XPath (rather than CSS)? Checked on the
/// rows selector and every field selector; definitions are internally
/// consistent, so any hit routes the whole parse to the XPath path.
pub fn uses_xpath(def: &Definition) -> bool {
    let row = def.search.rows.selector.as_deref().is_some_and(selector::is_xpath);
    row || def.search.fields.values().any(|f| f.selector.as_deref().is_some_and(selector::is_xpath))
}

/// Parse an HTML search response into releases, routing XPath definitions to
/// the (optional) libxml path.
pub fn parse_html_auto(def: &Definition, cfg: &IndexerConfig, body: &str) -> anyhow::Result<Vec<Release>> {
    if uses_xpath(def) {
        #[cfg(feature = "xpath")]
        {
            return crate::xpath::parse_html(def, cfg, body);
        }
        #[cfg(not(feature = "xpath"))]
        {
            anyhow::bail!(
                "definition '{}' uses XPath selectors; rebuild luma-indexer with the `xpath` feature",
                def.id
            );
        }
    }
    parse_html(def, cfg, body)
}

/// Parse an HTML search response into releases (CSS path).
pub fn parse_html(def: &Definition, cfg: &IndexerConfig, body: &str) -> anyhow::Result<Vec<Release>> {
    let doc = selector::parse_document(body);
    let root = doc.root_element();
    let base_ctx = base_context(def, cfg);

    let row_sel = def
        .search
        .rows
        .selector
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("definition has no rows selector"))?;
    // The rows selector can itself be templated (`{{ .Config.uploader }}`).
    let row_sel = template::render(row_sel, &base_ctx);

    let mut releases = Vec::new();
    for row in selector::select_all(root, &row_sel) {
        if let Some(result) = extract_row_html(def, &base_ctx, row) {
            releases.push(to_release(def, cfg, &result));
        }
    }
    Ok(releases)
}

/// A context with config seeded (query/keywords left empty - result parsing
/// only needs `.Config.*` and `.Result.*`).
fn base_context(def: &Definition, cfg: &IndexerConfig) -> Context {
    Context::with_config(def, cfg)
}

/// Extract all fields for one HTML row. Returns `None` when a required
/// (non-optional, no-default) field is missing - that release is skipped.
fn extract_row_html(def: &Definition, base_ctx: &Context, row: ElementRef) -> Option<HashMap<String, String>> {
    let mut result: HashMap<String, String> = HashMap::new();
    for (name, field) in &def.search.fields {
        let mut ctx = base_ctx.clone();
        ctx.result = result.clone();
        let value = resolve_field_html(field, row, &ctx)?;
        result.insert(name.clone(), value);
    }
    Some(result)
}

/// Resolve one field against an HTML row. `None` signals a required miss.
fn resolve_field_html(field: &Field, row: ElementRef, ctx: &Context) -> Option<String> {
    // 1) Raw value from text template / case switch / selector / row itself.
    let raw: Option<String> = if let Some(text) = &field.text {
        Some(template::render(text, ctx))
    } else if !field.case.is_empty() {
        eval_case_html(field, row, ctx)
    } else if let Some(sel) = &field.selector {
        let sel = template::render(sel, ctx);
        selector::select_first(row, &sel).map(|el| read_element(field, el, ctx))
    } else {
        // No locator: read the row element itself.
        Some(read_element(field, row, ctx))
    };

    // 2) Fall back to `default`, then honor optional/required semantics.
    let value = match raw {
        Some(v) => v,
        None => match &field.default {
            Some(d) => template::render(d, ctx),
            None if field.optional => String::new(),
            None => return None, // required field missing -> skip the row
        },
    };

    // 3) Field filters.
    Some(filters::apply(&value, &field.filters, ctx))
}

/// Read text (with `remove:`) or an attribute from a matched element.
fn read_element(field: &Field, el: ElementRef, _ctx: &Context) -> String {
    if let Some(attr) = &field.attribute {
        selector::element_attr(el, attr).unwrap_or_default()
    } else if let Some(remove) = &field.remove {
        selector::element_text_removing(el, remove)
    } else {
        selector::element_text(el)
    }
}

/// `case:` switch - first sub-selector that matches wins; `*` is the default.
fn eval_case_html(field: &Field, row: ElementRef, ctx: &Context) -> Option<String> {
    let mut default: Option<&String> = None;
    for (sel, val) in &field.case {
        if sel == "*" {
            default = Some(val);
            continue;
        }
        let rendered = template::render(sel, ctx);
        if selector::select_first(row, &rendered).is_some() {
            return Some(template::render(val, ctx));
        }
    }
    default.map(|d| template::render(d, ctx))
}

// ----- JSON result parsing --------------------------------------------------------

/// Parse a JSON search response into releases (Cardigann `response: type: json`).
/// Row/field selectors are dotted JSON paths (`$.data.torrents`, `results`,
/// `foo[0].bar`); `text` templates and filters work exactly as for HTML.
pub fn parse_json(def: &Definition, cfg: &IndexerConfig, body: &str) -> anyhow::Result<Vec<Release>> {
    let root: serde_json::Value = serde_json::from_str(body)?;
    let base_ctx = base_context(def, cfg);

    let row_sel = def
        .search
        .rows
        .selector
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("definition has no rows selector"))?;
    let rows = match json_get(&root, row_sel) {
        Some(serde_json::Value::Array(arr)) => arr.clone(),
        // A single object row, or nothing.
        Some(v @ serde_json::Value::Object(_)) => vec![v.clone()],
        _ => Vec::new(),
    };

    let mut releases = Vec::new();
    for row in &rows {
        if let Some(result) = extract_row_json(def, &base_ctx, row) {
            releases.push(to_release(def, cfg, &result));
        }
    }
    Ok(releases)
}

fn extract_row_json(
    def: &Definition,
    base_ctx: &Context,
    row: &serde_json::Value,
) -> Option<HashMap<String, String>> {
    let mut result: HashMap<String, String> = HashMap::new();
    for (name, field) in &def.search.fields {
        let mut ctx = base_ctx.clone();
        ctx.result = result.clone();
        let value = resolve_field_json(field, row, &ctx)?;
        result.insert(name.clone(), value);
    }
    Some(result)
}

fn resolve_field_json(field: &Field, row: &serde_json::Value, ctx: &Context) -> Option<String> {
    let raw: Option<String> = if let Some(text) = &field.text {
        Some(template::render(text, ctx))
    } else if !field.case.is_empty() {
        // JSON case: a sub-path that exists (and is truthy) selects its value.
        let mut default = None;
        let mut hit = None;
        for (path, val) in &field.case {
            if path == "*" {
                default = Some(val);
            } else if json_get(row, path).is_some_and(json_truthy) {
                hit = Some(val);
                break;
            }
        }
        hit.or(default).map(|v| template::render(v, ctx))
    } else if let Some(sel) = &field.selector {
        json_get(row, sel).map(json_scalar_string)
    } else {
        Some(json_scalar_string(row))
    };

    let value = match raw {
        Some(v) => v,
        None => match &field.default {
            Some(d) => template::render(d, ctx),
            None if field.optional => String::new(),
            None => return None,
        },
    };
    Some(filters::apply(&value, &field.filters, ctx))
}

/// Resolve a dotted JSON path (`$.a.b`, `a`, `a[0].b`) against a value.
fn json_get<'a>(value: &'a serde_json::Value, path: &str) -> Option<&'a serde_json::Value> {
    let path = path.trim().trim_start_matches('$').trim_start_matches('.');
    if path.is_empty() {
        return Some(value);
    }
    let mut cur = value;
    for seg in path.split('.') {
        // Split `key[idx]` into a key then any number of array indices.
        let (key, rest) = match seg.find('[') {
            Some(i) => (&seg[..i], &seg[i..]),
            None => (seg, ""),
        };
        if !key.is_empty() {
            cur = cur.get(key)?;
        }
        let mut r = rest;
        while let Some(close) = r.find(']') {
            let idx: usize = r[1..close].parse().ok()?;
            cur = cur.get(idx)?;
            r = &r[close + 1..];
        }
    }
    Some(cur)
}

/// A JSON scalar as a string (numbers without quotes, bools as `true`/`false`,
/// null as empty). Non-scalars stringify to their compact JSON.
fn json_scalar_string(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn json_truthy(v: &serde_json::Value) -> bool {
    match v {
        serde_json::Value::Null => false,
        serde_json::Value::Bool(b) => *b,
        serde_json::Value::String(s) => !s.is_empty(),
        serde_json::Value::Number(n) => n.as_f64() != Some(0.0),
        serde_json::Value::Array(a) => !a.is_empty(),
        serde_json::Value::Object(o) => !o.is_empty(),
    }
}

// ----- XML result parsing ---------------------------------------------------------

/// Parse an XML search response (Torznab/Newznab feeds) into releases. Uses the
/// crate's namespaced-XML DOM rather than the HTML engine.
pub fn parse_xml(def: &Definition, cfg: &IndexerConfig, body: &str) -> anyhow::Result<Vec<Release>> {
    let doc = crate::xmltree::parse(body);
    let base_ctx = base_context(def, cfg);

    let row_sel = def
        .search
        .rows
        .selector
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("definition has no rows selector"))?;
    let row_sel = template::render(row_sel, &base_ctx);

    let mut releases = Vec::new();
    for row in crate::xmltree::select_all(&doc, &row_sel) {
        if let Some(result) = extract_row_xml(def, &base_ctx, row) {
            releases.push(to_release(def, cfg, &result));
        }
    }
    Ok(releases)
}

fn extract_row_xml(
    def: &Definition,
    base_ctx: &Context,
    row: &crate::xmltree::XmlEl,
) -> Option<HashMap<String, String>> {
    let mut result: HashMap<String, String> = HashMap::new();
    for (name, field) in &def.search.fields {
        let mut ctx = base_ctx.clone();
        ctx.result = result.clone();
        let value = resolve_field_xml(field, row, &ctx)?;
        result.insert(name.clone(), value);
    }
    Some(result)
}

fn resolve_field_xml(field: &Field, row: &crate::xmltree::XmlEl, ctx: &Context) -> Option<String> {
    let raw: Option<String> = if let Some(text) = &field.text {
        Some(template::render(text, ctx))
    } else if !field.case.is_empty() {
        let mut default = None;
        let mut hit = None;
        for (sel, val) in &field.case {
            if sel == "*" {
                default = Some(val);
            } else if crate::xmltree::select_first(row, &template::render(sel, ctx)).is_some() {
                hit = Some(val);
                break;
            }
        }
        hit.or(default).map(|v| template::render(v, ctx))
    } else if let Some(sel) = &field.selector {
        let sel = template::render(sel, ctx);
        crate::xmltree::select_first(row, &sel).map(|el| match &field.attribute {
            Some(attr) => el.attr(attr).unwrap_or_default().to_string(),
            None => el.text(),
        })
    } else {
        Some(match &field.attribute {
            Some(attr) => row.attr(attr).unwrap_or_default().to_string(),
            None => row.text(),
        })
    };

    let value = match raw {
        Some(v) => v,
        None => match &field.default {
            Some(d) => template::render(d, ctx),
            None if field.optional => String::new(),
            None => return None,
        },
    };
    Some(filters::apply(&value, &field.filters, ctx))
}

// ----- result -> release ----------------------------------------------------------

/// Map an extracted field set to a [`Release`], resolving relative URLs and
/// parsing sizes/numbers.
pub fn to_release(def: &Definition, cfg: &IndexerConfig, r: &HashMap<String, String>) -> Release {
    let get = |k: &str| r.get(k).map(String::as_str).filter(|s| !s.is_empty());
    let base = &cfg.base_url;

    let title = get("title").unwrap_or_default().to_string();
    let details_url = get("details").map(|d| join_url(base, d));

    // `download` may be a magnet or a (relative) .torrent link.
    let (mut link, mut magnet) = (None, get("magnet").map(str::to_string));
    if let Some(dl) = get("download") {
        if dl.starts_with("magnet:") {
            magnet.get_or_insert_with(|| dl.to_string());
        } else {
            link = Some(join_url(base, dl));
        }
    }

    let categories = category::newznab_for_tracker_id(def, get("category").unwrap_or_default())
        .into_iter()
        .collect();

    Release {
        guid: get("guid")
            .map(str::to_string)
            .or_else(|| details_url.clone())
            .unwrap_or_else(|| title.clone()),
        title,
        link,
        magnet,
        info_hash: get("infohash").map(str::to_string),
        size_bytes: get("size").and_then(parse_size),
        seeders: get("seeders").and_then(parse_int),
        leechers: get("leechers").and_then(parse_int),
        grabs: get("grabs").and_then(parse_int),
        imdb_id: get("imdbid").map(|s| format!("tt{}", s.trim_start_matches("tt"))),
        tmdb_id: get("tmdbid").and_then(|s| s.parse().ok()),
        published_at: get("date").map(str::to_string),
        details_url,
        categories,
        download_volume_factor: get("downloadvolumefactor").and_then(|s| s.parse().ok()),
        upload_volume_factor: get("uploadvolumefactor").and_then(|s| s.parse().ok()),
    }
}

/// Parse an integer that may carry thousands separators.
fn parse_int(s: &str) -> Option<u32> {
    let cleaned: String = s.chars().filter(|c| c.is_ascii_digit()).collect();
    cleaned.parse().ok()
}

/// Parse a human size (`1.5 GB`, `700 MiB`, `1,024 KB`, a bare byte count) to
/// bytes.
pub fn parse_size(s: &str) -> Option<u64> {
    let s = s.trim().replace(',', "");
    let split = s.find(|c: char| c.is_alphabetic());
    let (num, unit) = match split {
        Some(i) => (s[..i].trim(), s[i..].trim().to_uppercase()),
        None => (s.as_str(), String::new()),
    };
    let value: f64 = num.trim().parse().ok()?;
    let mult = match unit.as_str() {
        "" | "B" => 1.0,
        "KB" | "KIB" | "K" => 1024.0,
        "MB" | "MIB" | "M" => 1024.0 * 1024.0,
        "GB" | "GIB" | "G" => 1024.0 * 1024.0 * 1024.0,
        "TB" | "TIB" | "T" => 1024.0_f64.powi(4),
        _ => return None,
    };
    Some((value * mult) as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_parsing() {
        assert_eq!(parse_size("1.5 GB"), Some(1_610_612_736));
        assert_eq!(parse_size("700 MB"), Some(734_003_200));
        assert_eq!(parse_size("1,024 KB"), Some(1_048_576));
        assert_eq!(parse_size("2048"), Some(2048));
    }

    #[test]
    fn url_joining() {
        assert_eq!(join_url("https://x.to/", "/browse?q=a"), "https://x.to/browse?q=a");
        assert_eq!(join_url("https://x.to", "dl/1"), "https://x.to/dl/1");
        assert_eq!(join_url("https://x.to/", "https://cdn/z"), "https://cdn/z");
        assert_eq!(join_url("https://x.to/", "magnet:?xt=1"), "magnet:?xt=1");
    }
}
