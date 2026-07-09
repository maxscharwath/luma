//! The Cardigann YAML definition schema, as spoken by Jackett and Prowlarr
//! (`definitions/vXX/*.yml`). This is a faithful-enough model of the format to
//! drive the engine: every field the search/login/download pipelines read is
//! typed here; the long tail of purely-cosmetic keys (descriptions, changelog
//! comments) is dropped by serde's default of ignoring unknown fields.
//!
//! Deserialization is deliberately lenient — real definitions mix scalars and
//! sequences freely (a `links:` is a list, an `args:` is a scalar *or* a list,
//! a category `id:` is an int *or* a string) — so a handful of custom
//! deserializers normalize those into stable Rust shapes.

use indexmap::IndexMap;
use serde::Deserialize;

/// One parsed Cardigann definition. Loaded from YAML; cheap to clone-by-ref via
/// the engine holding it behind an `Arc`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Definition {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub language: String,
    /// `public` | `private` | `semi-private`.
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(default = "default_encoding")]
    pub encoding: String,
    /// Candidate base URLs, best first. The engine picks the first reachable.
    #[serde(default)]
    pub links: Vec<String>,
    #[serde(default)]
    pub legacylinks: Vec<String>,
    /// Minimum delay between requests to this indexer, in seconds (politeness /
    /// rate-limit avoidance). Fractional in the wild (`4.1`).
    #[serde(default)]
    pub request_delay: Option<f64>,
    pub caps: Caps,
    #[serde(default)]
    pub settings: Vec<Setting>,
    #[serde(default)]
    pub login: Option<Login>,
    pub search: Search,
    #[serde(default)]
    pub download: Option<Download>,
}

fn default_encoding() -> String {
    "UTF-8".to_string()
}

// ----- caps -----------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct Caps {
    #[serde(default)]
    pub categorymappings: Vec<CategoryMapping>,
    /// Newznab-style category tree (`categories:` map of id -> name), used by a
    /// few definitions instead of `categorymappings`.
    #[serde(default)]
    pub categories: IndexMap<String, String>,
    /// mode name (`search`, `tv-search`, `movie-search`, ...) -> the query
    /// parameters that mode accepts (`q`, `season`, `ep`, `imdbid`, `tmdbid`...).
    #[serde(default)]
    pub modes: IndexMap<String, Vec<String>>,
    #[serde(default)]
    pub allowrawsearch: bool,
}

/// One tracker category mapped onto a Torznab/Newznab bucket.
#[derive(Debug, Clone, Deserialize)]
pub struct CategoryMapping {
    /// The tracker's own category id (int or string in YAML; kept as a string).
    #[serde(deserialize_with = "de_scalar_string")]
    pub id: String,
    /// The Newznab category name, e.g. `Movies/HD`, `TV/Anime`.
    pub cat: String,
    #[serde(default)]
    pub desc: Option<String>,
    #[serde(default)]
    pub default: bool,
}

// ----- settings -------------------------------------------------------------------

/// One admin-facing configuration input (username, password, a freeleech
/// toggle, a sort dropdown...). We keep the whole thing so the admin UI can
/// render it and so `.Config.<name>` resolves to the configured-or-default
/// value.
#[derive(Debug, Clone, Deserialize)]
pub struct Setting {
    pub name: String,
    /// `text` | `password` | `checkbox` | `select` | `info` | `info_*`.
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(default)]
    pub label: Option<String>,
    /// Default value; scalar (string / bool / number) normalized to a string.
    #[serde(default, deserialize_with = "de_opt_scalar_string")]
    pub default: Option<String>,
    /// For `select`: option value -> display label. The *key* is what
    /// `.Config.<name>` yields.
    #[serde(default)]
    pub options: IndexMap<String, String>,
}

// ----- login ----------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct Login {
    /// `form` | `post` | `get` | `cookie` | `oneurl` | `getpost`.
    #[serde(default)]
    pub method: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub submitpath: Option<String>,
    /// CSS selector of the `<form>` to submit (method `form`): its action +
    /// hidden inputs are read from the page.
    #[serde(default)]
    pub form: Option<String>,
    /// Field name -> templated value.
    #[serde(default)]
    pub inputs: IndexMap<String, ScalarString>,
    /// Field name -> a value scraped from the login page before submitting
    /// (CSRF tokens etc).
    #[serde(default)]
    pub selectorinputs: IndexMap<String, Selector>,
    /// Cookie strings to set directly (method `cookie`).
    #[serde(default, deserialize_with = "de_string_or_seq")]
    pub cookies: Vec<String>,
    /// Error conditions to detect a failed login.
    #[serde(default)]
    pub error: Vec<LoginError>,
    #[serde(default)]
    pub test: Option<LoginTest>,
    #[serde(default)]
    pub captcha: Option<Captcha>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LoginError {
    #[serde(default)]
    pub selector: Option<String>,
    #[serde(default)]
    pub message: Option<Message>,
}

/// A message block: either a scraped selector or a templated literal.
#[derive(Debug, Clone, Deserialize)]
pub struct Message {
    #[serde(default)]
    pub selector: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LoginTest {
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub selector: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Captcha {
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(default)]
    pub selector: Option<String>,
    #[serde(default)]
    pub input: Option<String>,
}

// ----- search ---------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct Search {
    /// One or more request paths (relative to the base link); templated. Each
    /// may declare its own response type / inputs.
    #[serde(default, deserialize_with = "de_search_paths")]
    pub paths: Vec<SearchPath>,
    /// Shared query parameters / form fields, templated, sent on every path.
    #[serde(default)]
    pub inputs: IndexMap<String, ScalarString>,
    #[serde(default)]
    pub headers: IndexMap<String, ScalarString>,
    /// Filters applied to the raw keyword string before it is templated in.
    #[serde(default)]
    pub keywordsfilters: Vec<Filter>,
    /// Filters applied to the whole response body before parsing.
    #[serde(default)]
    pub preprocessingfilters: Vec<Filter>,
    pub rows: Rows,
    /// Field name -> extraction rule. Order matters: a field's `text` template
    /// can reference an earlier field via `.Result.<name>`.
    #[serde(default)]
    pub fields: IndexMap<String, Field>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SearchPath {
    pub path: String,
    /// HTTP method (`get` default, `post`).
    #[serde(default)]
    pub method: Option<String>,
    /// Per-path response override.
    #[serde(default)]
    pub response: Option<ResponseSpec>,
    #[serde(default)]
    pub inputs: IndexMap<String, ScalarString>,
    #[serde(default)]
    pub followredirect: bool,
    /// Restrict this path to certain requested categories (by mapped name).
    #[serde(default, deserialize_with = "de_string_or_seq")]
    pub categories: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResponseSpec {
    /// `html` (default) | `json` | `xml`.
    #[serde(rename = "type", default)]
    pub kind: String,
    /// For JSON: a jsonpath-ish base under which rows live (rarely used; rows
    /// usually carry their own selector).
    #[serde(default)]
    pub attribute: Option<String>,
    #[serde(default)]
    pub nocookies: bool,
}

/// How to locate the per-release rows in a response, and the paging hints.
#[derive(Debug, Clone, Deserialize)]
pub struct Rows {
    /// CSS/XPath selector (HTML) or a key/jsonpath (JSON) selecting each row.
    #[serde(default)]
    pub selector: Option<String>,
    /// Rows to merge upward into the previous one (multi-line row layouts).
    #[serde(default)]
    pub after: i64,
    /// A count/paging hint block.
    #[serde(default)]
    pub count: Option<CountBlock>,
    /// Date-carrying header rows interleaved between result rows.
    #[serde(default)]
    pub dateheaders: Option<Selector>,
    #[serde(default)]
    pub filters: Vec<Filter>,
    /// When true, an absent row attribute yields "no results" rather than error.
    #[serde(default, rename = "missingAttributeEqualsNoResults")]
    pub missing_attribute_equals_no_results: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CountBlock {
    #[serde(default)]
    pub selector: Option<String>,
}

/// One extracted field. The engine resolves, in order: a `text` template, or a
/// `selector` (+ `attribute`/`remove`/`case`), then runs `filters`, then falls
/// back to `default`. `optional` downgrades an extraction miss to empty.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Field {
    #[serde(default)]
    pub selector: Option<String>,
    /// A templated literal (may reference `.Result.*`, `.Config.*`).
    #[serde(default)]
    pub text: Option<String>,
    /// Read this attribute instead of the element text.
    #[serde(default)]
    pub attribute: Option<String>,
    /// CSS selector of descendant nodes to strip before reading text.
    #[serde(default)]
    pub remove: Option<String>,
    /// switch: sub-selector -> literal value (first match wins; `*` = default).
    #[serde(default)]
    pub case: IndexMap<String, String>,
    #[serde(default)]
    pub filters: Vec<Filter>,
    #[serde(default)]
    pub optional: bool,
    #[serde(default, deserialize_with = "de_opt_scalar_string")]
    pub default: Option<String>,
}

/// A bare selector block (used by `selectorinputs`, `dateheaders`).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Selector {
    #[serde(default)]
    pub selector: Option<String>,
    #[serde(default)]
    pub attribute: Option<String>,
    #[serde(default)]
    pub remove: Option<String>,
    #[serde(default)]
    pub case: IndexMap<String, String>,
    #[serde(default)]
    pub filters: Vec<Filter>,
    #[serde(default)]
    pub optional: bool,
}

// ----- download -------------------------------------------------------------------

/// How to turn a details/download URL into an actual `.torrent` link, magnet,
/// or infohash. Definitions either give a direct link in the `download` field
/// or need a follow-up fetch of the details page with selectors.
#[derive(Debug, Clone, Deserialize)]
pub struct Download {
    /// Ordered candidate selectors (first non-empty wins).
    #[serde(default, deserialize_with = "de_download_selectors")]
    pub selectors: Vec<Selector>,
    /// Direct method: `get` (default) | `post`.
    #[serde(default)]
    pub method: Option<String>,
    /// Extract the infohash directly (magnet-only trackers).
    #[serde(default)]
    pub infohash: Option<InfoHash>,
    /// Templated inputs for a POST download.
    #[serde(default)]
    pub inputs: IndexMap<String, ScalarString>,
    #[serde(default)]
    pub before: Option<BeforeRequest>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct InfoHash {
    #[serde(default)]
    pub hash: Option<Selector>,
    #[serde(default)]
    pub title: Option<Selector>,
    /// Some definitions inline the selector on the infohash block itself.
    #[serde(default)]
    pub selector: Option<String>,
    #[serde(default)]
    pub attribute: Option<String>,
}

/// A priming request performed before the download (e.g. hit a `/download.php`
/// that sets a token cookie).
#[derive(Debug, Clone, Deserialize)]
pub struct BeforeRequest {
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub method: Option<String>,
    #[serde(default)]
    pub inputs: IndexMap<String, ScalarString>,
}

// ----- filters --------------------------------------------------------------------

/// One field/keyword filter: a name plus 0..n string arguments. Args in YAML
/// are a scalar (`args: foo`), a list (`args: ["a", "b"]`), or absent.
#[derive(Debug, Clone, Deserialize)]
pub struct Filter {
    pub name: String,
    #[serde(default, deserialize_with = "de_filter_args")]
    pub args: Vec<String>,
}

// ----- flexible scalar ------------------------------------------------------------

/// A YAML scalar (string / bool / int / float) that we always want as a string
/// (definition values are consumed as text after templating).
#[derive(Debug, Clone, Default)]
pub struct ScalarString(pub String);

impl<'de> Deserialize<'de> for ScalarString {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(ScalarString(scalar_to_string(serde_yaml::Value::deserialize(d)?)))
    }
}

impl std::ops::Deref for ScalarString {
    type Target = str;
    fn deref(&self) -> &str {
        &self.0
    }
}

// ----- custom deserializers -------------------------------------------------------

fn scalar_to_string(v: serde_yaml::Value) -> String {
    match v {
        serde_yaml::Value::String(s) => s,
        serde_yaml::Value::Bool(b) => b.to_string(),
        serde_yaml::Value::Number(n) => n.to_string(),
        serde_yaml::Value::Null => String::new(),
        // Sequences / mappings never appear where a scalar is expected; stringify
        // defensively rather than fail the whole definition.
        other => serde_yaml::to_string(&other).unwrap_or_default().trim().to_string(),
    }
}

fn de_scalar_string<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    Ok(scalar_to_string(serde_yaml::Value::deserialize(d)?))
}

fn de_opt_scalar_string<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Option<String>, D::Error> {
    let v = serde_yaml::Value::deserialize(d)?;
    Ok(match v {
        serde_yaml::Value::Null => None,
        other => Some(scalar_to_string(other)),
    })
}

/// A field that may be a single string or a list of strings (`links`,
/// `cookies`, `categories`).
fn de_string_or_seq<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    let v = serde_yaml::Value::deserialize(d)?;
    Ok(match v {
        serde_yaml::Value::Null => Vec::new(),
        serde_yaml::Value::Sequence(seq) => seq.into_iter().map(scalar_to_string).collect(),
        other => vec![scalar_to_string(other)],
    })
}

/// Filter `args`: scalar, list, or absent -> `Vec<String>`.
fn de_filter_args<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    de_string_or_seq(d)
}

/// `search.paths` entries are usually maps (`{path, response, ...}`) but a few
/// legacy definitions list bare path strings.
fn de_search_paths<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<SearchPath>, D::Error> {
    use serde::de::Error;
    let seq = Vec::<serde_yaml::Value>::deserialize(d)?;
    seq.into_iter()
        .map(|v| match v {
            serde_yaml::Value::String(path) => Ok(SearchPath {
                path,
                method: None,
                response: None,
                inputs: IndexMap::new(),
                followredirect: false,
                categories: Vec::new(),
            }),
            other => serde_yaml::from_value(other).map_err(D::Error::custom),
        })
        .collect()
}

/// `download.selectors` entries are selector maps, but a shorthand allows a bare
/// selector string.
fn de_download_selectors<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Vec<Selector>, D::Error> {
    use serde::de::Error;
    let seq = Vec::<serde_yaml::Value>::deserialize(d)?;
    seq.into_iter()
        .map(|v| match v {
            serde_yaml::Value::String(s) => {
                Ok(Selector { selector: Some(s), ..Selector::default() })
            }
            other => serde_yaml::from_value(other).map_err(D::Error::custom),
        })
        .collect()
}

/// Parse a definition from YAML bytes.
pub fn parse(bytes: &[u8]) -> anyhow::Result<Definition> {
    Ok(serde_yaml::from_slice(bytes)?)
}
