//! `cache.cleanup` wipe the on-demand HLS transcode cache (disposable;
//! regenerated on playback), then enforce the `cacheLimit` budget on the
//! poster/backdrop image cache (never auto-wiped expensive to refetch so it
//! grows unbounded without this; trimmed oldest-first when over the limit).

use std::path::Path;

use super::prelude::*;

pub(super) fn run(ctx: &JobContext) -> Result<()> {
    let data_dir = &ctx.state.config.data_dir;

    // 1) Wipe the disposable HLS transcode cache.
    let transcode = data_dir.join("transcode");
    ctx.info(format!("clearing transcode cache at {}", transcode.display()));
    let before = dir_size(&transcode);
    let entries: Vec<_> = std::fs::read_dir(&transcode)
        .map(|rd| rd.flatten().collect())
        .unwrap_or_default();
    let total = entries.len();
    for (i, entry) in entries.iter().enumerate() {
        if ctx.cancelled() {
            ctx.warn("cancellation requested stopping cache cleanup");
            break;
        }
        let p = entry.path();
        if p.is_dir() {
            let _ = std::fs::remove_dir_all(&p);
        } else {
            let _ = std::fs::remove_file(&p);
        }
        ctx.progress(i + 1, total);
    }
    let freed = before.saturating_sub(dir_size(&transcode));
    ctx.info(format!("freed {} across {total} transcode entries", human_bytes(freed)));

    // 2) Enforce the image-cache budget (`cacheLimit`).
    enforce_image_limit(ctx, &data_dir.join("images"));
    Ok(())
}

/// Trim the poster/backdrop image cache to the configured `cacheLimit`, deleting
/// oldest files first. "Illimité"/"Unlimited" (or any non-numeric value) disables
/// trimming. A deleted poster is re-downloaded on the next enrichment.
fn enforce_image_limit(ctx: &JobContext, images: &Path) {
    let label = ctx.state.settings.get_str("cacheLimit", "80 Go");
    let Some(limit) = parse_limit_bytes(&label) else {
        ctx.info(format!("image cache limit “{label}” → no trimming"));
        return;
    };
    let used = dir_size(images);
    if used <= limit {
        ctx.info(format!("image cache {} within limit {}", human_bytes(used), human_bytes(limit)));
        return;
    }

    // Oldest-first by mtime: least-recently fetched art is evicted first.
    let mut files: Vec<(std::path::PathBuf, u64, std::time::SystemTime)> =
        walkdir::WalkDir::new(images)
            .into_iter()
            .filter_map(std::result::Result::ok)
            .filter(|e| e.file_type().is_file())
            .filter_map(|e| {
                let m = e.metadata().ok()?;
                Some((e.path().to_path_buf(), m.len(), m.modified().ok()?))
            })
            .collect();
    files.sort_by_key(|(_, _, mtime)| *mtime);

    let (mut remaining, mut freed, mut removed) = (used, 0u64, 0usize);
    for (path, size, _) in files {
        if remaining <= limit || ctx.cancelled() {
            break;
        }
        if std::fs::remove_file(&path).is_ok() {
            remaining = remaining.saturating_sub(size);
            freed += size;
            removed += 1;
        }
    }
    ctx.info(format!(
        "image cache over limit ({} > {}) trimmed {} across {removed} files (now {})",
        human_bytes(used),
        human_bytes(limit),
        human_bytes(freed),
        human_bytes(remaining),
    ));
}

/// Parse a cache-limit label (`"80 Go"`, `"256 Go"`, `"Illimité"`, …) into a byte
/// budget. `None` = unlimited / unparseable → no trimming. Labels use decimal
/// "Go" (gigaoctets), so 1 Go = 1e9 bytes.
fn parse_limit_bytes(label: &str) -> Option<u64> {
    let digits: String = label.chars().take_while(|c| !c.is_alphabetic()).filter(char::is_ascii_digit).collect();
    match digits.parse::<u64>() {
        Ok(n) if n > 0 => Some(n * 1_000_000_000),
        _ => None,
    }
}

/// Recursive byte size of a directory tree (0 if missing).
fn dir_size(path: &Path) -> u64 {
    walkdir::WalkDir::new(path)
        .into_iter()
        .filter_map(std::result::Result::ok)
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| e.metadata().ok())
        .map(|m| m.len())
        .sum()
}

/// Compact human byte size for log lines (e.g. `1.4 GB`).
fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{size:.1} {}", UNITS[unit])
    }
}
