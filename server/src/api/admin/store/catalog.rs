//! Registry catalog handling: fetch + normalize the index (schema 1 and 2),
//! pick the artifact matching this server's build target, and enrich each
//! entry with the installed/update/compatibility state the Store UI shows.
//!
//! Catalog schema 2 (what `scripts/gen-registry.ts` emits):
//! ```json
//! { "schema": 2, "modules": [{
//!     "id": "dev.luma.mdns", "name": "…", "version": "0.1.0",
//!     "description": "…", "library": false, "minServer": "0.1.4",
//!     "dependsOn": { "dev.luma.torrents": "^0.1.0" },
//!     "artifacts": [{ "target": "x86_64-unknown-linux-musl",
//!                     "url": "…", "size": 123, "sha256": "…" }]
//! }] }
//! ```
//! Schema 1 (legacy flat `url`/`size`/`sha256` per module) still parses, with
//! the single artifact treated as platform-independent.

use luma_module_host::HostCtx;
use luma_module_supervisor::Supervisor;
use serde_json::{json, Value};

use crate::state::SharedState;

/// The target triple this server binary was built for (set by `build.rs`).
const BUILD_TARGET: &str = env!("LUMA_BUILD_TARGET");

/// This server's version, checked against a module's `minServer`.
const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// One downloadable `.lmod` build of a module. `target = None` means the
/// bundle is platform-independent (a library module: manifest + FE only).
pub struct Artifact {
    pub target: Option<String>,
    pub url: String,
    pub size: Option<u64>,
    pub sha256: Option<String>,
}

/// A module entry normalized out of a registry catalog (either schema).
pub struct CatalogModule {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub min_server: Option<String>,
    pub library: bool,
    /// Hard dependencies as `(module id, optional semver range)`.
    pub depends_on: Vec<(String, Option<String>)>,
    pub artifacts: Vec<Artifact>,
}

/// Fetch the configured registry's catalog and normalize it.
pub async fn fetch(state: &SharedState, sup: &Supervisor) -> anyhow::Result<Vec<CatalogModule>> {
    let url = {
        let u = state.setting_str("moduleRegistryUrl", super::DEFAULT_REGISTRY);
        if u.trim().is_empty() { super::DEFAULT_REGISTRY.to_string() } else { u }
    };
    let raw = sup.fetch_catalog(&url).await?;
    let modules = raw
        .get("modules")
        .and_then(Value::as_array)
        .map(|mods| mods.iter().filter_map(parse_module).collect())
        .unwrap_or_default();
    Ok(modules)
}

fn parse_module(m: &Value) -> Option<CatalogModule> {
    let id = m.get("id")?.as_str()?.to_string();
    let str_of = |k: &str| m.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
    let artifacts: Vec<Artifact> = match m.get("artifacts").and_then(Value::as_array) {
        Some(list) => list.iter().filter_map(parse_artifact).collect(),
        // Schema 1: one flat url/size/sha256 on the module itself, no target
        // metadata; treated as platform-independent (the legacy behavior).
        None => m
            .get("url")
            .and_then(Value::as_str)
            .map(|url| Artifact {
                target: None,
                url: url.to_string(),
                size: m.get("size").and_then(Value::as_u64),
                sha256: m.get("sha256").and_then(Value::as_str).map(str::to_string),
            })
            .into_iter()
            .collect(),
    };
    let depends_on = m
        .get("dependsOn")
        .and_then(Value::as_object)
        .map(|deps| {
            deps.iter()
                .map(|(dep_id, range)| {
                    let range = range
                        .as_str()
                        .map(str::trim)
                        .filter(|r| !r.is_empty() && *r != "*")
                        .map(str::to_string);
                    (dep_id.clone(), range)
                })
                .collect()
        })
        .unwrap_or_default();
    Some(CatalogModule {
        id,
        name: str_of("name"),
        version: str_of("version"),
        description: str_of("description"),
        min_server: m.get("minServer").and_then(Value::as_str).map(str::to_string),
        library: m.get("library").and_then(Value::as_bool).unwrap_or(false),
        depends_on,
        artifacts,
    })
}

fn parse_artifact(a: &Value) -> Option<Artifact> {
    Some(Artifact {
        target: a.get("target").and_then(Value::as_str).map(str::to_string),
        url: a.get("url")?.as_str()?.to_string(),
        size: a.get("size").and_then(Value::as_u64),
        sha256: a.get("sha256").and_then(Value::as_str).map(str::to_string),
    })
}

/// The artifact to install on THIS server, or `None` when the registry has no
/// build for its platform. Preference order: exact build-target match, then a
/// platform-independent bundle (library modules), then a musl build of the
/// same arch (fully static, so it also runs on the glibc build of the server).
pub fn pick_artifact(m: &CatalogModule) -> Option<&Artifact> {
    pick_for(&m.artifacts, BUILD_TARGET)
}

fn pick_for<'a>(artifacts: &'a [Artifact], host: &str) -> Option<&'a Artifact> {
    if let Some(a) = artifacts.iter().find(|a| a.target.as_deref() == Some(host)) {
        return Some(a);
    }
    if let Some(a) = artifacts.iter().find(|a| a.target.is_none()) {
        return Some(a);
    }
    let musl = host.replace("-gnu", "-musl");
    if musl != host {
        return artifacts.iter().find(|a| a.target.as_deref() == Some(musl.as_str()));
    }
    None
}

/// Server compatibility verdict for one catalog entry: `(compatible, reason)`.
pub fn compat_verdict(m: &CatalogModule) -> (bool, Option<String>) {
    if !luma_module_manifest::server_satisfies(m.min_server.as_deref(), SERVER_VERSION) {
        let needs = m.min_server.as_deref().unwrap_or("?");
        return (false, Some(format!("requires LUMA server {needs} (this server is {SERVER_VERSION})")));
    }
    if pick_artifact(m).is_none() {
        return (false, Some(format!("no build for this server's platform ({BUILD_TARGET})")));
    }
    (true, None)
}

/// The `GET /api/admin/store/catalog` response: every catalog module resolved
/// against this server. Field names stay a superset of the legacy schema-1
/// passthrough (`url`/`size` per module), so an older client keeps working.
pub fn enriched(state: &SharedState, modules: &[CatalogModule]) -> Value {
    let installed: std::collections::HashMap<String, String> =
        luma_module_kernel::manifests(state).into_iter().map(|m| (m.id, m.version)).collect();
    let entries: Vec<Value> = modules
        .iter()
        .map(|m| {
            let artifact = pick_artifact(m);
            let installed_version = installed.get(&m.id);
            let (compatible, reason) = compat_verdict(m);
            let update_available = installed_version
                .is_some_and(|current| luma_module_manifest::is_newer(&m.version, current));
            json!({
                "id": m.id,
                "name": m.name,
                "version": m.version,
                "description": m.description,
                "library": m.library,
                "minServer": m.min_server,
                "dependsOn": m.depends_on.iter()
                    .map(|(dep, range)| json!({ "id": dep, "version": range }))
                    .collect::<Vec<_>>(),
                "target": artifact.and_then(|a| a.target.clone()),
                "url": artifact.map(|a| a.url.clone()),
                "size": artifact.and_then(|a| a.size),
                "sha256": artifact.and_then(|a| a.sha256.clone()),
                "installedVersion": installed_version,
                "updateAvailable": update_available,
                "compatible": compatible,
                "reason": reason,
            })
        })
        .collect();
    json!({
        "schema": 2,
        "serverVersion": SERVER_VERSION,
        "target": BUILD_TARGET,
        "modules": entries,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn artifact(target: Option<&str>) -> Artifact {
        Artifact {
            target: target.map(str::to_string),
            url: format!("https://x/{}.lmod", target.unwrap_or("universal")),
            size: Some(1),
            sha256: Some("00".into()),
        }
    }

    #[test]
    fn pick_prefers_exact_target_then_universal_then_musl() {
        let arts = vec![
            artifact(Some("x86_64-unknown-linux-musl")),
            artifact(Some("aarch64-unknown-linux-musl")),
        ];
        // Exact match wins.
        let picked = pick_for(&arts, "x86_64-unknown-linux-musl").unwrap();
        assert_eq!(picked.target.as_deref(), Some("x86_64-unknown-linux-musl"));
        // A glibc host falls back to the same-arch static musl build.
        let picked = pick_for(&arts, "x86_64-unknown-linux-gnu").unwrap();
        assert_eq!(picked.target.as_deref(), Some("x86_64-unknown-linux-musl"));
        // A platform with no native build gets nothing (a sidecar binary can't run).
        assert!(pick_for(&arts, "aarch64-apple-darwin").is_none());
        // A universal (library) bundle satisfies any host.
        let with_universal = vec![artifact(Some("x86_64-unknown-linux-musl")), artifact(None)];
        let picked = pick_for(&with_universal, "aarch64-apple-darwin").unwrap();
        assert!(picked.target.is_none());
    }

    #[test]
    fn parse_handles_schema_2_and_legacy_schema_1() {
        let v2: Value = serde_json::from_str(
            r#"{ "id": "a.b", "name": "AB", "version": "0.2.0", "minServer": "0.1.4",
                 "library": false,
                 "dependsOn": { "c.d": "^0.1.0", "e.f": "*" },
                 "artifacts": [{ "target": "x86_64-unknown-linux-musl",
                                 "url": "https://x/a.b-x86_64-unknown-linux-musl.lmod",
                                 "size": 5, "sha256": "ab" }] }"#,
        )
        .unwrap();
        let m = parse_module(&v2).unwrap();
        assert_eq!(m.version, "0.2.0");
        assert_eq!(m.min_server.as_deref(), Some("0.1.4"));
        // "*"/blank ranges normalize to None.
        assert_eq!(
            m.depends_on,
            vec![("c.d".to_string(), Some("^0.1.0".to_string())), ("e.f".to_string(), None)]
        );
        assert_eq!(m.artifacts.len(), 1);
        assert_eq!(m.artifacts[0].target.as_deref(), Some("x86_64-unknown-linux-musl"));

        let v1: Value = serde_json::from_str(
            r#"{ "id": "a.b", "name": "AB", "version": "0.1.0",
                 "url": "https://x/a.b.lmod", "size": 5, "sha256": "ab" }"#,
        )
        .unwrap();
        let m = parse_module(&v1).unwrap();
        // Schema 1's single flat artifact is platform-independent.
        assert_eq!(m.artifacts.len(), 1);
        assert!(m.artifacts[0].target.is_none());
        assert_eq!(m.artifacts[0].sha256.as_deref(), Some("ab"));
    }
}
