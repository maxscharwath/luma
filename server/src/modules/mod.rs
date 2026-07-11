//! The module registry: the single server-side composition point for every
//! module, of every tier.
//!
//! A module has up to three facets:
//!  - its **manifest** (id / capabilities / icon / dependency graph) -- the
//!    portable `luma_module_sdk::Module`, which every compile-time module exports
//!    as a `MODULE` const;
//!  - optionally its **backend behavior** ([`luma_module_host::ServerModule`]):
//!    the admin routes it serves (behind an enabled-gate) and its async
//!    enable/disable lifecycle. This now lives in each module's OWN crate; the
//!    binary only lists the modules (the composition root) and drives them
//!    generically, so the core is not aware of any specific module;
//!  - or it is **runtime-loaded** (a WASM module in the [`WasmHost`]).
//!
//! [`build`] declares the compile-time modules, asserts every behavior has a
//! manifest, and the public functions merge in the WASM tier. The whole server
//! reads modules through this one API; per-module enabled/config state lives in
//! `luma_engine::modules` (the settings blob).

use std::sync::{Arc, OnceLock};

use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::{from_fn_with_state, Next};
use axum::response::IntoResponse;
use axum::Router;
use luma_module_host::{HostCtx, ServerModule};
use luma_module_sdk::{EmbeddedModule, ModuleManifest, Registry};

use crate::state::SharedState;

/// The compile-time module set: manifests (+ dependency graph) paired with the
/// backend behaviors the ones that have them provide (`ServerModule`s collected
/// from each module's crate).
struct ModuleRegistry {
    manifests: Registry,
    servers: Vec<Box<dyn ServerModule<SharedState>>>,
}

/// Build (once) the compile-time registry: register every module's manifest, then
/// collect the `ServerModule` each crate exports. The assertion catches a behavior
/// whose id drifts from any manifest.
fn build() -> ModuleRegistry {
    let mut manifests = Registry::new();
    // Every compile-time module exports a `MODULE` const (an `EmbeddedModule`
    // built from its module.json + icon.svg); the codegen ones come in via the
    // generated aggregator.
    manifests.register(Box::new(luma_indexer::MODULE));
    manifests.register(Box::new(luma_torrent::MODULE));
    manifests.register(Box::new(luma_torznab::MODULE));
    manifests.register(Box::new(luma_scene::MODULE));
    manifests.register(Box::new(luma_whisper::MODULE));
    manifests.register(Box::new(luma_vector::MODULE));
    manifests.register(Box::new(luma_mdns::MODULE));
    manifests.register(Box::new(luma_vpn::MODULE));
    manifests.register(Box::new(luma_remote::MODULE));
    // Acquisition is a settings-view module (no dedicated routes), so it has a
    // manifest but no ServerModule behavior.
    manifests.register(Box::new(EmbeddedModule::new(
        include_str!("../../modules/dev.luma.acquisition/module.json"),
        include_bytes!("../../modules/dev.luma.acquisition/icon.svg"),
    )));
    // Download-engine sub-modules: backend-only (no page/icon), they toggle a
    // download-client factory kind on the Downloads registry.
    manifests.register(Box::new(EmbeddedModule::iconless(include_str!(
        "../../modules/dev.luma.engine.transmission/module.json"
    ))));
    manifests.register(Box::new(EmbeddedModule::iconless(include_str!(
        "../../modules/dev.luma.engine.qbittorrent/module.json"
    ))));
    luma_modules_generated::register_all(&mut manifests);

    // The backend behavior of each module, collected from its crate. The binary
    // names no module type here beyond calling its `server_module()` constructor
    // (the composition root); every module now owns its own `ServerModule`.
    let servers: Vec<Box<dyn ServerModule<SharedState>>> = vec![
        luma_torrent::server_module(),
        luma_vpn::server_module(),
        luma_indexer::server_module(),
        luma_remote::server_module(),
        luma_transmission::server_module(),
        luma_qbittorrent::server_module(),
    ];

    let ids: Vec<String> = manifests.manifests().into_iter().map(|m| m.id).collect();
    for s in &servers {
        assert!(
            ids.iter().any(|id| id == s.id()),
            "ServerModule {:?} has no matching module manifest",
            s.id(),
        );
    }
    ModuleRegistry { manifests, servers }
}

fn registry() -> &'static ModuleRegistry {
    static REGISTRY: OnceLock<ModuleRegistry> = OnceLock::new();
    REGISTRY.get_or_init(build)
}

/// Compile-time module ids in dependency (initialization) order, or registration
/// order (logged) if the graph fails to resolve.
fn resolved_order() -> Vec<String> {
    let reg = &registry().manifests;
    match reg.resolve() {
        Ok(order) => order,
        Err(err) => {
            tracing::error!(%err, "module graph did not resolve; using registration order");
            reg.manifests().into_iter().map(|m| m.id).collect()
        }
    }
}

/// Compile-time module manifests in dependency (initialization) order. Falls back
/// to registration order (logged) if the graph fails to resolve, so a broken
/// dependency can never take the listing endpoint down.
fn compiled_manifests() -> Vec<ModuleManifest> {
    let all = registry().manifests.manifests();
    resolved_order()
        .iter()
        .filter_map(|id| all.iter().find(|m| &m.id == id).cloned())
        .collect()
}

/// Every module's manifest for the listing endpoints: compile-time (dependency
/// ordered) plus the runtime-loaded (WASM) ones. Used by `/api/modules` and the
/// admin list -- the one merge point across tiers.
pub fn manifests(state: &SharedState) -> Vec<ModuleManifest> {
    let mut all = compiled_manifests();
    if let Ok(host) = state.wasm.read() {
        all.extend(host.manifests());
    }
    all
}

/// A module's packaged icon bytes (compile-time first, then WASM), for
/// `GET /api/modules/<id>/icon`. Owned bytes so both tiers share one shape.
pub fn icon(state: &SharedState, id: &str) -> Option<(&'static str, Vec<u8>)> {
    if let Some(ic) = registry().manifests.icon_of(id) {
        return Some((ic.content_type, ic.bytes.to_vec()));
    }
    state.wasm.read().ok().and_then(|host| host.icon(id)).map(|i| (i.content_type, i.bytes))
}

/// The backend behavior for a module id, if it has any (for the enable/disable
/// lifecycle driven by the admin toggle).
pub fn find_server(id: &str) -> Option<&'static dyn ServerModule<SharedState>> {
    registry().servers.iter().find(|m| m.id() == id).map(|m| m.as_ref())
}

/// At boot, bring every enabled module's live services up (and make sure disabled
/// ones are down), in dependency order, awaiting each so ordering holds (the VPN
/// bridge starts before the engine that tunnels through it). This is the generic
/// mirror of the per-toggle `on_enable`/`on_disable` the admin console runs, so a
/// module's enabled state is durable across a restart instead of the binary
/// hardcoding which module to start.
pub async fn apply_enabled_states(state: &SharedState) {
    let order = resolved_order();
    let host: Arc<dyn HostCtx> = state.clone();
    let mut servers: Vec<&dyn ServerModule<SharedState>> =
        registry().servers.iter().map(|m| m.as_ref()).collect();
    servers.sort_by_key(|m| order.iter().position(|id| id == m.id()).unwrap_or(usize::MAX));
    for module in servers {
        if luma_engine::modules::module_enabled(&state.settings, module.id()) {
            module.on_enable(host.clone()).await;
        } else {
            module.on_disable(host.clone()).await;
        }
    }
}

/// The admin routers of every backend module, each behind its enabled-gate,
/// merged into one router for the `/api/admin` subtree.
pub fn mount_admin(state: SharedState) -> Router<SharedState> {
    let mut router = Router::new();
    for module in &registry().servers {
        // Lifecycle-only modules (the engines) contribute no routes; skip them so
        // we never wrap an empty router in a route_layer (which axum rejects).
        if let Some(routes) = module.admin_routes(&state) {
            router = router.merge(module_scope(state.clone(), module.id(), routes));
        }
    }
    router
}

/// Wrap a router so every request to it 404s while `id` is disabled. Uses
/// `route_layer`, so it only guards the module's own routes (an unrelated 404
/// still falls through normally). Needs the resolved `state` (like the session
/// guard) because the enabled flag is read from the live settings store.
fn module_scope(state: SharedState, id: &'static str, router: Router<SharedState>) -> Router<SharedState> {
    router.route_layer(from_fn_with_state(
        state,
        move |State(state): State<SharedState>, req: Request, next: Next| async move {
            if luma_engine::modules::module_enabled(&state.settings, id) {
                next.run(req).await
            } else {
                StatusCode::NOT_FOUND.into_response()
            }
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_modules_resolve() {
        // Also runs build()'s ServerModule<->manifest consistency assertion.
        let order = registry().manifests.resolve().expect("built-in module graph resolves");
        assert!(order.contains(&"dev.luma.torrents".to_string()));
        assert!(order.contains(&"dev.luma.indexer".to_string()));
        assert!(order.contains(&"dev.luma.hello".to_string()));
    }

    #[test]
    fn compiled_manifests_expose_download_client_kinds() {
        let torrents = compiled_manifests()
            .into_iter()
            .find(|m| m.id == "dev.luma.torrents")
            .expect("torrents module present");
        assert!(torrents.provides.iter().any(|c| c.kind == "download-client" && c.id == "rqbit"));
    }

    #[test]
    fn every_server_module_has_a_matching_manifest() {
        // find_server ids must all be resolvable manifests.
        for s in &registry().servers {
            assert!(
                compiled_manifests().iter().any(|m| m.id == s.id()),
                "no manifest for ServerModule {:?}",
                s.id(),
            );
        }
    }
}
