//! Wire types for file naming templates + the library rename tool. Pure data
//! (serde + ts-rs); the engine lives in `crate::services::organize`.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// The five naming templates (Sonarr/Radarr-style token strings).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct NamingTemplatesView {
    pub movie_folder: String,
    pub movie_file: String,
    pub series_folder: String,
    pub season_folder: String,
    pub episode_file: String,
}

/// `GET /api/admin/organize/naming` current templates + a rendered sample.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct NamingView {
    pub templates: NamingTemplatesView,
    pub sample: SampleNames,
}

/// Example rendered names for the live preview.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SampleNames {
    /// e.g. `The Matrix (1999)/The Matrix (1999) Bluray-1080p.mkv`
    pub movie: String,
    /// e.g. `Breaking Bad (2008)/Season 01/Breaking Bad - S01E02 - ... .mkv`
    pub episode: String,
}

/// `POST /api/admin/organize/sample` body (render as the admin types).
pub type SampleBody = NamingTemplatesView;

/// One file the rename tool would move.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct OrganizeMove {
    pub title: String,
    /// `movie` | `episode`.
    pub kind: String,
    /// Current path, relative to its library folder.
    pub from: String,
    /// Expected path, relative to its library folder.
    pub to: String,
}

/// `GET /api/admin/organize/preview`.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct OrganizePlan {
    pub moves: Vec<OrganizeMove>,
    /// Total library files considered.
    pub total_files: u32,
    /// Files already matching the templates.
    pub matching: u32,
}

/// `POST /api/admin/organize/apply` result.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct OrganizeResult {
    pub moved: u32,
    pub failed: u32,
    pub errors: Vec<String>,
}
