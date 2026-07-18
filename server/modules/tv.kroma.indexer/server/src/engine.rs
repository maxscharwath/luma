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
                "definition '{}' uses XPath selectors; rebuild kroma-indexer with the `xpath` feature",
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

/// Resolve one field against an XML row. `None` signals a required miss.
fn resolve_field_xml(field: &Field, row: &crate::xmltree::XmlEl, ctx: &Context) -> Option<String> {
    // 1) Raw value from text template / case switch / selector / row itself.
    let raw: Option<String> = if let Some(text) = &field.text {
        Some(template::render(text, ctx))
    } else if !field.case.is_empty() {
        eval_case_xml(field, row, ctx)
    } else if let Some(sel) = &field.selector {
        let sel = template::render(sel, ctx);
        crate::xmltree::select_first(row, &sel).map(|el| read_element_xml(field, el))
    } else {
        // No locator: read the row element itself.
        Some(read_element_xml(field, row))
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

/// Read an attribute or the flattened text of a matched XML element.
fn read_element_xml(field: &Field, el: &crate::xmltree::XmlEl) -> String {
    match &field.attribute {
        Some(attr) => el.attr(attr).unwrap_or_default().to_string(),
        None => el.text(),
    }
}

/// `case:` switch - first sub-selector that matches wins; `*` is the default.
fn eval_case_xml(field: &Field, row: &crate::xmltree::XmlEl, ctx: &Context) -> Option<String> {
    let mut default: Option<&String> = None;
    for (sel, val) in &field.case {
        if sel == "*" {
            default = Some(val);
            continue;
        }
        let rendered = template::render(sel, ctx);
        if crate::xmltree::select_first(row, &rendered).is_some() {
            return Some(template::render(val, ctx));
        }
    }
    default.map(|d| template::render(d, ctx))
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
    use std::collections::HashMap;

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

    #[test]
    fn size_parsing_units_and_edge_cases() {
        assert_eq!(parse_size("1 TB"), Some(1_099_511_627_776));
        assert_eq!(parse_size("512 MiB"), Some(536_870_912));
        // Bare single-letter unit.
        assert_eq!(parse_size("3G"), Some(3_221_225_472));
        assert_eq!(parse_size("0"), Some(0));
        // Unknown unit and a value with no leading number are rejected.
        assert_eq!(parse_size("1.5 XB"), None);
        assert_eq!(parse_size("abc"), None);
    }

    #[test]
    fn int_parsing_strips_non_digits() {
        assert_eq!(parse_int("1,234"), Some(1234));
        assert_eq!(parse_int("12 seeders"), Some(12));
        assert_eq!(parse_int("none"), None);
        assert_eq!(parse_int(""), None);
    }

    // ----- fixtures ---------------------------------------------------------------

    fn build_def(yaml: &str) -> Definition {
        crate::definition::parse(yaml.as_bytes()).expect("definition fixture must parse")
    }

    fn cfg(base: &str) -> IndexerConfig {
        IndexerConfig { base_url: base.to_string(), settings: std::collections::HashMap::new() }
    }

    // ----- to_release -------------------------------------------------------------

    fn cat_def() -> Definition {
        build_def(
            r#"
id: t
name: T
caps:
  categorymappings:
    - {id: "100", cat: "Movies/HD"}
    - {id: "200", cat: "TV/HD"}
search:
  rows:
    selector: "tr"
"#,
        )
    }

    #[test]
    fn to_release_maps_every_field() {
        let def = cat_def();
        let cfg = cfg("https://site.to/");
        let mut r: HashMap<String, String> = HashMap::new();
        r.insert("title".into(), "Cool.Movie.2020.1080p".into());
        r.insert("details".into(), "/torrent/42".into());
        r.insert("download".into(), "/dl/42.torrent".into());
        r.insert("size".into(), "1.5 GB".into());
        r.insert("seeders".into(), "1,024".into());
        r.insert("leechers".into(), "12".into());
        r.insert("grabs".into(), "5".into());
        r.insert("category".into(), "100".into());
        r.insert("imdbid".into(), "0133093".into());
        r.insert("tmdbid".into(), "603".into());
        r.insert("date".into(), "2020-01-02".into());
        r.insert("infohash".into(), "DEADBEEF".into());
        r.insert("downloadvolumefactor".into(), "0.5".into());
        r.insert("uploadvolumefactor".into(), "1".into());

        let rel = to_release(&def, &cfg, &r);
        assert_eq!(rel.title, "Cool.Movie.2020.1080p");
        assert_eq!(rel.details_url.as_deref(), Some("https://site.to/torrent/42"));
        assert_eq!(rel.link.as_deref(), Some("https://site.to/dl/42.torrent"));
        assert_eq!(rel.magnet, None);
        assert_eq!(rel.size_bytes, Some(1_610_612_736));
        assert_eq!(rel.seeders, Some(1024));
        assert_eq!(rel.leechers, Some(12));
        assert_eq!(rel.grabs, Some(5));
        assert_eq!(rel.categories, vec![2040]);
        assert_eq!(rel.imdb_id.as_deref(), Some("tt0133093"));
        assert_eq!(rel.tmdb_id, Some(603));
        assert_eq!(rel.published_at.as_deref(), Some("2020-01-02"));
        assert_eq!(rel.info_hash.as_deref(), Some("DEADBEEF"));
        assert_eq!(rel.download_volume_factor, Some(0.5));
        assert_eq!(rel.upload_volume_factor, Some(1.0));
        // No explicit guid: falls back to the details URL.
        assert_eq!(rel.guid, "https://site.to/torrent/42");
    }

    #[test]
    fn to_release_download_magnet_and_guid_fallbacks() {
        let def = cat_def();
        let cfg = cfg("https://site.to/");

        // A magnet in `download` lands in `magnet`, never `link`.
        let mut r: HashMap<String, String> = HashMap::new();
        r.insert("title".into(), "Only Title".into());
        r.insert("download".into(), "magnet:?xt=urn:btih:ABC".into());
        let rel = to_release(&def, &cfg, &r);
        assert_eq!(rel.magnet.as_deref(), Some("magnet:?xt=urn:btih:ABC"));
        assert_eq!(rel.link, None);
        // No guid + no details: guid falls back to the title.
        assert_eq!(rel.guid, "Only Title");
        // Unmapped/empty category -> no categories.
        assert!(rel.categories.is_empty());

        // Explicit guid wins; an already-tt imdbid is not double-prefixed; an
        // explicit `magnet` key is kept when `download` is also a magnet.
        let mut r2: HashMap<String, String> = HashMap::new();
        r2.insert("title".into(), "T".into());
        r2.insert("guid".into(), "the-guid".into());
        r2.insert("magnet".into(), "magnet:?xt=urn:btih:KEEP".into());
        r2.insert("download".into(), "magnet:?xt=urn:btih:OTHER".into());
        r2.insert("imdbid".into(), "tt42".into());
        let rel2 = to_release(&def, &cfg, &r2);
        assert_eq!(rel2.guid, "the-guid");
        assert_eq!(rel2.magnet.as_deref(), Some("magnet:?xt=urn:btih:KEEP"));
        assert_eq!(rel2.imdb_id.as_deref(), Some("tt42"));
    }

    #[test]
    fn to_release_download_absolute_url_kept() {
        let def = cat_def();
        let cfg = cfg("https://site.to/");
        let mut r: HashMap<String, String> = HashMap::new();
        r.insert("title".into(), "T".into());
        r.insert("download".into(), "https://cdn.example/x.torrent".into());
        let rel = to_release(&def, &cfg, &r);
        assert_eq!(rel.link.as_deref(), Some("https://cdn.example/x.torrent"));
    }

    // ----- parse_html -------------------------------------------------------------

    #[test]
    fn parse_html_extracts_rows() {
        let def = build_def(
            r#"
id: t
name: T
caps:
  categorymappings:
    - {id: "1", cat: "Movies/HD"}
search:
  rows:
    selector: "table.results tr.torrent"
  fields:
    title:
      selector: "td.name a"
    details:
      selector: "td.name a"
      attribute: href
    download:
      selector: "td.name a"
      attribute: href
    size:
      selector: "td.size"
    seeders:
      selector: "td.seeders"
    category:
      text: "1"
"#,
        );
        let cfg = cfg("https://site.to/");
        let body = r#"
          <table class="results">
            <tr class="torrent">
              <td class="name"><a href="/t/1">Cool Movie 2020</a></td>
              <td class="size">1.5 GB</td>
              <td class="seeders">10</td>
            </tr>
            <tr class="torrent">
              <td class="name"><a href="/t/2">Other Show</a></td>
              <td class="size">700 MB</td>
              <td class="seeders">3</td>
            </tr>
          </table>
        "#;
        let rels = parse_html(&def, &cfg, body).unwrap();
        assert_eq!(rels.len(), 2);
        assert_eq!(rels[0].title, "Cool Movie 2020");
        assert_eq!(rels[0].details_url.as_deref(), Some("https://site.to/t/1"));
        assert_eq!(rels[0].link.as_deref(), Some("https://site.to/t/1"));
        assert_eq!(rels[0].size_bytes, Some(1_610_612_736));
        assert_eq!(rels[0].seeders, Some(10));
        assert_eq!(rels[0].categories, vec![2040]);
        assert_eq!(rels[1].seeders, Some(3));
    }

    #[test]
    fn parse_html_skips_required_miss_and_honors_optional_default() {
        let def = build_def(
            r#"
id: t
name: T
caps: {}
search:
  rows:
    selector: "tr.r"
  fields:
    title:
      selector: "td.title"
    size:
      selector: "td.size"
      default: "2 GB"
    seeders:
      selector: "td.seeders"
      optional: true
    grabs:
      selector: "td.grabs"
"#,
        );
        let cfg = cfg("https://x/");
        let body = r#"
          <table>
            <tr class="r"><td class="title">Good</td><td class="grabs">7</td></tr>
            <tr class="r"><td class="title">NoGrabs</td></tr>
          </table>
        "#;
        let rels = parse_html(&def, &cfg, body).unwrap();
        // Second row misses the required (non-optional, no-default) `grabs`: dropped.
        assert_eq!(rels.len(), 1);
        assert_eq!(rels[0].title, "Good");
        // `size` fell back to its default; `seeders` was optional -> empty -> None.
        assert_eq!(rels[0].size_bytes, Some(2 * 1024 * 1024 * 1024));
        assert_eq!(rels[0].seeders, None);
        assert_eq!(rels[0].grabs, Some(7));
    }

    #[test]
    fn parse_html_case_switch_selects_category() {
        let def = build_def(
            r#"
id: t
name: T
caps:
  categorymappings:
    - {id: "1", cat: "Movies/HD"}
search:
  rows:
    selector: "tr.r"
  fields:
    title:
      selector: "td.title"
    category:
      case:
        "td.hd": "1"
        "*": "9999"
"#,
        );
        let cfg = cfg("https://x/");
        let body = r#"
          <table>
            <tr class="r"><td class="title">A</td><td class="hd">HD</td></tr>
            <tr class="r"><td class="title">B</td></tr>
          </table>
        "#;
        let rels = parse_html(&def, &cfg, body).unwrap();
        // Row A hits the `td.hd` case -> id 1 -> Movies/HD (2040).
        assert_eq!(rels[0].categories, vec![2040]);
        // Row B hits `*` -> id 9999, unmapped -> no categories.
        assert!(rels[1].categories.is_empty());
    }

    #[test]
    fn parse_html_without_rows_selector_errors() {
        let def = build_def(
            r#"
id: t
name: T
caps: {}
search:
  rows: {}
"#,
        );
        let err = parse_html(&def, &cfg("https://x/"), "<html></html>").unwrap_err();
        assert!(err.to_string().contains("no rows selector"), "{err}");
    }

    // ----- parse_json -------------------------------------------------------------

    #[test]
    fn parse_json_dotted_paths_and_scalars() {
        let def = build_def(
            r#"
id: t
name: T
caps: {}
search:
  rows:
    selector: "$.data.torrents"
  fields:
    title:
      selector: "name"
    size:
      selector: "size"
    seeders:
      selector: "seeders"
"#,
        );
        let cfg = cfg("https://x/");
        let body = r#"{"data":{"torrents":[
          {"name":"Rel One 1080p","size":123456,"seeders":42},
          {"name":"Rel Two 720p","size":999,"seeders":1}
        ]}}"#;
        let rels = parse_json(&def, &cfg, body).unwrap();
        assert_eq!(rels.len(), 2);
        assert_eq!(rels[0].title, "Rel One 1080p");
        assert_eq!(rels[0].size_bytes, Some(123456));
        assert_eq!(rels[0].seeders, Some(42));
        assert_eq!(rels[1].seeders, Some(1));
    }

    #[test]
    fn parse_json_single_object_row_and_case() {
        // A rows selector resolving to a single object yields one row.
        let obj_def = build_def(
            r#"
id: t
name: T
caps: {}
search:
  rows:
    selector: "result"
  fields:
    title:
      selector: "name"
"#,
        );
        let rels = parse_json(&obj_def, &cfg("https://x/"), r#"{"result":{"name":"Solo 1080p"}}"#).unwrap();
        assert_eq!(rels.len(), 1);
        assert_eq!(rels[0].title, "Solo 1080p");

        // JSON `case`: a truthy sub-path hits, else the `*` default.
        let case_def = build_def(
            r#"
id: t
name: T
caps: {}
search:
  rows:
    selector: "items"
  fields:
    title:
      selector: "name"
    seeders:
      case:
        has: "9"
        "*": "0"
"#,
        );
        let body = r#"{"items":[{"name":"A 1080p","has":true},{"name":"B 720p"}]}"#;
        let rels = parse_json(&case_def, &cfg("https://x/"), body).unwrap();
        assert_eq!(rels[0].seeders, Some(9));
        assert_eq!(rels[1].seeders, Some(0));
    }

    #[test]
    fn parse_json_missing_rows_returns_empty() {
        let def = build_def(
            r#"
id: t
name: T
caps: {}
search:
  rows:
    selector: "$.nope"
  fields:
    title:
      selector: "name"
"#,
        );
        let rels = parse_json(&def, &cfg("https://x/"), r#"{"data":1}"#).unwrap();
        assert!(rels.is_empty());
    }

    // ----- json helpers -----------------------------------------------------------

    #[test]
    fn json_get_resolves_paths() {
        let v: serde_json::Value =
            serde_json::from_str(r#"{"a":{"b":[10,20,{"c":"deep"}]}}"#).unwrap();
        assert_eq!(json_get(&v, "$.a.b[0]").unwrap().as_i64(), Some(10));
        assert_eq!(json_get(&v, "a.b[1]").unwrap().as_i64(), Some(20));
        assert_eq!(json_get(&v, "a.b[2].c").unwrap().as_str(), Some("deep"));
        assert!(json_get(&v, "a.x").is_none());
        assert!(json_get(&v, "a.b[9]").is_none());
        // Empty / bare-$ path resolves to the whole value.
        assert!(std::ptr::eq(json_get(&v, "").unwrap(), &v));
        assert!(std::ptr::eq(json_get(&v, "$").unwrap(), &v));
    }

    #[test]
    fn json_scalar_string_and_truthy() {
        use serde_json::json;
        assert_eq!(json_scalar_string(&json!("hi")), "hi");
        assert_eq!(json_scalar_string(&json!(42)), "42");
        assert_eq!(json_scalar_string(&json!(true)), "true");
        assert_eq!(json_scalar_string(&serde_json::Value::Null), "");
        assert_eq!(json_scalar_string(&json!([1, 2])), "[1,2]");

        assert!(!json_truthy(&serde_json::Value::Null));
        assert!(json_truthy(&json!(true)) && !json_truthy(&json!(false)));
        assert!(json_truthy(&json!("x")) && !json_truthy(&json!("")));
        assert!(json_truthy(&json!(5)) && !json_truthy(&json!(0)));
        assert!(json_truthy(&json!([1])) && !json_truthy(&json!([])));
        assert!(json_truthy(&json!({"a":1})) && !json_truthy(&json!({})));
    }

    // ----- parse_xml --------------------------------------------------------------

    const XML_FEED: &str = r#"<?xml version="1.0"?>
      <rss xmlns:torznab="http://torznab.com/">
      <channel>
        <item>
          <title>Obsession 2026 1080p</title>
          <guid>abc123</guid>
          <category>2000</category>
          <torznab:attr name="seeders" value="305"/>
          <torznab:attr name="size" value="2314321864"/>
        </item>
        <item>
          <title>Other 720p</title>
          <guid>def456</guid>
          <category>2000</category>
          <torznab:attr name="seeders" value="7"/>
          <torznab:attr name="size" value="1000"/>
        </item>
      </channel>
      </rss>"#;

    #[test]
    fn parse_xml_extracts_items_and_attrs() {
        let def = build_def(
            r#"
id: t
name: T
caps:
  categorymappings:
    - {id: "2000", cat: "Movies"}
search:
  rows:
    selector: "item"
  fields:
    title:
      selector: "title"
    guid:
      selector: "guid"
    category:
      selector: "category"
    seeders:
      selector: "[name=seeders]"
      attribute: value
    size:
      selector: "[name=size]"
      attribute: value
"#,
        );
        let rels = parse_xml(&def, &cfg("https://x/"), XML_FEED).unwrap();
        assert_eq!(rels.len(), 2);
        assert_eq!(rels[0].title, "Obsession 2026 1080p");
        assert_eq!(rels[0].guid, "abc123");
        assert_eq!(rels[0].seeders, Some(305));
        assert_eq!(rels[0].size_bytes, Some(2_314_321_864));
        assert_eq!(rels[0].categories, vec![2000]);
        assert_eq!(rels[1].seeders, Some(7));
    }

    #[test]
    fn parse_xml_case_switch() {
        let def = build_def(
            r#"
id: t
name: T
caps: {}
search:
  rows:
    selector: "item"
  fields:
    title:
      selector: "title"
    seeders:
      case:
        "[name=seeders]": "100"
        "*": "0"
"#,
        );
        let rels = parse_xml(&def, &cfg("https://x/"), XML_FEED).unwrap();
        // Both items carry a seeders attr -> the case hit fires (constant 100).
        assert_eq!(rels[0].seeders, Some(100));
        assert_eq!(rels[1].seeders, Some(100));
    }

    // ----- build_requests / query_attributes --------------------------------------

    #[test]
    fn build_requests_get_movie_with_imdb_and_categories() {
        let def = build_def(
            r#"
id: t
name: T
caps:
  categorymappings:
    - {id: "42", cat: "Movies/HD"}
  modes:
    search: [q]
    movie-search: [q, imdbid, tmdbid]
search:
  paths:
    - path: "/search?q={{ .Keywords }}"
      inputs:
        cat: "{{ join .Categories \",\" }}"
  inputs:
    imdb: "{{ .Query.IMDBID }}"
  rows:
    selector: "tr"
"#,
        );
        let cfg = cfg("https://site.to/");
        let q = Query::Movie {
            tmdb_id: None,
            imdb_id: Some("tt0133093".into()),
            title: "The Matrix".into(),
            year: Some(1999),
        };
        let reqs = build_requests(&def, &cfg, &q, &[2000]);
        assert_eq!(reqs.len(), 1);
        assert_eq!(reqs[0].url, "https://site.to/search?q=The Matrix 1999");
        assert_eq!(reqs[0].method, "get");
        assert_eq!(reqs[0].response_kind, "html");
        // query_attributes rendered `.Query.IMDBID`; categories mapped to id 42.
        assert!(reqs[0].inputs.contains(&("imdb".to_string(), "tt0133093".to_string())));
        assert!(reqs[0].inputs.contains(&("cat".to_string(), "42".to_string())));
    }

    #[test]
    fn build_requests_post_json_path() {
        let def = build_def(
            r#"
id: t
name: T
caps: {}
search:
  paths:
    - path: /api
      method: POST
      response:
        type: json
  rows:
    selector: "$.rows"
"#,
        );
        let reqs = build_requests(&def, &cfg("https://api.x/"), &Query::Text { query: "hi".into() }, &[]);
        assert_eq!(reqs.len(), 1);
        assert_eq!(reqs[0].url, "https://api.x/api");
        assert_eq!(reqs[0].method, "post");
        assert_eq!(reqs[0].response_kind, "json");
    }

    // ----- preprocess / uses_xpath ------------------------------------------------

    #[test]
    fn preprocess_noop_and_filtered() {
        let plain = build_def(
            r#"
id: t
name: T
caps: {}
search:
  rows:
    selector: "tr"
"#,
        );
        assert_eq!(preprocess(&plain, &cfg("https://x/"), "body"), "body");

        let filtered = build_def(
            r#"
id: t
name: T
caps: {}
search:
  preprocessingfilters:
    - name: re_replace
      args: ["^junk", ""]
  rows:
    selector: "tr"
"#,
        );
        assert_eq!(preprocess(&filtered, &cfg("https://x/"), "junkREST"), "REST");
    }

    #[test]
    fn uses_xpath_detection() {
        let css = build_def(
            r#"
id: t
name: T
caps: {}
search:
  rows:
    selector: "tr.torrent"
  fields:
    title:
      selector: "a"
"#,
        );
        assert!(!uses_xpath(&css));

        let xpath = build_def(
            r#"
id: t
name: T
caps: {}
search:
  rows:
    selector: "//tr[@class='torrent']"
  fields:
    title:
      selector: "a"
"#,
        );
        assert!(uses_xpath(&xpath));
    }
}
