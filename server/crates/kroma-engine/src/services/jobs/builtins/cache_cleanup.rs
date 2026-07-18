//! `cache.cleanup` report the on-demand HLS segment cache size (trimmed live by
//! infra::hls against the `transcodeCacheLimit` byte budget - idle / superseded
//! sessions evicted oldest-first, live ones untouched - so no manual wipe here),
//! then enforce the `cacheLimit` budget on the poster/backdrop image cache (never
//! auto-wiped expensive to refetch so it grows unbounded without this; trimmed
//! oldest-first when over the limit).

use std::collections::HashSet;
use std::path::Path;

use super::prelude::*;

/// Nightly maintenance: report the HLS cache footprint + trim the image cache.
pub(super) const SPEC: Builtin = Builtin {
    key: JobKey("cache.cleanup"),
    category: Category::Maintenance,
    schedule: Some("0 4 * * *"),
    triggers: &[],
    run,
};

pub(super) fn run(ctx: &JobContext) -> Result<()> {
    let data_dir = &ctx.state.config.data_dir;

    // 1) The HLS segment cache trims itself live against its byte budget (see
    // infra::hls); we just report its current footprint. Every segment is
    // regenerable, so there is nothing to protect or wipe here.
    let budget = crate::services::settings::transcode_cache_limit_bytes(&ctx.state.settings);
    let limit = if budget == 0 { "unlimited".to_string() } else { human_bytes(budget) };
    ctx.info(format!("HLS segment cache: {} / {limit} budget", human_bytes(ctx.state.hls.cache_bytes())));

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

    // Uploaded avatars live in this same dir (image::store_upload) but are NOT
    // regenerable art, so never trim a file an account still references it would
    // 404 the avatar with no way to refetch.
    let protected: HashSet<String> = crate::db::avatar_urls(&ctx.state.db)
        .unwrap_or_default()
        .iter()
        .filter_map(|u| u.rsplit('/').next().map(str::to_owned))
        .collect();

    // Oldest-first by mtime: least-recently fetched art is evicted first.
    let mut files: Vec<(std::path::PathBuf, u64, std::time::SystemTime)> =
        walkdir::WalkDir::new(images)
            .into_iter()
            .filter_map(std::result::Result::ok)
            .filter(|e| e.file_type().is_file())
            .filter(|e| {
                !e.path()
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| protected.contains(n))
            })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_limit_bytes_reads_leading_gigabytes() {
        assert_eq!(parse_limit_bytes("80 Go"), Some(80_000_000_000));
        assert_eq!(parse_limit_bytes("256 Go"), Some(256_000_000_000));
        // Non-numeric / zero / empty labels mean "unlimited" -> no budget.
        assert_eq!(parse_limit_bytes("Illimité"), None);
        assert_eq!(parse_limit_bytes("0 Go"), None);
        assert_eq!(parse_limit_bytes(""), None);
    }

    #[test]
    fn human_bytes_scales_units() {
        assert_eq!(human_bytes(0), "0 B");
        assert_eq!(human_bytes(512), "512 B");
        assert_eq!(human_bytes(1024), "1.0 KB");
        assert_eq!(human_bytes(1536), "1.5 KB");
        assert_eq!(human_bytes(1024 * 1024), "1.0 MB");
        assert_eq!(human_bytes(1_500_000_000), "1.4 GB");
    }

    #[test]
    fn dir_size_sums_files_recursively_and_zero_when_missing() {
        use std::sync::atomic::{AtomicU32, Ordering};
        static SEQ: AtomicU32 = AtomicU32::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("kroma-dirsize-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        // A path that does not exist sums to zero.
        assert_eq!(dir_size(&dir), 0);

        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("a.bin"), b"abc").unwrap(); // 3 bytes
        std::fs::write(dir.join("sub/b.bin"), b"hello").unwrap(); // 5 bytes
        assert_eq!(dir_size(&dir), 8);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
