//! The built-in jobs shipped with LUMA one handler file per job in this
//! directory, wired into the typed [`JOBS`] registry below.
//!
//! A job's identity is a [`JobId`] (not a string), so every reference is
//! compiler-checked + autocompleted and the set is exported to the clients. The
//! handler is a plain `fn` in its own file; this registry is the single typed
//! list of what ships. Adding a job: drop a handler file, add its `mod` line +
//! one [`JOBS`] row + a [`JobId`] variant the compiler enforces all three line
//! up.

use crate::model::{Category, JobId};

use super::{JobContext, JobManager, Trigger};

mod cache_cleanup;
mod library_scan;
mod metadata_enrich;
mod reco_refresh;
mod reembed;
mod search_reindex;
mod sections_curate;
mod sections_personalize;

/// A built-in job descriptor: typed identity + metadata + handler. The handler
/// lives in the per-job file; this is the registry entry.
pub struct Builtin {
    pub id: JobId,
    pub category: Category,
    /// Default cron schedule (user-overridable), or `None` for manual-only.
    pub schedule: Option<&'static str>,
    /// Extra trigger sources beyond manual + cron (file-watch, chaining).
    pub triggers: &'static [Trigger],
    pub run: fn(&JobContext) -> anyhow::Result<()>,
}

/// Every built-in job, in admin-listing order.
static JOBS: &[Builtin] = &[
    Builtin {
        id: JobId::CacheCleanup,
        category: Category::Maintenance,
        schedule: Some("0 4 * * *"),
        triggers: &[],
        run: cache_cleanup::run,
    },
    Builtin {
        id: JobId::RecommendationsRefresh,
        category: Category::Recommendations,
        schedule: Some("0 5 * * *"),
        triggers: &[],
        run: reco_refresh::run,
    },
    Builtin {
        id: JobId::RecommendationsReembed,
        category: Category::Recommendations,
        schedule: None,
        triggers: &[],
        run: reembed::run,
    },
    Builtin {
        id: JobId::SectionsPersonalize,
        category: Category::Recommendations,
        schedule: Some("30 5 * * *"),
        triggers: &[],
        run: sections_personalize::run,
    },
    Builtin {
        id: JobId::SectionsCurate,
        category: Category::Recommendations,
        schedule: Some("0 6 * * *"),
        triggers: &[],
        run: sections_curate::run,
    },
    Builtin {
        id: JobId::LibraryScan,
        category: Category::Library,
        schedule: None,
        triggers: &[Trigger::LibraryChange],
        run: library_scan::run,
    },
    Builtin {
        id: JobId::MetadataEnrich,
        category: Category::Library,
        schedule: None,
        triggers: &[],
        run: metadata_enrich::run,
    },
    Builtin {
        id: JobId::SearchReindex,
        category: Category::Library,
        schedule: None,
        triggers: &[],
        run: search_reindex::run,
    },
    Builtin {
        id: JobId::MarkersDetect,
        category: Category::Library,
        // Nightly + manual only: fingerprinting decodes every episode's audio, so
        // it must not fire on every library change.
        schedule: Some("0 3 * * *"),
        triggers: &[],
        run: crate::services::markers::job::run,
    },
];

/// Register every built-in job on the manager (registration order = listing
/// order). Called once at startup.
pub fn register_all(m: &mut JobManager) {
    for b in JOBS {
        m.register(b);
    }
}

/// Shared imports for the per-job handler files `use super::prelude::*;`.
pub(crate) mod prelude {
    pub(crate) use super::snippet;
    pub(crate) use crate::services::jobs::JobContext;
    pub use anyhow::Result;
}

/// A single-line, length-capped snippet of an LLM reply, for error logs (so a
/// parse failure shows *what* the model returned without flooding). Shared by the
/// personalize + curate jobs.
pub(crate) fn snippet(text: &str) -> String {
    let one_line: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if one_line.chars().count() > 500 {
        format!("{}…", one_line.chars().take(500).collect::<String>())
    } else {
        one_line
    }
}

#[cfg(test)]
mod tests {
    use super::JOBS;
    use crate::model::JobId;

    /// Every `JobId` variant has exactly one registry row, and ids are unique
    /// so the typed registry and the id enum can't drift apart.
    #[test]
    fn registry_covers_every_jobid() {
        assert_eq!(JOBS.len(), JobId::ALL.len(), "JOBS rows vs JobId variants");
        for id in JobId::ALL {
            let rows = JOBS.iter().filter(|b| b.id == id).count();
            assert_eq!(rows, 1, "JobId::{id:?} must have exactly one JOBS row, found {rows}");
        }
    }
}
