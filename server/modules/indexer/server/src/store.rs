//! The definition store: fetch the community-maintained Cardigann definition
//! set at runtime and cache it under the data directory.
//!
//! The definitions are GPL and must not be distributed with LUMA (MIT); so
//! nothing is vendored. Instead the end user's server downloads the current set
//! from the upstream repo on demand (a single tarball), extracts the highest
//! schema-version directory, and keeps the `*.yml` files locally. The admin
//! triggers a re-sync to pick up upstream fixes.
//!
//! Transport reuses the system `curl` (via `luma-http`, so a VPN proxy applies)
//! and the system `tar` for extraction - the same "shell out to the OS" stance
//! the rest of LUMA's acquisition transport takes.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context as _, Result};
use serde::{Deserialize, Serialize};

use crate::definition::{self, Definition};

/// Where the upstream set is fetched from. The `master` tarball is one request
/// (versus ~600 for per-file fetches). Overridable so a deployment can pin a
/// fork/mirror.
pub const DEFAULT_SOURCE: &str =
    "https://codeload.github.com/Prowlarr/Indexers/tar.gz/refs/heads/master";

/// A lightweight view of a definition for the admin's browse list (parsed
/// without the full search/login schema).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefinitionMeta {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub links: Vec<String>,
}

/// Outcome of a sync, for the admin toast.
#[derive(Debug, Clone, Serialize)]
pub struct SyncReport {
    pub count: usize,
    pub version: String,
}

/// A local cache of Cardigann definitions.
pub struct DefinitionStore {
    dir: PathBuf,
    source: String,
}

impl DefinitionStore {
    /// Cache lives at `<data_dir>/indexer-defs`.
    pub fn new(data_dir: &Path) -> Self {
        DefinitionStore { dir: data_dir.join("indexer-defs"), source: DEFAULT_SOURCE.to_string() }
    }

    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = source.into();
        self
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Have definitions been fetched yet?
    pub fn is_populated(&self) -> bool {
        std::fs::read_dir(&self.dir)
            .map(|mut d| d.any(|e| e.as_ref().map(is_yml).unwrap_or(false)))
            .unwrap_or(false)
    }

    /// Download + extract the current definition set. Replaces the cache
    /// atomically-ish (extract to a temp dir, then swap the yml files in).
    pub fn sync(&self) -> Result<SyncReport> {
        std::fs::create_dir_all(&self.dir).context("create defs dir")?;
        let tmp = self.dir.join(".sync-tmp");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).context("create sync tmp")?;

        // 1) Download the tarball (curl, so the VPN proxy applies if set).
        let tarball = tmp.join("defs.tar.gz");
        let bytes = luma_http::Fetch::new()
            .max_time(120)
            .get(&self.source)
            .context("download definitions")?
            .ensure_ok()?
            .body;
        std::fs::write(&tarball, &bytes).context("write tarball")?;

        // 2) Extract with the system tar.
        let out = Command::new("tar")
            .arg("-xzf")
            .arg(&tarball)
            .arg("-C")
            .arg(&tmp)
            .output()
            .context("spawn tar")?;
        if !out.status.success() {
            bail!("tar failed: {}", String::from_utf8_lossy(&out.stderr).trim());
        }

        // 3) Find `.../definitions/v<N>` and pick the highest version.
        let defs_root = find_definitions_root(&tmp)
            .context("no definitions/ directory in the downloaded archive")?;
        let version = pick_version_dir(&defs_root)
            .context("no version directory under definitions/")?;
        let src = defs_root.join(&version);

        // 4) Copy the yml files flat into the cache (overwriting), then clean up.
        let mut count = 0;
        for entry in std::fs::read_dir(&src).context("read version dir")? {
            let entry = entry?;
            if is_yml(&entry) {
                let dest = self.dir.join(entry.file_name());
                std::fs::copy(entry.path(), dest)?;
                count += 1;
            }
        }
        let _ = std::fs::remove_dir_all(&tmp);

        if count == 0 {
            bail!("archive contained no definitions");
        }
        Ok(SyncReport { count, version })
    }

    /// List cached definitions (lightweight metadata), sorted by name.
    pub fn list(&self) -> Result<Vec<DefinitionMeta>> {
        let mut out = Vec::new();
        let rd = match std::fs::read_dir(&self.dir) {
            Ok(rd) => rd,
            Err(_) => return Ok(out), // not synced yet
        };
        for entry in rd {
            let entry = entry?;
            if !is_yml(&entry) {
                continue;
            }
            if let Ok(bytes) = std::fs::read(entry.path()) {
                if let Ok(mut meta) = serde_yaml::from_slice::<DefinitionMeta>(&bytes) {
                    // Key on the file stem, not the internal `id`: Jackett /
                    // Prowlarr identify an indexer by filename, and a handful of
                    // definitions carry an internal id that differs from it
                    // (`darkpeers-api.yml` -> `id: darkpeers`). The stem is what
                    // `load` resolves and what a saved indexer stores.
                    if let Some(stem) = entry.path().file_stem().map(|s| s.to_string_lossy().into_owned()) {
                        meta.id = stem;
                        out.push(meta);
                    }
                }
            }
        }
        out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        Ok(out)
    }

    /// Load and fully parse one definition by id.
    pub fn load(&self, id: &str) -> Result<Definition> {
        let path = self.path_for(id);
        let bytes = std::fs::read(&path)
            .with_context(|| format!("definition '{id}' not found (run a definitions sync?)"))?;
        definition::parse(&bytes).with_context(|| format!("parse definition '{id}'"))
    }

    fn path_for(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.yml"))
    }
}

fn is_yml(entry: &std::fs::DirEntry) -> bool {
    entry.path().extension().is_some_and(|e| e == "yml" || e == "yaml")
}

/// Locate the `definitions` directory inside the extracted archive (one level
/// under the `Indexers-master/` top folder).
fn find_definitions_root(tmp: &Path) -> Option<PathBuf> {
    for entry in std::fs::read_dir(tmp).ok()? {
        let entry = entry.ok()?;
        if entry.path().is_dir() {
            let candidate = entry.path().join("definitions");
            if candidate.is_dir() {
                return Some(candidate);
            }
        }
    }
    // Fallback: maybe the archive already IS the definitions dir.
    let direct = tmp.join("definitions");
    direct.is_dir().then_some(direct)
}

/// Pick the highest `v<N>` directory name under `definitions/`.
fn pick_version_dir(defs_root: &Path) -> Option<String> {
    let mut best: Option<(u32, String)> = None;
    for entry in std::fs::read_dir(defs_root).ok()? {
        let entry = entry.ok()?;
        if !entry.path().is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if let Some(n) = name.strip_prefix('v').and_then(|d| d.parse::<u32>().ok()) {
            if best.as_ref().is_none_or(|(bn, _)| n > *bn) {
                best = Some((n, name));
            }
        }
    }
    best.map(|(_, name)| name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_dir_picks_highest() {
        let tmp = std::env::temp_dir().join(format!("luma-defs-test-{}", std::process::id()));
        let defs = tmp.join("definitions");
        for v in ["v1", "v9", "v11", "v10", "notaversion"] {
            std::fs::create_dir_all(defs.join(v)).unwrap();
        }
        assert_eq!(pick_version_dir(&defs).as_deref(), Some("v11"));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// Live end-to-end sync against the real upstream repo. Ignored by default
    /// (network + a few MB); run with `cargo test -p luma-indexer -- --ignored`.
    #[test]
    #[ignore]
    fn real_sync_downloads_and_loads() {
        let dir = std::env::temp_dir().join(format!("luma-defs-live-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let store = DefinitionStore::new(&dir);
        let report = store.sync().expect("sync");
        assert!(report.count > 100, "expected many definitions, got {}", report.count);
        let metas = store.list().unwrap();
        // Nearly all copied files list as definitions (a stray non-definition
        // yaml or two is fine).
        assert!(metas.len() >= report.count - 5, "listed {} of {}", metas.len(), report.count);
        // A well-known public tracker should be loadable + fully parseable.
        let tpb = metas.iter().find(|m| m.id == "thepiratebay").expect("thepiratebay present");
        let def = store.load(&tpb.id).expect("load+parse thepiratebay");
        assert_eq!(def.id, "thepiratebay");
        assert!(!def.search.fields.is_empty());

        // Robustness: how many of the *real* definitions parse fully with our
        // schema? Print the failures so schema gaps are visible.
        let (mut ok, mut fail) = (0u32, 0u32);
        for m in &metas {
            match store.load(&m.id) {
                Ok(_) => ok += 1,
                Err(e) => {
                    fail += 1;
                    if fail <= 25 {
                        eprintln!("[parse-fail] {}: {e:#}", m.id);
                    }
                }
            }
        }
        eprintln!("[schema-coverage] {ok} parsed OK, {fail} failed of {}", metas.len());
        // We should parse the overwhelming majority; allow a small long tail.
        assert!(ok * 100 / metas.len() as u32 >= 90, "only {ok}/{} parsed", metas.len());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn meta_parses_minimal_yaml() {
        let yaml = br#"
id: example
name: Example Tracker
type: public
description: "A test"
links:
  - https://example.org/
"#;
        let meta: DefinitionMeta = serde_yaml::from_slice(yaml).unwrap();
        assert_eq!(meta.id, "example");
        assert_eq!(meta.kind, "public");
        assert_eq!(meta.links, vec!["https://example.org/"]);
    }
}
