//! Resolved provider metadata entities (TMDB) — part of the public wire
//! contract shared with web/TV clients.
//!
//! These are pure data types (serde + ts-rs derives) with no I/O. The TMDB HTTP
//! client + cache that produce them live in [`crate::infra::metadata`].

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Resolved provider metadata for one movie or show. Serialized to clients and
/// round-tripped through the DB's `metadata` JSON column (hence `Deserialize`).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Metadata {
    // `&'static str` can't be deserialized into; it's always "tmdb" anyway, so
    // skip it on the way in and default it.
    #[serde(skip_deserializing, default = "default_provider")]
    #[ts(type = "string")]
    pub provider: &'static str,
    #[serde(rename = "tmdbId")]
    pub tmdb_id: u64,
    #[serde(rename = "imdbId", skip_serializing_if = "Option::is_none")]
    pub imdb_id: Option<String>,
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tagline: Option<String>,
    pub overview: Option<String>,
    #[serde(rename = "releaseDate", skip_serializing_if = "Option::is_none")]
    pub release_date: Option<String>,
    pub genres: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rating: Option<f32>,
    #[serde(rename = "posterUrl", skip_serializing_if = "Option::is_none")]
    pub poster_url: Option<String>,
    #[serde(rename = "backdropUrl", skip_serializing_if = "Option::is_none")]
    pub backdrop_url: Option<String>,
    /// Stylised title-treatment logo (transparent PNG), for hero/preview artwork.
    #[serde(rename = "logoUrl", skip_serializing_if = "Option::is_none")]
    pub logo_url: Option<String>,
    /// Top-billed cast (name + character), from TMDB credits. Empty when the
    /// lookup predates this field or the provider returned none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cast: Vec<CastMember>,
    /// TMDB keyword tags (e.g. "road movie", "dystopia", "heist") — a strong
    /// thematic signal for the recommendation embedding. Internal: consumed
    /// in-memory by `infra::embed::build_doc` during enrichment; deliberately not
    /// persisted to the metadata JSON nor sent to clients.
    #[serde(default, skip_serializing)]
    #[ts(skip)]
    pub keywords: Vec<String>,
    #[serde(rename = "tmdbUrl")]
    pub tmdb_url: String,
}

/// One top-billed cast member, surfaced in the detail page's "Distribution".
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CastMember {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub character: Option<String>,
    /// Profile photo. A TMDB URL when first resolved; rewritten to a locally
    /// cached WebP path (`/api/images/<hash>.webp`) by [`crate::image::localize`].
    #[serde(rename = "profileUrl", default, skip_serializing_if = "Option::is_none")]
    pub profile_url: Option<String>,
}

fn default_provider() -> &'static str {
    "tmdb"
}
