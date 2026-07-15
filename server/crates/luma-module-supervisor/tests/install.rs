//! Integration tests for the supervisor's install path: the `minServer`
//! compatibility gate and the download checksum verifier. Bundles are built
//! in-memory as raw tars (the installer accepts zstd / gzip / raw, dispatched
//! by magic bytes) and use `library: true` manifests so nothing is spawned.

use luma_module_supervisor::{verify_sha256, Supervisor, SupervisorConfig};

fn tar_with_manifest(manifest: &str) -> Vec<u8> {
    let mut builder = tar::Builder::new(Vec::new());
    let bytes = manifest.as_bytes();
    let mut header = tar::Header::new_gnu();
    header.set_size(bytes.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    builder.append_data(&mut header, "module.json", bytes).unwrap();
    builder.into_inner().unwrap()
}

fn temp_modules_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("luma-sup-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn supervisor(dir: &std::path::Path, server_version: &str) -> std::sync::Arc<Supervisor> {
    Supervisor::new(SupervisorConfig {
        modules_dir: dir.to_path_buf(),
        core_url: "http://127.0.0.1:0".into(),
        host_token: "t".into(),
        db_path: dir.join("db.sqlite"),
        data_dir: dir.to_path_buf(),
        reserved_ids: vec!["dev.luma.reserved".into()],
        server_version: server_version.into(),
    })
}

#[test]
fn install_rejects_a_module_needing_a_newer_server() {
    let dir = temp_modules_dir("gate");
    let sup = supervisor(&dir, "0.1.4");
    let bundle = tar_with_manifest(
        r#"{ "id": "com.example.demo", "name": "Demo", "version": "1.0.0",
             "minServer": "999.0.0", "library": true }"#,
    );
    let err = sup.install(&bundle).unwrap_err().to_string();
    assert!(err.contains("requires LUMA server"), "unexpected error: {err}");
    assert!(sup.installed_ids().is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn install_accepts_a_satisfied_min_server() {
    let dir = temp_modules_dir("ok");
    let sup = supervisor(&dir, "0.1.4");
    let bundle = tar_with_manifest(
        r#"{ "id": "com.example.demo", "name": "Demo", "version": "1.0.0",
             "minServer": "0.1.0", "library": true }"#,
    );
    let manifest = sup.install(&bundle).unwrap();
    assert_eq!(manifest["id"], "com.example.demo");
    assert_eq!(sup.installed_ids(), vec!["com.example.demo".to_string()]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn install_still_rejects_reserved_ids() {
    let dir = temp_modules_dir("reserved");
    let sup = supervisor(&dir, "0.1.4");
    let bundle = tar_with_manifest(
        r#"{ "id": "dev.luma.reserved", "name": "Shadow", "version": "1.0.0", "library": true }"#,
    );
    let err = sup.install(&bundle).unwrap_err().to_string();
    assert!(err.contains("built into this server"), "unexpected error: {err}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn checksum_verification_accepts_match_and_rejects_mismatch() {
    // sha256("luma") both cases must pass; a different hash must refuse.
    let bytes = b"luma";
    let good = "53009c20073f1d96f75d46db1f6f25bc9b461cda906accc86792e189986ecb1f";
    assert!(verify_sha256(bytes, good).is_ok());
    assert!(verify_sha256(bytes, &good.to_uppercase()).is_ok());
    let err = verify_sha256(bytes, "deadbeef").unwrap_err().to_string();
    assert!(err.contains("checksum mismatch"), "unexpected error: {err}");
}
