//! Self-provisioning of the `wireproxy` binary (userspace WireGuard exposing a
//! SOCKS5 proxy), mirroring the cloudflared provisioner. Static Go binary,
//! ISC-licensed, official releases from github.com/windtf/wireproxy; assets
//! are tarballs named `wireproxy_<os>_<arch>.tar.gz` holding one `wireproxy`.

use std::path::{Path, PathBuf};

use tokio::process::Command;

const RELEASE: &str = "https://github.com/windtf/wireproxy/releases/latest/download";

fn cached_path(data_dir: &Path) -> PathBuf {
    data_dir.join("bin").join("wireproxy")
}

fn asset() -> Option<&'static str> {
    Some(match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => "wireproxy_linux_amd64.tar.gz",
        ("linux", "aarch64") => "wireproxy_linux_arm64.tar.gz",
        ("linux", "arm") => "wireproxy_linux_arm.tar.gz",
        ("macos", "x86_64") => "wireproxy_darwin_amd64.tar.gz",
        ("macos", "aarch64") => "wireproxy_darwin_arm64.tar.gz",
        _ => return None,
    })
}

/// The wireproxy binary path, downloading it on first use.
pub async fn ensure(data_dir: &Path) -> Result<PathBuf, String> {
    let dest = cached_path(data_dir);
    if dest.exists() {
        return Ok(dest);
    }
    download(data_dir).await
}

async fn download(data_dir: &Path) -> Result<PathBuf, String> {
    let asset = asset().ok_or_else(|| {
        format!("no wireproxy build for {}/{}", std::env::consts::OS, std::env::consts::ARCH)
    })?;
    let bindir = data_dir.join("bin");
    std::fs::create_dir_all(&bindir).map_err(|e| format!("create bin dir: {e}"))?;
    let dest = cached_path(data_dir);
    let tmp = bindir.join(format!("wireproxy.download.{}", std::process::id()));
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

    let extracted = Command::new("tar")
        .arg("-xzf")
        .arg(&tmp)
        .arg("-C")
        .arg(&bindir)
        .arg("wireproxy")
        .status()
        .await
        .map(|s| s.success())
        .unwrap_or(false);
    let _ = std::fs::remove_file(&tmp);
    if !extracted {
        return Err("failed to extract wireproxy archive".to_string());
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o755));
    }

    // Validate: it must actually run, else discard the partial/corrupt file.
    let runs = Command::new(&dest)
        .arg("--version")
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !runs {
        let _ = std::fs::remove_file(&dest);
        return Err("downloaded wireproxy is not runnable".to_string());
    }
    Ok(dest)
}
