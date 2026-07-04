//! Self-provisioning of the `cloudflared` binary the connector runs.
//!
//! On first enable the server downloads the official `cloudflared` for its
//! platform into `<data_dir>/bin` using `curl` (the same HTTP path as the rest of
//! the server). The download is validated by running `--version` before it is
//! accepted, and served over HTTPS from Cloudflare's official GitHub releases.
//!
//! We track the `latest` release for robustness (a hardcoded tag can 404 as
//! releases roll); swap `RELEASE` for a versioned URL to pin.

use std::path::{Path, PathBuf};

use tokio::process::Command;

/// Official release base. `latest/download/<asset>` always resolves to the newest
/// build for that asset name.
const RELEASE: &str = "https://github.com/cloudflare/cloudflared/releases/latest/download";

/// The cached binary's file name.
pub fn bin_name() -> &'static str {
    if cfg!(windows) {
        "cloudflared.exe"
    } else {
        "cloudflared"
    }
}

/// Where a self-provisioned copy lives.
pub fn cached_path(data_dir: &Path) -> PathBuf {
    data_dir.join("bin").join(bin_name())
}

/// The release asset for this platform + whether it's a gzip tarball (macOS) as
/// opposed to a directly-usable binary (Linux / Windows).
fn asset() -> Option<(&'static str, bool)> {
    let a = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => ("cloudflared-linux-amd64", false),
        ("linux", "aarch64") => ("cloudflared-linux-arm64", false),
        ("linux", "arm") => ("cloudflared-linux-arm", false),
        ("linux", "x86") => ("cloudflared-linux-386", false),
        ("macos", "x86_64") => ("cloudflared-darwin-amd64.tgz", true),
        ("macos", "aarch64") => ("cloudflared-darwin-arm64.tgz", true),
        ("windows", "x86_64") => ("cloudflared-windows-amd64.exe", false),
        ("windows", "x86") => ("cloudflared-windows-386.exe", false),
        _ => return None,
    };
    Some(a)
}

/// Download + install cloudflared into `<data_dir>/bin`, returning its path.
/// Validates the result by running `--version` (a corrupt/partial download is
/// rejected and removed).
pub async fn download(data_dir: &Path) -> Result<PathBuf, String> {
    let (asset, is_tgz) = asset().ok_or_else(|| {
        format!("no cloudflared build for {}/{}", std::env::consts::OS, std::env::consts::ARCH)
    })?;
    let bindir = data_dir.join("bin");
    std::fs::create_dir_all(&bindir).map_err(|e| format!("create bin dir: {e}"))?;
    let dest = bindir.join(bin_name());
    let tmp = bindir.join(format!("cloudflared.download.{}", std::process::id()));
    let url = format!("{RELEASE}/{asset}");

    let ok = Command::new("curl")
        .args(["-fSL", "--max-time", "180", "-o"])
        .arg(&tmp)
        .arg(&url)
        .status()
        .await
        .map(|s| s.success())
        .unwrap_or(false);
    if !ok {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!("download failed: {url}"));
    }

    if is_tgz {
        // The macOS asset is a tarball holding a single `cloudflared`; extract it
        // straight into bindir (== dest for macOS).
        let extracted = Command::new("tar")
            .arg("-xzf")
            .arg(&tmp)
            .arg("-C")
            .arg(&bindir)
            .arg("cloudflared")
            .status()
            .await
            .map(|s| s.success())
            .unwrap_or(false);
        let _ = std::fs::remove_file(&tmp);
        if !extracted {
            return Err("failed to extract cloudflared archive".to_string());
        }
    } else {
        std::fs::rename(&tmp, &dest).map_err(|e| format!("install cloudflared: {e}"))?;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o755));
    }

    // Validate: it must actually run, else discard the (partial/corrupt) file.
    let runs = Command::new(&dest)
        .arg("--version")
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !runs {
        let _ = std::fs::remove_file(&dest);
        return Err("downloaded cloudflared is not runnable".to_string());
    }
    Ok(dest)
}
