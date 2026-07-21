//! Problem reports (the "signaler un probleme" flow): any user can flag an issue
//! on a movie / show / episode (wrong metadata, audio, video, subtitles, other);
//! `reports.manage` holders triage them from the admin console. Pure data
//! (serde); persistence lives in `crate::db`, the create/triage handlers in
//! `crate::api::{reports, admin::reports}`.
//!
//! The JSON shape here is a public contract web/TV clients depend on it, so
//! field names and casing must not drift. Timestamps are epoch milliseconds.

use serde::{Deserialize, Serialize};

/// What a report is filed against. Movies and episodes both live in the `items`
/// table; a show is its own aggregate. The three share one id space at the report
/// level via `(subject_kind, subject_id)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReportSubjectKind {
    Movie,
    Show,
    Episode,
}

impl ReportSubjectKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ReportSubjectKind::Movie => "movie",
            ReportSubjectKind::Show => "show",
            ReportSubjectKind::Episode => "episode",
        }
    }
    pub fn parse(s: &str) -> Option<ReportSubjectKind> {
        match s {
            "movie" => Some(ReportSubjectKind::Movie),
            "show" => Some(ReportSubjectKind::Show),
            "episode" => Some(ReportSubjectKind::Episode),
            _ => None,
        }
    }
}

/// The nature of the problem being reported. `Metadata` covers a wrong fiche
/// (title / overview / poster / cast / bad TMDB match); `Other` carries a free
/// text message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReportCategory {
    Metadata,
    Video,
    Audio,
    Subtitles,
    Other,
}

impl ReportCategory {
    pub fn as_str(self) -> &'static str {
        match self {
            ReportCategory::Metadata => "metadata",
            ReportCategory::Video => "video",
            ReportCategory::Audio => "audio",
            ReportCategory::Subtitles => "subtitles",
            ReportCategory::Other => "other",
        }
    }
    pub fn parse(s: &str) -> Option<ReportCategory> {
        match s {
            "metadata" => Some(ReportCategory::Metadata),
            "video" => Some(ReportCategory::Video),
            "audio" => Some(ReportCategory::Audio),
            "subtitles" => Some(ReportCategory::Subtitles),
            "other" => Some(ReportCategory::Other),
            _ => None,
        }
    }
}

/// A report's triage state. `Open` awaits an admin; `Resolved` was acted on;
/// `Dismissed` was judged not actionable. Both terminal states record who/when.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReportStatus {
    Open,
    Resolved,
    Dismissed,
}

impl ReportStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            ReportStatus::Open => "open",
            ReportStatus::Resolved => "resolved",
            ReportStatus::Dismissed => "dismissed",
        }
    }
    pub fn parse(s: &str) -> Option<ReportStatus> {
        match s {
            "open" => Some(ReportStatus::Open),
            "resolved" => Some(ReportStatus::Resolved),
            "dismissed" => Some(ReportStatus::Dismissed),
            _ => None,
        }
    }
}

/// One problem report, as listed in the admin queue (the reporter's username is
/// hydrated for display). `subject_title` is snapshotted at report time so the
/// queue survives a re-scan or deletion of the underlying title.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub id: String,
    pub subject_kind: ReportSubjectKind,
    /// The local catalog id (movie/episode item id, or show id). Deep-links the
    /// admin queue straight to the title's fiche.
    pub subject_id: String,
    /// Denormalized display title at report time.
    pub subject_title: String,
    pub category: ReportCategory,
    /// Optional free-text detail from the reporter.
    pub message: Option<String>,
    pub status: ReportStatus,
    pub reported_by: Option<String>,
    /// Reporter's username, hydrated for the admin queue.
    pub reported_by_name: Option<String>,
    pub resolved_by: Option<String>,
    pub resolved_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Status tallies for the admin queue's filter chips.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportCounts {
    pub total: u32,
    pub open: u32,
    pub resolved: u32,
    pub dismissed: u32,
}

/// `GET /api/admin/reports`.
#[derive(Debug, Clone, Serialize)]
pub struct ReportsView {
    pub reports: Vec<Report>,
    pub counts: ReportCounts,
}

/// `POST /api/reports` body. The server resolves `subjectTitle` from the catalog
/// (validating the subject exists), so clients never send it.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateReportBody {
    pub subject_kind: ReportSubjectKind,
    pub subject_id: String,
    pub category: ReportCategory,
    #[serde(default)]
    pub message: Option<String>,
}
