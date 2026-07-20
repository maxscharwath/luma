//! Resolved provider metadata entities (TMDB) part of the public wire
//! contract shared with web/TV clients.
//!
//! These are pure data types (serde + ts-rs derives) with no I/O. The TMDB HTTP
//! client + cache that produce them live in `crate::infra::metadata`.

use serde::{Deserialize, Serialize};

/// Resolved provider metadata for one movie or show. Serialized to clients and
/// round-tripped through the DB's `metadata` JSON column (hence `Deserialize`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metadata {
    // `&'static str` can't be deserialized into; it's always "tmdb" anyway, so
    // skip it on the way in and default it.
    #[serde(skip_deserializing, default = "default_provider")]
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
    /// Plex-style theme song. A locally-cached MP3 path (`/api/themes/<tvdb>.mp3`)
    /// that the detail page loops under the hero. Only resolved for TV shows
    /// (sourced from the community tvthemes archive, keyed by [`Self::tvdb_id`]);
    /// `None` for movies and shows with no archived theme.
    #[serde(rename = "themeUrl", default, skip_serializing_if = "Option::is_none")]
    pub theme_url: Option<String>,
    /// Top-billed cast (name + character), from TMDB credits. Empty when the
    /// lookup predates this field or the provider returned none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cast: Vec<CastMember>,
    /// Key crew (directors first, then writers; TV creators folded in), from TMDB
    /// credits. Powers the detail "Réalisation" line and deterministic director
    /// collections. Empty when the lookup predates this field.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub crew: Vec<CrewMember>,
    /// TMDB keyword tags (e.g. "road movie", "dystopia", "heist") a strong
    /// thematic signal for the recommendation embedding. Internal: consumed
    /// in-memory by [`build_doc`] during enrichment; deliberately not
    /// persisted to the metadata JSON nor sent to clients.
    #[serde(default, skip_serializing)]
    pub keywords: Vec<String>,
    /// TheTVDB series id, from TMDB's `external_ids`. Internal: used during
    /// enrichment to look up the theme song (the tvthemes archive is TVDB-keyed),
    /// then dropped not persisted to the metadata JSON nor sent to clients.
    #[serde(default, skip_serializing)]
    pub tvdb_id: Option<u64>,
    #[serde(rename = "tmdbUrl")]
    pub tmdb_url: String,
}

/// One top-billed cast member, surfaced in the detail page's "Distribution".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CastMember {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub character: Option<String>,
    /// Profile photo. A TMDB URL when first resolved; rewritten to a locally
    /// cached WebP path (`/api/images/<hash>.webp`) by `crate::image::localize`.
    #[serde(rename = "profileUrl", default, skip_serializing_if = "Option::is_none")]
    pub profile_url: Option<String>,
}

/// One key crew member (director, writer, creator), surfaced on the detail page
/// and used to group director collections. `job` is the TMDB job title
/// (`"Director"`, `"Writer"`, `"Creator"`, …).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrewMember {
    pub name: String,
    pub job: String,
    /// Profile photo (optional; not yet localized directors render as text).
    #[serde(rename = "profileUrl", default, skip_serializing_if = "Option::is_none")]
    pub profile_url: Option<String>,
}

fn default_provider() -> &'static str {
    "tmdb"
}

/// Assemble the text we embed for one title. Genres are repeated because they're
/// the strongest "feels like" signal: in the lexical backend repetition up-weights
/// them; in MiniLM it nudges the sentence meaning. Order is most- to
/// least-discriminating so a truncating tokenizer keeps the important parts.
pub fn build_doc(title: &str, year: Option<u32>, meta: &Metadata) -> String {
    let mut parts: Vec<String> = Vec::with_capacity(8);
    parts.push(title.to_string());
    if let Some(y) = year {
        parts.push(y.to_string());
    }
    if !meta.genres.is_empty() {
        let genres = meta.genres.join(" ");
        parts.push(genres.clone()); // repeat: genres dominate similarity
        parts.push(genres);
    }
    // Keyword tags ("road movie", "dystopia", "heist") are the strongest signal
    // for themed rows they carry the vibe the overview rarely states outright.
    if !meta.keywords.is_empty() {
        parts.push(meta.keywords.join(" "));
    }
    for c in meta.cast.iter().take(6) {
        parts.push(c.name.clone());
    }
    if let Some(tagline) = &meta.tagline {
        parts.push(tagline.clone());
    }
    if let Some(overview) = &meta.overview {
        parts.push(overview.clone());
    }
    parts.join(". ")
}

/// One TMDB title offered by the "fix the match" picker, with the confidence
/// [`crate::matching`] gives it against what the filename parsed to. `score` is
/// what lets the UI show *why* the automatic pick landed where it did.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchCandidate {
    pub tmdb_id: u64,
    pub title: String,
    pub original_title: Option<String>,
    pub year: Option<u32>,
    pub poster_url: Option<String>,
    pub overview: Option<String>,
    pub rating: Option<f32>,
    /// Confidence in `0.0..=1.0` that this is the title on disk.
    pub score: f32,
    /// Already the stored match for this element.
    pub current: bool,
}

/// `GET /api/rematch/{kind}/{id}/candidates`: what we would match against, and
/// the ranked candidates to choose from.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchCandidates {
    /// The text searched for: the operator's query when they typed one, else the
    /// title parsed from the filename. The UI prefills its search box with it.
    pub query: String,
    /// The year parsed from the filename, which is what scoring compares against.
    pub year: Option<u32>,
    /// The TMDB id currently stored for this element, if it ever resolved.
    pub current_tmdb_id: Option<u64>,
    /// Whether the current match was chosen by an operator rather than resolved
    /// automatically (i.e. whether there is something to reset).
    pub pinned: bool,
    pub results: Vec<MatchCandidate>,
}
