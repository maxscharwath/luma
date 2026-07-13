//! Module bundle format + its security-sensitive unpacking.
//!
//! A bundle is a tar of `module.json` + optional `module.wasm` + `fe/` +
//! `icon.{svg,png}`, either raw (`.tar`) or gzip-compressed (`.lmod`, from
//! `bun run modules:pack`). Because an admin uploads arbitrary bytes, the unpacker
//! rebuilds every entry path from its `Normal` components only, so `..`,
//! absolute, and drive-prefixed entries cannot escape the install dir; and the
//! module id (which becomes the install directory name) must be a safe name.

use std::io::Read;
use std::path::{Component, Path, PathBuf};

use anyhow::{bail, Context, Result};

pub const MANIFEST_FILE: &str = "module.json";
pub const WASM_FILE: &str = "module.wasm";
/// The staging subdir name, kept out of the loaded set.
pub const STAGING: &str = ".staging";

/// A module id must be a safe directory name (it becomes `<root>/<id>/`).
pub fn validate_id(id: &str) -> Result<()> {
    let ok = !id.is_empty()
        && id.len() <= 128
        && id != "."
        && id != ".."
        && id.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'));
    if !ok {
        bail!("invalid module id {id:?}");
    }
    Ok(())
}

/// Rebuild an archive entry path from its `Normal` components only (dropping
/// `..`, absolute, and drive prefixes) and keep it only if it is an allow-listed
/// bundle file. Returns `None` for anything to skip. The returned path can never
/// contain `..` or a root, so `dest.join(it)` always stays inside `dest` -- this
/// is the path-escape defense, factored out so it is testable without a crafted
/// (and un-craftable: `tar::Builder` rejects `..`) malicious archive.
fn sanitized_entry(raw: &Path) -> Option<PathBuf> {
    let safe: PathBuf = raw
        .components()
        .filter_map(|c| match c {
            Component::Normal(p) => Some(p),
            _ => None,
        })
        .collect();
    if safe.as_os_str().is_empty() {
        return None;
    }
    let rel = safe.to_string_lossy().replace('\\', "/");
    let allowed = matches!(rel.as_ref(), "module.json" | "module.wasm" | "icon.svg" | "icon.png")
        || rel.starts_with("fe/");
    allowed.then_some(safe)
}

/// Unpack a bundle into `dest`, keeping only allow-listed entries and
/// neutralizing any path escape (see [`sanitized_entry`]). Accepts both a raw
/// tar (`.tar`) and a gzip-compressed tar (`.lmod`), detected by the gzip magic.
pub fn unpack_validated(bundle_bytes: &[u8], dest: &Path) -> Result<()> {
    // `.lmod` is a gzip-compressed tar; a plain `.tar` is raw. Detect the gzip
    // magic (1f 8b) so both install through the same path.
    let mut decompressed = Vec::new();
    let tar_bytes: &[u8] = if bundle_bytes.starts_with(&[0x1f, 0x8b]) {
        flate2::read::GzDecoder::new(bundle_bytes)
            .read_to_end(&mut decompressed)
            .context("decompressing gzip (.lmod) bundle")?;
        &decompressed
    } else {
        bundle_bytes
    };
    let mut archive = tar::Archive::new(tar_bytes);
    for entry in archive.entries().context("reading bundle tar")? {
        let mut entry = entry?;
        let raw = entry.path()?.into_owned();
        let Some(safe) = sanitized_entry(&raw) else {
            continue;
        };
        let out = dest.join(&safe);
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent)?;
        }
        entry.unpack(&out).with_context(|| format!("extracting {}", safe.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_id_rejects_unsafe_names() {
        for good in ["dev.luma.hello", "a.b", "mod_1-2.x"] {
            assert!(validate_id(good).is_ok(), "{good} should be valid");
        }
        for bad in ["", ".", "..", "a/b", "a\\b", "a b", "a/../b", &"x".repeat(200)] {
            assert!(validate_id(bad).is_err(), "{bad:?} should be rejected");
        }
    }

    /// Build a tiny in-memory tar with the given (path, contents) entries.
    fn make_tar(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut builder = tar::Builder::new(Vec::new());
        for (path, data) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder.append_data(&mut header, path, *data).unwrap();
        }
        builder.into_inner().unwrap()
    }

    #[test]
    fn unpack_keeps_allowed_entries_and_drops_the_rest() {
        let dir = std::env::temp_dir().join("luma-bundle-allow-test");
        let _ = std::fs::remove_dir_all(&dir);
        let tar = make_tar(&[
            ("module.json", b"{}"),
            ("module.wasm", b"\0asm"),
            ("fe/remoteEntry.js", b"x"),
            ("secret.env", b"nope"), // not allow-listed -> skipped
        ]);
        unpack_validated(&tar, &dir).unwrap();
        assert!(dir.join("module.json").exists());
        assert!(dir.join("fe/remoteEntry.js").exists());
        assert!(!dir.join("secret.env").exists(), "unknown entries must be dropped");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unpack_accepts_gzip_lmod_bundles() {
        let dir = std::env::temp_dir().join("luma-bundle-lmod-test");
        let _ = std::fs::remove_dir_all(&dir);
        let tar = make_tar(&[("module.json", b"{}"), ("module.wasm", b"\0asm")]);
        // gzip the tar, the way `.lmod` ships it.
        let mut gz = Vec::new();
        {
            let mut enc = flate2::write::GzEncoder::new(&mut gz, flate2::Compression::default());
            std::io::Write::write_all(&mut enc, &tar).unwrap();
            enc.finish().unwrap();
        }
        assert_eq!(&gz[..2], &[0x1f, 0x8b], "gzip magic");
        unpack_validated(&gz, &dir).unwrap();
        assert!(dir.join("module.json").exists());
        assert!(dir.join("module.wasm").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sanitized_entry_neutralizes_escapes_and_drops_unknowns() {
        // Allow-listed files pass through unchanged.
        assert_eq!(sanitized_entry(Path::new("module.json")), Some(PathBuf::from("module.json")));
        assert_eq!(
            sanitized_entry(Path::new("fe/remoteEntry.js")),
            Some(PathBuf::from("fe/remoteEntry.js"))
        );
        // Traversal / absolute prefixes are stripped to their Normal tail. The
        // result never contains `..` or a root, so `dest.join(it)` stays inside
        // dest (a `../../module.json` lands at `<dest>/module.json`, not outside).
        assert_eq!(
            sanitized_entry(Path::new("../../module.json")),
            Some(PathBuf::from("module.json"))
        );
        for e in ["../escaped.txt", "/etc/passwd", "secret.env", ".."] {
            assert_eq!(sanitized_entry(Path::new(e)), None, "{e:?} must be dropped");
        }
        // Whatever survives has no `..`/root component.
        for e in ["module.json", "fe/x/y.js", "../../fe/z.css"] {
            if let Some(p) = sanitized_entry(Path::new(e)) {
                assert!(
                    p.components().all(|c| matches!(c, Component::Normal(_))),
                    "{e:?} -> {p:?} still has an unsafe component"
                );
            }
        }
    }
}
