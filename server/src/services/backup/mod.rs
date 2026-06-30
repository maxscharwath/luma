//! Portable backup orchestration: the DB row export/import ([`crate::db`]'s
//! `backup` module) + the user-uploaded files those rows reference (avatars),
//! packed into a ZIP container ([`archive`]) with an optional password-encryption
//! envelope ([`crypto`]).
//!
//! The DB layer can't read the filesystem (layering: it never sees `data_dir`),
//! so asset bundling + archiving live here, where `services` may use `db`+`infra`.

mod archive;
mod crypto;

use std::path::Path;

use anyhow::Result;
use serde_json::Value;

use crate::db::{self, BackupDoc, Pool};
use crate::infra::image::{images_dir, PUBLIC_PREFIX};

use archive::Assets;

/// Why an import couldn't proceed mapped to localized HTTP errors by the API.
#[derive(Debug)]
pub enum ImportError {
    /// The file is an encrypted backup but no password was supplied.
    PasswordRequired,
    /// Wrong password or corrupted ciphertext (the AEAD tag failed).
    WrongPassword,
    /// Not a recognizable backup (bad zip / json / envelope).
    Invalid(anyhow::Error),
    /// A database failure during the restore itself.
    Db(anyhow::Error),
}

/// Export the portable backup to a `.luma` file's bytes. With a non-empty
/// `password` the ZIP is wrapped in an encrypted envelope; otherwise the bytes
/// are the (compressed) ZIP itself. Either way the import auto-detects.
pub fn export(pool: &Pool, data_dir: &Path, password: Option<&str>) -> Result<Vec<u8>> {
    let doc = db::export_portable(pool)?;
    let assets = gather_assets(&doc, data_dir);
    let zip = archive::write_zip(&doc, &assets)?;
    match password.filter(|p| !p.is_empty()) {
        Some(pw) => crypto::seal(&zip, pw),
        None => Ok(zip),
    }
}

/// Restore a backup from bytes (encrypted envelope, ZIP, or legacy v1 JSON):
/// write avatar files back into the image cache, then the DB rows. `reset` wipes
/// the portable tables first (atomic). Returns per-table row counts.
pub fn import(
    pool: &Pool,
    data_dir: &Path,
    bytes: &[u8],
    password: Option<&str>,
    reset: bool,
) -> std::result::Result<Vec<(String, usize)>, ImportError> {
    let (doc, assets) = decode(bytes, password)?;
    write_assets(data_dir, &assets);
    db::import_portable(pool, &doc, reset).map_err(ImportError::Db)
}

/// Detect the container, decrypt if needed, and parse to `(doc, assets)`.
fn decode(bytes: &[u8], password: Option<&str>) -> std::result::Result<(BackupDoc, Assets), ImportError> {
    if crypto::is_encrypted(bytes) {
        let Some(pw) = password.filter(|p| !p.is_empty()) else {
            return Err(ImportError::PasswordRequired);
        };
        let zip = match crypto::open(bytes, pw) {
            Ok(Some(z)) => z,
            Ok(None) => return Err(ImportError::WrongPassword),
            Err(e) => return Err(ImportError::Invalid(e)),
        };
        return archive::read_zip(&zip).map_err(ImportError::Invalid);
    }
    if bytes.starts_with(b"PK\x03\x04") {
        return archive::read_zip(bytes).map_err(ImportError::Invalid);
    }
    if bytes.iter().copied().find(|b| !b.is_ascii_whitespace()) == Some(b'{') {
        return archive::read_legacy_json(bytes).map_err(ImportError::Invalid);
    }
    Err(ImportError::Invalid(anyhow::anyhow!("unrecognized backup format")))
}

/// Collect the avatar image files the `users` rows reference (gradient avatars
/// have no file and need nothing).
fn gather_assets(doc: &BackupDoc, data_dir: &Path) -> Assets {
    let dir = images_dir(data_dir);
    let mut out = Assets::new();
    let mut seen = std::collections::HashSet::new();
    for user in doc.tables.get("users").into_iter().flatten() {
        let Some(name) = user.get("avatar_url").and_then(Value::as_str).and_then(local_image_name)
        else {
            continue;
        };
        if seen.insert(name.to_string()) {
            if let Ok(bytes) = std::fs::read(dir.join(name)) {
                out.push((name.to_string(), bytes));
            }
        }
    }
    out
}

/// Write restored asset files into the image cache (skipping any already present).
fn write_assets(data_dir: &Path, assets: &Assets) {
    let dir = images_dir(data_dir);
    std::fs::create_dir_all(&dir).ok();
    for (name, bytes) in assets {
        if !is_safe_name(name) {
            continue; // never let a backup write outside the cache dir
        }
        let path = dir.join(name);
        if !path.exists() {
            let _ = std::fs::write(&path, bytes);
        }
    }
}

/// The bare `<hash>.<ext>` filename for an avatar URL that points at our own image
/// cache (`/api/images/<name>`), or `None` for gradient/remote avatars.
fn local_image_name(url: &str) -> Option<&str> {
    url.strip_prefix(PUBLIC_PREFIX).filter(|n| is_safe_name(n))
}

/// A safe cache filename: a bare name, no separators or `..` traversal.
fn is_safe_name(name: &str) -> bool {
    !name.is_empty() && !name.contains('/') && !name.contains('\\') && !name.contains("..")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    static SEQ: AtomicU32 = AtomicU32::new(0);

    fn fresh(tag: &str) -> (Pool, std::path::PathBuf) {
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let data = std::env::temp_dir().join(format!("luma-bksvc-{tag}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data);
        std::fs::create_dir_all(images_dir(&data)).unwrap();
        let pool = crate::db::init(&data.join("luma.db")).unwrap();
        (pool, data)
    }

    fn seed_user_with_avatar(pool: &Pool, data_dir: &Path) {
        std::fs::write(images_dir(data_dir).join("av99.webp"), b"AVATAR").unwrap();
        pool.get()
            .unwrap()
            .execute(
                "INSERT INTO users (id,email,username,password_hash,avatar_url,created_at) \
                 VALUES ('u1','a@b.c','Al','ph','/api/images/av99.webp','t')",
                [],
            )
            .unwrap();
    }

    fn user_count(pool: &Pool) -> i64 {
        pool.get().unwrap().query_row("SELECT COUNT(*) FROM users", [], |r| r.get(0)).unwrap()
    }

    #[test]
    fn zip_round_trip_restores_rows_and_avatar() {
        let (src, src_dir) = fresh("src");
        seed_user_with_avatar(&src, &src_dir);
        let bytes = export(&src, &src_dir, None).unwrap();
        assert!(bytes.starts_with(b"PK\x03\x04"), "unencrypted .luma is a zip");

        let (dst, dst_dir) = fresh("dst");
        import(&dst, &dst_dir, &bytes, None, false).unwrap();
        assert_eq!(user_count(&dst), 1);
        assert_eq!(std::fs::read(images_dir(&dst_dir).join("av99.webp")).unwrap(), b"AVATAR");
    }

    #[test]
    fn encrypted_round_trip_and_password_errors() {
        let (src, src_dir) = fresh("esrc");
        seed_user_with_avatar(&src, &src_dir);
        let sealed = export(&src, &src_dir, Some("hunter2")).unwrap();
        assert!(crypto::is_encrypted(&sealed));

        let (dst, dst_dir) = fresh("edst");
        // No password on an encrypted file → PasswordRequired.
        assert!(matches!(import(&dst, &dst_dir, &sealed, None, false), Err(ImportError::PasswordRequired)));
        // Wrong password → WrongPassword.
        assert!(matches!(import(&dst, &dst_dir, &sealed, Some("nope"), false), Err(ImportError::WrongPassword)));
        // Right password → restored.
        import(&dst, &dst_dir, &sealed, Some("hunter2"), false).unwrap();
        assert_eq!(user_count(&dst), 1);
    }

    #[test]
    fn reset_wipes_pre_existing_rows() {
        let (src, src_dir) = fresh("rsrc");
        seed_user_with_avatar(&src, &src_dir);
        let bytes = export(&src, &src_dir, None).unwrap();

        let (dst, dst_dir) = fresh("rdst");
        // A pre-existing account on the target that's NOT in the backup.
        dst.get().unwrap().execute(
            "INSERT INTO users (id,email,username,password_hash,created_at) VALUES ('keep','k@b.c','K','ph','t')", []).unwrap();

        // Merge keeps it (2 users); reset wipes it first (1 user).
        import(&dst, &dst_dir, &bytes, None, false).unwrap();
        assert_eq!(user_count(&dst), 2);
        import(&dst, &dst_dir, &bytes, None, true).unwrap();
        assert_eq!(user_count(&dst), 1);
    }
}
