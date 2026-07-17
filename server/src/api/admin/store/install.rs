//! Install-by-id with automatic dependency resolution: resolve a module's hard
//! `dependsOn` closure against what is already present (compiled-in + runtime
//! installed) and the registry catalog, plan missing or out-of-range deps
//! first, then download + checksum-verify + install everything in dependency
//! order. All-or-nothing per module: a failed dep aborts before its dependents
//! are touched.

use std::collections::{HashMap, HashSet};

use anyhow::{anyhow, bail, Result};
use kroma_module_supervisor::Supervisor;
use serde_json::{json, Value};

use super::catalog::{self, CatalogModule};
use crate::state::SharedState;

/// This server's version, checked against each entry's `minServer`.
const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Update every runtime-installed module to the newest COMPATIBLE catalog
/// version. Called at boot (opt-out via the `moduleAutoUpdate` setting) so a
/// server `.spk` update alone keeps the modules current instead of leaving the
/// admin to update each one by hand. Best-effort: a catalog-fetch or per-module
/// install failure is logged and skipped. `install_with_deps` stops the old
/// process, swaps the files, and respawns, so a running module updates in place.
/// Returns `(id, from, to)` for each module actually updated.
pub async fn auto_update(state: &SharedState, sup: &Supervisor) -> Vec<(String, String, String)> {
    let mut updated = Vec::new();
    let modules = match catalog::fetch(sup, &catalog::registry_url(state)).await {
        Ok(m) => m,
        Err(e) => {
            tracing::warn!(error = %format!("{e:#}"), "module auto-update: catalog fetch failed");
            return updated;
        }
    };
    let by_id: HashMap<&str, &CatalogModule> =
        modules.iter().map(|m| (m.id.as_str(), m)).collect();
    for manifest in sup.installed_manifests() {
        let (Some(id), Some(cur)) = (
            manifest.get("id").and_then(Value::as_str),
            manifest.get("version").and_then(Value::as_str),
        ) else {
            continue;
        };
        let Some(entry) = by_id.get(id) else { continue };
        if !kroma_module_manifest::is_newer(&entry.version, cur) {
            continue;
        }
        if !kroma_module_manifest::server_satisfies(entry.min_server.as_deref(), SERVER_VERSION) {
            tracing::info!(
                module = id,
                requires = entry.min_server.as_deref().unwrap_or("?"),
                "module update needs a newer server; skipped (update the server first)"
            );
            continue;
        }
        let to = entry.version.clone();
        match install_with_deps(state, sup, id).await {
            Ok(_) => {
                tracing::info!(module = id, from = cur, to = %to, "auto-updated module");
                updated.push((id.to_string(), cur.to_string(), to));
            }
            Err(e) => {
                tracing::warn!(module = id, error = %format!("{e:#}"), "module auto-update failed")
            }
        }
    }
    updated
}

/// Resolve, download and install `root_id` plus any missing hard dependencies,
/// dependencies first. Returns the report the Store UI shows: everything that
/// was actually installed, deps included.
pub async fn install_with_deps(
    state: &SharedState,
    sup: &Supervisor,
    root_id: &str,
) -> Result<Value> {
    let modules = catalog::fetch(sup, &catalog::registry_url(state)).await?;
    let by_id: HashMap<&str, &CatalogModule> =
        modules.iter().map(|m| (m.id.as_str(), m)).collect();
    // Everything already on this server (compiled-in roster + installed .kmod),
    // with its version: a satisfied dependency is not reinstalled.
    let present: HashMap<String, String> =
        kroma_module_kernel::manifests(state).into_iter().map(|m| (m.id, m.version)).collect();

    let mut plan: Vec<&CatalogModule> = Vec::new();
    let mut planned: HashSet<String> = HashSet::new();
    let mut stack: Vec<String> = Vec::new();
    plan_install(root_id, true, &by_id, &present, &mut plan, &mut planned, &mut stack)?;

    let mut installed = Vec::new();
    for entry in plan {
        let artifact = catalog::pick_artifact(entry)
            .ok_or_else(|| anyhow!("'{}' has no build for this server's platform", entry.id))?;
        let manifest = sup
            .install_from_url(&artifact.url, artifact.sha256.as_deref())
            .await
            .map_err(|e| anyhow!("installing '{}' failed: {e:#}", entry.id))?;
        installed.push(json!({
            "id": manifest.get("id"),
            "name": manifest.get("name"),
            "version": manifest.get("version"),
        }));
    }
    Ok(json!({ "requested": root_id, "installed": installed }))
}

/// Post-order walk of the hard-dependency graph, so dependencies land in the
/// plan before their dependents. A dependency already present at a satisfying
/// version is skipped; one that is missing (or installed outside the declared
/// range) is planned from the catalog. `is_root` bypasses that shortcut so an
/// explicit install/update of an already-installed module still proceeds.
fn plan_install<'a>(
    id: &str,
    is_root: bool,
    by_id: &HashMap<&str, &'a CatalogModule>,
    present: &HashMap<String, String>,
    plan: &mut Vec<&'a CatalogModule>,
    planned: &mut HashSet<String>,
    stack: &mut Vec<String>,
) -> Result<()> {
    if planned.contains(id) {
        return Ok(());
    }
    if stack.iter().any(|s| s == id) {
        bail!("dependency cycle in the registry involving '{id}'");
    }
    let entry = *by_id.get(id).ok_or_else(|| {
        if is_root {
            anyhow!("'{id}' is not in the registry")
        } else {
            anyhow!("dependency '{id}' is neither installed nor in the registry")
        }
    })?;
    // Fail fast with the precise blocker instead of a partial install.
    if !kroma_module_manifest::server_satisfies(entry.min_server.as_deref(), SERVER_VERSION) {
        bail!(
            "'{id}' requires KROMA server {} (this server is {SERVER_VERSION}); update the server first",
            entry.min_server.as_deref().unwrap_or("?"),
        );
    }
    if catalog::pick_artifact(entry).is_none() {
        bail!("'{id}' has no build for this server's platform");
    }
    stack.push(id.to_string());
    for (dep_id, range) in &entry.depends_on {
        let satisfied = present.get(dep_id).is_some_and(|installed| {
            range.as_deref().is_none_or(|r| kroma_module_manifest::range_matches(r, installed))
        });
        if satisfied {
            continue;
        }
        // The catalog's copy must itself satisfy the declared range, or the
        // auto-install would produce a combination the dependent rejects.
        if let (Some(range), Some(dep_entry)) = (range.as_deref(), by_id.get(dep_id.as_str())) {
            if !kroma_module_manifest::range_matches(range, &dep_entry.version) {
                bail!(
                    "'{id}' needs {dep_id}@{range} but the registry has {}",
                    dep_entry.version,
                );
            }
        }
        plan_install(dep_id, false, by_id, present, plan, planned, stack)?;
    }
    stack.pop();
    planned.insert(id.to_string());
    plan.push(entry);
    Ok(())
}
