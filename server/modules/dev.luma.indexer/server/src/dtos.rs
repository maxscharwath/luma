//! Indexer wire types: the admin config views and the built-in Cardigann
//! definition catalog shapes. Pure data (serde); relocated here from the core
//! `luma-domain` crate so the module that owns them also owns their contract.

use serde::{Deserialize, Serialize};

/// One configured Torznab indexer, as listed to admins. The API key is
/// write-only (mirroring the remote-access token convention): clients only
/// learn whether one is set.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexerView {
    pub id: String,
    pub name: String,
    pub url: String,
    pub has_api_key: bool,
    pub categories: Vec<u32>,
    pub enabled: bool,
    /// Flat score bonus in the decision engine (tiebreak between indexers).
    pub priority: i32,
    /// `torznab` (external Jackett/Prowlarr) or `builtin` (native Cardigann).
    pub kind: String,
    /// The Cardigann definition id (built-in indexers only).
    pub definition_id: Option<String>,
    /// Names of the settings that currently have a value (secrets never leave
    /// the server; the edit form re-renders the schema and blanks secrets).
    pub configured_settings: Vec<String>,
    pub last_ok_at: Option<i64>,
    pub last_error: Option<String>,
    pub created_at: i64,
}

/// `GET /api/admin/indexers`.
#[derive(Debug, Clone, Serialize)]
pub struct IndexersView {
    pub indexers: Vec<IndexerView>,
}

/// `POST /api/admin/indexers` / `PUT /api/admin/indexers/:id` body. Omitted
/// fields keep their current value on update; an omitted `api_key` keeps the
/// stored secret.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveIndexerBody {
    pub name: Option<String>,
    pub url: Option<String>,
    pub api_key: Option<String>,
    pub categories: Option<Vec<u32>>,
    pub enabled: Option<bool>,
    pub priority: Option<i32>,
    /// `builtin` to create a native-Cardigann indexer (default `torznab`).
    #[serde(default)]
    pub kind: Option<String>,
    /// The Cardigann definition id (built-in create).
    #[serde(default)]
    pub definition_id: Option<String>,
    /// Per-indexer settings (credentials + toggles). Merged into the stored
    /// map on update; an omitted secret keeps its stored value.
    #[serde(default)]
    pub settings: Option<std::collections::HashMap<String, String>>,
}

// ----- built-in definition catalog ------------------------------------------------

/// One Cardigann definition in the admin's browse list.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexerDefinitionView {
    pub id: String,
    pub name: String,
    /// `public` | `private` | `semi-private`.
    pub kind: String,
    pub description: String,
    pub links: Vec<String>,
}

/// `GET /api/admin/indexers/definitions`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexerDefinitionsView {
    pub definitions: Vec<IndexerDefinitionView>,
    /// Whether the definition set has been fetched yet.
    pub synced: bool,
}

/// One configurable setting of a definition, for rendering the add form.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexerDefinitionSettingView {
    pub name: String,
    /// `text` | `password` | `checkbox` | `select` | `info`.
    pub kind: String,
    pub label: String,
    pub default: Option<String>,
    /// For `select`: ordered (value, label) pairs.
    pub options: Vec<(String, String)>,
}

/// `GET /api/admin/indexers/definitions/:id` - the schema needed to add it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexerDefinitionDetailView {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub description: String,
    pub links: Vec<String>,
    pub settings: Vec<IndexerDefinitionSettingView>,
}

/// `POST /api/admin/indexers/definitions/sync` result.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncDefinitionsResult {
    pub count: usize,
    pub version: String,
}

/// `POST /api/admin/indexers/:id/test` result (a `t=caps` round-trip).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexerTestResult {
    pub ok: bool,
    pub latency_ms: u64,
    pub server_title: Option<String>,
    /// Whether the indexer resolves TMDB ids (movie / tv search).
    pub supports_tmdb: bool,
    pub error: Option<String>,
}
