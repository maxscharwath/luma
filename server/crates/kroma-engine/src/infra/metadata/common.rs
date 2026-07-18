//! Raw TMDB `credits` / `created_by` JSON shapes and the JSON->domain mappers
//! shared by the two TMDB adapters: [`super::client`] (library enrichment) and
//! [`super::discover`] (the request flow's window onto the outside catalog). Both
//! parse the same blocks into the same [`CastMember`] / [`CrewMember`] shapes, so
//! the shapes and mappers live here once.

use serde::Deserialize;

use crate::domain::metadata::{CastMember, CrewMember};

use super::client::IMG;

/// TMDB crew jobs we surface the authorship roles, ranked. Anything else
/// (gaffer, editor, …) is dropped.
pub(super) const KEY_CREW_JOBS: &[&str] = &["Director", "Creator", "Writer", "Screenplay", "Story"];

/// The appended `credits` block cast + crew.
#[derive(Debug, Deserialize)]
pub(super) struct RawCredits {
    #[serde(default)]
    pub cast: Vec<RawCast>,
    #[serde(default)]
    pub crew: Vec<RawCrew>,
}

#[derive(Debug, Deserialize)]
pub(super) struct RawCast {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub character: Option<String>,
    #[serde(default)]
    pub profile_path: Option<String>,
    #[serde(default)]
    pub order: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub(super) struct RawCrew {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub job: String,
}

/// TV `created_by` block (top-level on series details) the show's creators.
#[derive(Debug, Deserialize)]
pub(super) struct RawCreatedBy {
    #[serde(default)]
    pub name: String,
}

/// Top-billed cast (TMDB orders by `order` ascending; sort defensively), capped at
/// `max_cast`; empty characters dropped and photos absolutized to `w185`. With
/// `drop_unnamed` set, rows with an empty name are filtered first (the discover
/// path does this; enrichment keeps TMDB's list as-is).
pub(super) fn build_cast(mut raw: Vec<RawCast>, max_cast: usize, drop_unnamed: bool) -> Vec<CastMember> {
    raw.sort_by_key(|m| m.order.unwrap_or(u32::MAX));
    raw.into_iter()
        .filter(|m| !drop_unnamed || !m.name.is_empty())
        .take(max_cast)
        .map(|m| CastMember {
            name: m.name,
            character: m.character.filter(|s| !s.is_empty()),
            profile_url: m.profile_path.map(|p| format!("{IMG}/w185{p}")),
        })
        .collect()
}

/// Build the capped, deduped authorship-crew list from the `crew` block plus TV
/// `created_by` names: only [`KEY_CREW_JOBS`] survive, directors/creators rank
/// first, one row per person (most senior role wins), capped at `max_crew`. TV
/// series carry their creators in `created_by` (no crew "Director"), folded in as
/// "Creator".
pub(super) fn build_crew(crew: Vec<RawCrew>, created_by: Vec<RawCreatedBy>, max_crew: usize) -> Vec<CrewMember> {
    let rank = |job: &str| KEY_CREW_JOBS.iter().position(|j| *j == job).unwrap_or(usize::MAX);
    let mut candidates: Vec<(usize, CrewMember)> = crew
        .into_iter()
        .filter(|c| !c.name.is_empty() && KEY_CREW_JOBS.contains(&c.job.as_str()))
        .map(|c| (rank(&c.job), CrewMember { name: c.name, job: c.job, profile_url: None }))
        .collect();
    // TV creators (no crew "Director" on series) → treat as "Creator".
    for cb in created_by.into_iter().filter(|c| !c.name.is_empty()) {
        candidates.push((rank("Creator"), CrewMember { name: cb.name, job: "Creator".into(), profile_url: None }));
    }
    // Most senior role first; keep one row per person.
    candidates.sort_by_key(|(r, _)| *r);
    let mut seen = std::collections::HashSet::new();
    candidates
        .into_iter()
        .filter(|(_, m)| seen.insert(m.name.clone()))
        .map(|(_, m)| m)
        .take(max_crew)
        .collect()
}
