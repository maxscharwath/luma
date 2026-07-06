//! Library file organization: Sonarr/Radarr-style naming ([`naming`]) and the
//! bulk rename tool that moves existing library files to match the configured
//! templates. Item logical ids are derived from the parsed title+year, not the
//! path, so renaming to a cleaner (still-parseable) name preserves watched
//! state / progress / my-list.

pub mod naming;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::db;
use crate::model::{
    Kind, MediaFile, MediaItem, OrganizeMove, OrganizePlan, OrganizeResult, SampleNames, Show,
};
use crate::services::settings::library_defs;
use crate::state::SharedState;

use naming::{NameContext, NamingTemplates};

/// Proper display title + year per show id, preferring the enriched TMDB
/// metadata over the parsed folder name (so a rename produces "Breaking Bad",
/// not "breaking bad").
fn show_info(shows: &[Show]) -> HashMap<String, (String, Option<u32>)> {
    shows
        .iter()
        .map(|s| {
            let title = s
                .metadata
                .as_ref()
                .and_then(|m| m.title.clone())
                .filter(|t| !t.trim().is_empty())
                .unwrap_or_else(|| s.title.clone());
            let year = s
                .year
                .or_else(|| s.metadata.as_ref().and_then(|m| year_of(m.release_date.as_deref())));
            (s.id.clone(), (title, year))
        })
        .collect()
}

fn year_of(date: Option<&str>) -> Option<u32> {
    date.and_then(|d| d.get(..4)).and_then(|y| y.parse().ok())
}

/// Render one movie + one episode example for the live template preview.
pub fn sample(tpl: &NamingTemplates) -> SampleNames {
    let movie = NameContext {
        title: "The Matrix".into(),
        year: Some(1999),
        resolution: Some("1080p".into()),
        source: Some("Bluray".into()),
        ..Default::default()
    };
    let episode = NameContext {
        title: "Breaking Bad".into(),
        year: Some(2008),
        season: Some(1),
        episode: Some(2),
        episode_title: Some("Cat's in the Bag...".into()),
        resolution: Some("1080p".into()),
        source: Some("WEBDL".into()),
        ..Default::default()
    };
    SampleNames {
        movie: tpl.movie_rel_path(&movie, "mkv").to_string_lossy().into_owned(),
        episode: tpl.episode_rel_path(&episode, "mkv").to_string_lossy().into_owned(),
    }
}

/// Compute the rename plan: every library file whose current path doesn't match
/// the configured templates. Non-destructive.
pub fn plan(state: &SharedState) -> Result<OrganizePlan> {
    let tpl = NamingTemplates::from_settings(&state.settings);
    let libs = library_defs(&state.settings, &state.config);
    let folders: HashMap<String, Vec<PathBuf>> =
        libs.into_iter().map(|d| (d.id, d.folders.into_iter().map(PathBuf::from).collect())).collect();

    let shows = db::list_shows(&state.db, None)?;
    let shows_by_id = show_info(&shows);
    let items = db::list_items(&state.db, None)?;

    let mut moves = Vec::new();
    let (mut total, mut matching) = (0u32, 0u32);
    for item in &items {
        for file in &item.files {
            let Some(abs) = current_abs(file) else { continue };
            let Some(root) = library_root(&folders, &item.library, &abs) else { continue };
            let Some((expected_rel, title)) = expected_rel(&tpl, item, file, &shows_by_id) else {
                continue;
            };
            total += 1;
            let expected_abs = root.join(&expected_rel);
            let current_rel = abs.strip_prefix(&root).unwrap_or(&abs);
            if expected_abs == abs {
                matching += 1;
            } else {
                moves.push(OrganizeMove {
                    title,
                    kind: if item.kind == Kind::Episode { "episode" } else { "movie" }.into(),
                    from: current_rel.to_string_lossy().into_owned(),
                    to: expected_rel.to_string_lossy().into_owned(),
                });
            }
        }
    }
    // Stable, readable order.
    moves.sort_by(|a, b| a.to.cmp(&b.to));
    Ok(OrganizePlan { moves, total_files: total, matching })
}

/// Apply the rename plan: recompute + move every mismatched file (same-filesystem
/// rename preserves the inode; item ids are title/year-based so watched/progress
/// survive). Emptied source folders are pruned; a scan is chained afterward.
pub fn apply(state: &SharedState, log: &dyn Fn(String)) -> Result<OrganizeResult> {
    let tpl = NamingTemplates::from_settings(&state.settings);
    let libs = library_defs(&state.settings, &state.config);
    let folders: HashMap<String, Vec<PathBuf>> =
        libs.into_iter().map(|d| (d.id, d.folders.into_iter().map(PathBuf::from).collect())).collect();
    let shows = db::list_shows(&state.db, None)?;
    let shows_by_id = show_info(&shows);
    let items = db::list_items(&state.db, None)?;

    let mut result = OrganizeResult { moved: 0, failed: 0, errors: Vec::new() };
    for item in &items {
        for file in &item.files {
            let Some(abs) = current_abs(file) else { continue };
            let Some(root) = library_root(&folders, &item.library, &abs) else { continue };
            let Some((expected_rel, title)) = expected_rel(&tpl, item, file, &shows_by_id) else {
                continue;
            };
            let dest = root.join(&expected_rel);
            if dest == abs {
                continue;
            }
            match move_file(&abs, &dest) {
                Ok(()) => {
                    log(format!("{title}: {} -> {}", abs.display(), dest.display()));
                    result.moved += 1;
                    prune_empty_dirs(abs.parent(), &root);
                }
                Err(e) => {
                    result.failed += 1;
                    result.errors.push(format!("{title}: {e:#}"));
                }
            }
        }
    }
    if result.moved > 0 {
        let _ = state.jobs.trigger(state.clone(), crate::services::jobs::JobKey("library.scan"), "organize");
    }
    Ok(result)
}

/// The naming context + display title for one library file, or `None` if it
/// can't be placed (loose video, missing episode numbers...).
fn expected_rel(
    tpl: &NamingTemplates,
    item: &MediaItem,
    file: &MediaFile,
    shows_by_id: &HashMap<String, (String, Option<u32>)>,
) -> Option<(PathBuf, String)> {
    let abs = current_abs(file)?;
    let ext = abs.extension()?.to_str()?.to_string();
    // Quality: resolution/codec from the probe, source from the current name.
    let width = file.video.as_ref().and_then(|v| v.width).map(|w| w as i64);
    let resolution = naming::resolution_from_width(width);
    let codec = naming::codec_label(file.video.as_ref().map(|v| v.codec.as_str()));
    let source = {
        let name = abs.file_stem()?.to_str()?;
        let (_, _, s) = naming::quality_from_parsed(&luma_release::parse_release_name(name));
        s
    };

    match item.kind {
        Kind::Movie | Kind::Video => {
            let title = movie_title(item);
            let ctx = NameContext {
                title: title.clone(),
                year: item.year,
                resolution,
                codec,
                source,
                ..Default::default()
            };
            Some((tpl.movie_rel_path(&ctx, &ext), title))
        }
        Kind::Episode => {
            let (season, episode) = (item.season?, item.episode?);
            // Proper show title + year from the enriched show, falling back to
            // the episode item's own parsed show title.
            let (show_title, year) = item
                .show_id
                .as_ref()
                .and_then(|id| shows_by_id.get(id))
                .cloned()
                .or_else(|| item.show_title.clone().map(|t| (t, item.year)))?;
            let ctx = NameContext {
                title: show_title.clone(),
                year,
                season: Some(season),
                episode: Some(episode),
                episode_title: item.episode_title.clone(),
                resolution,
                codec,
                source,
            };
            Some((tpl.episode_rel_path(&ctx, &ext), format!("{show_title} S{season:02}E{episode:02}")))
        }
    }
}

/// Prefer the localized TMDB title, then the parsed item title.
fn movie_title(item: &MediaItem) -> String {
    item.metadata
        .as_ref()
        .and_then(|m| m.title.clone())
        .filter(|t| !t.trim().is_empty())
        .unwrap_or_else(|| item.title.clone())
}

fn current_abs(file: &MediaFile) -> Option<PathBuf> {
    file.abs_path.as_deref().filter(|p| !p.starts_with("demo://")).map(PathBuf::from)
}

/// The library folder that contains `abs` (so a rename stays within the same
/// root / filesystem).
fn library_root(folders: &HashMap<String, Vec<PathBuf>>, lib_id: &str, abs: &Path) -> Option<PathBuf> {
    let roots = folders.get(lib_id)?;
    roots.iter().find(|root| abs.starts_with(root)).cloned().or_else(|| roots.first().cloned())
}

/// Rename in place, refusing to overwrite an existing different file.
fn move_file(from: &Path, to: &Path) -> Result<()> {
    if from == to {
        return Ok(());
    }
    if to.exists() {
        anyhow::bail!("target already exists: {}", to.display());
    }
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // Same filesystem: an atomic rename (keeps the inode). Cross-device: copy
    // then remove the source.
    match std::fs::rename(from, to) {
        Ok(()) => Ok(()),
        Err(_) => {
            std::fs::copy(from, to)?;
            std::fs::remove_file(from)?;
            Ok(())
        }
    }
}

/// Remove now-empty source directories up to (but not including) the library
/// root, so a rename doesn't leave orphan folders behind.
fn prune_empty_dirs(dir: Option<&Path>, root: &Path) {
    let mut cur = dir.map(Path::to_path_buf);
    while let Some(d) = cur {
        if d == root || !d.starts_with(root) {
            break;
        }
        let empty = std::fs::read_dir(&d).map(|mut e| e.next().is_none()).unwrap_or(false);
        if !empty || std::fs::remove_dir(&d).is_err() {
            break;
        }
        cur = d.parent().map(Path::to_path_buf);
    }
}
