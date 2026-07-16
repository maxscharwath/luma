//! Media requests (the "ask for a title" flow) + TMDB discovery wire types.
//! Pure data (serde + ts-rs); persistence lives in `crate::db`, orchestration
//! in `crate::services::requests`, the TMDB adapter in `crate::infra::metadata`.
//!
//! The JSON shape here is a public contract web/TV clients depend on it, so
//! field names and casing must not drift. Timestamps are epoch milliseconds.

use serde::{Deserialize, Serialize};

use crate::metadata::{CastMember, CrewMember};

/// What a request targets. Requests key on TMDB ids, so this mirrors TMDB's
/// movie/tv split under the catalog's own vocabulary (a "show", not a "tv").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RequestKind {
    Movie,
    Show,
}

impl RequestKind {
    pub fn as_str(self) -> &'static str {
        match self {
            RequestKind::Movie => "movie",
            RequestKind::Show => "show",
        }
    }
    pub fn parse(s: &str) -> Option<RequestKind> {
        match s {
            "movie" => Some(RequestKind::Movie),
            "show" => Some(RequestKind::Show),
            _ => None,
        }
    }
}

/// A request's lifecycle state. The DB stores the durable states; the transient
/// acquisition states (`searching`/`downloading`/`importing`) are derived from
/// the wanted/downloads ledgers when a view is built, so clients get one enum
/// for the whole status chip vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestStatus {
    Pending,
    Approved,
    Searching,
    Downloading,
    Importing,
    Available,
    PartiallyAvailable,
    Failed,
    Denied,
}

impl RequestStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            RequestStatus::Pending => "pending",
            RequestStatus::Approved => "approved",
            RequestStatus::Searching => "searching",
            RequestStatus::Downloading => "downloading",
            RequestStatus::Importing => "importing",
            RequestStatus::Available => "available",
            RequestStatus::PartiallyAvailable => "partially_available",
            RequestStatus::Failed => "failed",
            RequestStatus::Denied => "denied",
        }
    }
    pub fn parse(s: &str) -> Option<RequestStatus> {
        match s {
            "pending" => Some(RequestStatus::Pending),
            "approved" => Some(RequestStatus::Approved),
            "searching" => Some(RequestStatus::Searching),
            "downloading" => Some(RequestStatus::Downloading),
            "importing" => Some(RequestStatus::Importing),
            "available" => Some(RequestStatus::Available),
            "partially_available" => Some(RequestStatus::PartiallyAvailable),
            "failed" => Some(RequestStatus::Failed),
            "denied" => Some(RequestStatus::Denied),
            _ => None,
        }
    }
}

/// One (season, episode) pair, for a request that targets individual episodes
/// rather than whole seasons. `1`-based, mirroring TMDB's numbering.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EpisodeRef {
    pub season: u32,
    pub episode: u32,
}

/// One media request, as listed to clients (the requester sees their own; a
/// `requests.manage` holder sees everyone's, with the requester hydrated).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaRequest {
    pub id: String,
    pub kind: RequestKind,
    pub tmdb_id: u64,
    /// Denormalized at request time so list views need no TMDB call.
    pub title: String,
    pub year: Option<u32>,
    pub poster_url: Option<String>,
    /// Requested season numbers; `None` = the whole show (or a movie).
    pub seasons: Option<Vec<u32>>,
    /// Individual episodes requested alongside any full seasons. `None` = none.
    /// A show's target is the union of `seasons` (full) and `episodes`; both
    /// `None` = every season.
    pub episodes: Option<Vec<EpisodeRef>>,
    pub status: RequestStatus,
    pub requested_by: Option<String>,
    /// Requester's username, hydrated for the admin queue.
    pub requested_by_name: Option<String>,
    pub reviewed_by: Option<String>,
    /// Denial reason / admin note.
    pub note: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    /// Live download progress (0..1) when the request is `downloading` /
    /// `importing`, derived from its download rows. `None` otherwise.
    #[serde(default)]
    pub progress: Option<f64>,
    /// TMDB airing status, refreshed by the `acquisition.refresh` job (show:
    /// "Returning Series"/"Ended"/…; movie: "Released"/"Post Production"/…).
    /// `None` until the first refresh.
    pub air_status: Option<String>,
    /// Next air date (`YYYY-MM-DD`): a show's next episode, or an unreleased
    /// movie's soonest availability. `None` once nothing more is upcoming.
    pub next_air_date: Option<String>,
    /// Epoch-ms of the last TMDB refresh (throttles the refresh pass). Internal:
    /// never serialized to clients.
    #[serde(skip)]
    pub last_refresh_at: Option<i64>,
}

/// Status tallies for the admin queue's filter chips.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestCounts {
    pub total: u32,
    pub pending: u32,
    pub active: u32,
    pub available: u32,
    pub denied: u32,
    pub failed: u32,
}

/// `GET /api/requests`.
#[derive(Debug, Clone, Serialize)]
pub struct RequestsView {
    pub requests: Vec<MediaRequest>,
    pub counts: RequestCounts,
}

/// `POST /api/requests` body.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateRequestBody {
    pub kind: RequestKind,
    pub tmdb_id: u64,
    /// For shows: the seasons to request; `None`/empty = every season.
    #[serde(default)]
    pub seasons: Option<Vec<u32>>,
    /// For shows: individual episodes to request, unioned with `seasons`.
    /// `None`/empty = no per-episode ask.
    #[serde(default)]
    pub episodes: Option<Vec<EpisodeRef>>,
}

/// One TMDB discovery result, flagged against the local catalog + open
/// requests so cards can render Play / status chip / request button directly.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoverEntry {
    pub kind: RequestKind,
    pub tmdb_id: u64,
    pub title: String,
    pub year: Option<u32>,
    pub poster_url: Option<String>,
    pub backdrop_url: Option<String>,
    pub overview: Option<String>,
    pub rating: Option<f32>,
    /// Present in the local catalog (movie item / show).
    pub in_library: bool,
    /// The local catalog id when `in_library` (deep-link to the real fiche).
    pub local_id: Option<String>,
    /// The open request covering this title, when one exists.
    pub request_id: Option<String>,
    pub request_status: Option<RequestStatus>,
    /// Live download progress (0..1) while downloading/importing.
    #[serde(default)]
    pub request_progress: Option<f64>,
}

/// `GET /api/discover/search` / `GET /api/discover/trending`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoverResponse {
    pub results: Vec<DiscoverEntry>,
    pub page: u32,
    pub total_pages: u32,
}

/// One season row in a show's discovery detail (drives the season picker).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoverSeason {
    pub season: u32,
    pub name: Option<String>,
    pub episode_count: u32,
    pub air_date: Option<String>,
    /// Every episode of this season is already in the library.
    pub available: bool,
    /// How many of the season's episodes are on disk (for "4/6" partial state).
    pub episodes_available: u32,
    /// Covered by an open request.
    pub requested: bool,
}

/// `GET /api/discover/{movie,tv}/:tmdbId`: the request-flow detail page.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoverDetail {
    pub kind: RequestKind,
    pub tmdb_id: u64,
    pub title: String,
    pub year: Option<u32>,
    pub poster_url: Option<String>,
    pub backdrop_url: Option<String>,
    pub overview: Option<String>,
    pub tagline: Option<String>,
    pub genres: Vec<String>,
    pub rating: Option<f32>,
    pub runtime_min: Option<u32>,
    /// Empty for movies.
    pub seasons: Vec<DiscoverSeason>,
    /// Top-billed cast (name + character + photo), from TMDB credits. Empty when
    /// the provider returned none.
    #[serde(default)]
    pub cast: Vec<CastMember>,
    /// Key crew (directors / creators / writers), for the "Réalisation" line.
    #[serde(default)]
    pub crew: Vec<CrewMember>,
    /// "Titres similaires" TMDB recommendations, flagged against the local
    /// catalog + open requests so each tile deep-links correctly.
    #[serde(default)]
    pub similar: Vec<DiscoverEntry>,
    pub in_library: bool,
    pub local_id: Option<String>,
    pub request_id: Option<String>,
    pub request_status: Option<RequestStatus>,
    /// Live download progress (0..1) while the request is downloading/importing.
    #[serde(default)]
    pub request_progress: Option<f64>,
    /// TMDB airing status (show: "Returning Series"/"Ended"/…; movie:
    /// "Released"/…), for the "coming soon" badge. `None` when unknown.
    pub air_status: Option<String>,
    /// Next air date (`YYYY-MM-DD`): a show's next episode, or a movie's soonest
    /// availability. `None` when nothing is upcoming.
    pub next_air_date: Option<String>,
}
