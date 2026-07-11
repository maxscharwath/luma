//! The runtime-load tier for LUMA server modules: load WASM modules (extism)
//! into the running server, so a module installed at runtime takes part in the
//! same module system as the compiled-in ones.
//!
//! A WASM module lives in `<data>/modules/<id>/`: a `module.json` manifest, an
//! optional `module.wasm` (extism guest), an optional `fe/` (the Module
//! Federation remote the server serves), and an optional `icon.svg|png`. The
//! guest is sandboxed (extism/wasmtime: no ambient FS or network, no host
//! functions here) and exchanges JSON with the host ([`http`]). It can serve HTTP
//! through a `handle_http` export the host proxies at `/api/plugin/<id>/*`. It is
//! request/response logic -- never a live background service (those stay compiled
//! in). Bundle unpacking + its security live in [`bundle`].

mod bundle;
pub mod http;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, bail, Context, Result};
use extism::convert::Json;
use extism::{Manifest as ExtismManifest, Plugin, Wasm};
use luma_module_sdk::ModuleManifest;

use bundle::{unpack_validated, validate_id, MANIFEST_FILE, STAGING, WASM_FILE};
pub use http::{HttpReq, HttpResp};

/// A module's packaged icon read from its install dir (owned, unlike the
/// compile-time `ModuleIcon` whose bytes are `&'static`).
pub struct WasmIcon {
    pub content_type: &'static str,
    pub bytes: Vec<u8>,
}

/// A WASM-backed module loaded from its install directory.
pub struct WasmModule {
    manifest: ModuleManifest,
    dir: PathBuf,
    /// The extism plugin, when the module ships a `module.wasm`. Behind a Mutex
    /// because `Plugin::call` needs `&mut self`; `None` = frontend-only module.
    plugin: Option<Mutex<Plugin>>,
}

impl WasmModule {
    /// Load a module from `<data>/modules/<id>/`: parse `module.json` (the
    /// authoritative manifest) and instantiate `module.wasm` if present.
    pub fn load(dir: &Path) -> Result<Self> {
        let manifest_path = dir.join(MANIFEST_FILE);
        let raw = std::fs::read(&manifest_path)
            .with_context(|| format!("reading {}", manifest_path.display()))?;
        let manifest: ModuleManifest = serde_json::from_slice(&raw).context("parsing module.json")?;

        let wasm_path = dir.join(WASM_FILE);
        let plugin = if wasm_path.exists() {
            let ext = ExtismManifest::new([Wasm::file(&wasm_path)]);
            let plugin = Plugin::new(&ext, [], true)
                .with_context(|| format!("instantiating {}", wasm_path.display()))?;
            Some(Mutex::new(plugin))
        } else {
            None
        };
        Ok(Self { manifest, dir: dir.to_path_buf(), plugin })
    }

    pub fn manifest(&self) -> &ModuleManifest {
        &self.manifest
    }

    pub fn id(&self) -> &str {
        &self.manifest.id
    }

    /// The module's packaged icon (icon.svg or icon.png in its dir), if any.
    pub fn icon(&self) -> Option<WasmIcon> {
        for (name, content_type) in [("icon.svg", "image/svg+xml"), ("icon.png", "image/png")] {
            if let Ok(bytes) = std::fs::read(self.dir.join(name)) {
                return Some(WasmIcon { content_type, bytes });
            }
        }
        None
    }

    /// Whether this module serves HTTP (ships a wasm backend).
    pub fn serves_http(&self) -> bool {
        self.plugin.is_some()
    }

    /// Proxy an HTTP request to the module's `handle_http` export.
    pub fn handle_http(&self, req: &HttpReq) -> Result<HttpResp> {
        let input = serde_json::to_string(req)?;
        self.with_plugin(|plugin| {
            Ok(plugin
                .call::<&str, Json<HttpResp>>("handle_http", input.as_str())
                .context("calling wasm `handle_http` export")?
                .0)
        })
    }

    /// Invoke a named capability export with JSON in/out (generic dispatch, for
    /// download-client / indexer-engine / etc. capabilities a module provides).
    pub fn call(&self, name: &str, input: &serde_json::Value) -> Result<serde_json::Value> {
        let input = serde_json::to_string(input)?;
        self.with_plugin(|plugin| {
            Ok(plugin
                .call::<&str, Json<serde_json::Value>>(name, input.as_str())
                .with_context(|| format!("calling wasm `{name}` export"))?
                .0)
        })
    }

    fn with_plugin<T>(&self, f: impl FnOnce(&mut Plugin) -> Result<T>) -> Result<T> {
        let plugin = self
            .plugin
            .as_ref()
            .ok_or_else(|| anyhow!("module {:?} has no wasm backend", self.manifest.id))?;
        let mut guard = plugin.lock().expect("wasm plugin mutex poisoned");
        f(&mut guard)
    }
}

/// The host for every runtime-loaded WASM module: loads them from disk at boot,
/// installs / uninstalls at runtime, and exposes their manifests + HTTP proxy to
/// the server. Held on `AppState` behind an `RwLock` so installs mutate live.
pub struct WasmHost {
    /// `<data>/modules` -- one subdir per installed module.
    root: PathBuf,
    modules: Vec<Arc<WasmModule>>,
}

impl WasmHost {
    /// Load every installed module under `root`. Best-effort: a module that fails
    /// to load is logged and skipped so one bad install can't stop the rest.
    pub fn load_all(root: &Path) -> Self {
        std::fs::create_dir_all(root).ok();
        let mut modules = Vec::new();
        if let Ok(entries) = std::fs::read_dir(root) {
            for entry in entries.flatten() {
                let dir = entry.path();
                // Skip the staging dir + any non-directory.
                if !dir.is_dir() || dir.file_name().is_some_and(|n| n == STAGING) {
                    continue;
                }
                match WasmModule::load(&dir) {
                    Ok(m) => {
                        tracing::info!(id = %m.id(), "loaded runtime module");
                        modules.push(Arc::new(m));
                    }
                    Err(e) => tracing::warn!(
                        dir = %dir.display(),
                        error = %format!("{e:#}"),
                        "skipping unloadable runtime module",
                    ),
                }
            }
        }
        Self { root: root.to_path_buf(), modules }
    }

    pub fn manifests(&self) -> Vec<ModuleManifest> {
        self.modules.iter().map(|m| m.manifest().clone()).collect()
    }

    pub fn find(&self, id: &str) -> Option<Arc<WasmModule>> {
        self.modules.iter().find(|m| m.id() == id).cloned()
    }

    pub fn icon(&self, id: &str) -> Option<WasmIcon> {
        self.find(id).and_then(|m| m.icon())
    }

    pub fn handle_http(&self, id: &str, req: &HttpReq) -> Result<HttpResp> {
        self.find(id).ok_or_else(|| anyhow!("no module {id:?}"))?.handle_http(req)
    }

    /// Install a module from an uploaded `.tar` bundle: unpack (validated) into a
    /// staging dir, read its id, load-check it, then atomically swap it into
    /// `<root>/<id>/` (replacing any existing install of that id). Returns the
    /// installed manifest.
    pub fn install(&mut self, tar_bytes: &[u8]) -> Result<ModuleManifest> {
        let staging = self.root.join(STAGING);
        let _ = std::fs::remove_dir_all(&staging);
        std::fs::create_dir_all(&staging)?;
        let result = self.install_from_staging(tar_bytes, &staging);
        if result.is_err() {
            let _ = std::fs::remove_dir_all(&staging);
        }
        result
    }

    fn install_from_staging(&mut self, tar_bytes: &[u8], staging: &Path) -> Result<ModuleManifest> {
        unpack_validated(tar_bytes, staging)?;
        let manifest: ModuleManifest = serde_json::from_slice(
            &std::fs::read(staging.join(MANIFEST_FILE)).context("bundle is missing module.json")?,
        )
        .context("bundle module.json is invalid")?;
        validate_id(&manifest.id)?;
        // Load-check from staging before committing, so a broken bundle can't
        // clobber a working install.
        WasmModule::load(staging).context("bundle failed to load")?;

        let dest = self.root.join(&manifest.id);
        self.modules.retain(|m| m.id() != manifest.id);
        let _ = std::fs::remove_dir_all(&dest);
        std::fs::rename(staging, &dest).context("moving module into place")?;
        let module = WasmModule::load(&dest)?;
        let manifest = module.manifest().clone();
        self.modules.push(Arc::new(module));
        tracing::info!(id = %manifest.id, "installed runtime module");
        Ok(manifest)
    }

    /// Uninstall a runtime module: drop it and delete its install dir.
    pub fn uninstall(&mut self, id: &str) -> Result<()> {
        validate_id(id)?;
        let before = self.modules.len();
        self.modules.retain(|m| m.id() != id);
        if self.modules.len() == before {
            bail!("module {id:?} is not installed");
        }
        let dir = self.root.join(id);
        std::fs::remove_dir_all(&dir).with_context(|| format!("removing {}", dir.display()))?;
        tracing::info!(id, "uninstalled runtime module");
        Ok(())
    }
}
