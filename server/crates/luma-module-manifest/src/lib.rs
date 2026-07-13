//! The LUMA server module contract.
//!
//! A *module* is a self-contained domain (torrent downloading, indexing,
//! transcription) that describes itself and declares what it needs and what it
//! provides. This crate is the shared vocabulary every module and the host
//! agree on:
//!
//! - [`Module`] - the trait a module crate implements.
//! - [`ModuleManifest`] / [`Capability`] - the serde wire shape the server
//!   publishes at `GET /api/modules`, and the frontend `@luma/module-sdk`
//!   registry mirrors, so a module's backend crate and frontend package are
//!   joined by one `id`.
//! - [`Registry`] - gathers modules, resolves the dependency graph
//!   (topological order, missing-dep and cycle detection), and exposes the
//!   manifests + the capability -> provider index.
//! - [`ModuleEvent`] - an open, module-authored event envelope, the loose-
//!   coupling counterpart to direct capability lookups.
//!
//! ## Compile-time today, runtime-loadable later
//!
//! This is deliberately a *compile-time* contract: a module is a crate linked
//! into the binary, and "add a module at runtime" means enabling it via config
//! (the same mental model the `torrent-rqbit` feature + `RQBIT_COMPILED` flag
//! already use). Whether a `Box<dyn Module>` is constructed by a compiled-in
//! crate (now), a WASM component (the only mechanism that hot-loads on the
//! fully-static musl build), or a native dylib (glibc / macOS dev) is a
//! property of *how the box is produced*, not of this trait. The same registry
//! and manifests serve every tier, so the runtime-load path is additive.

mod embedded;
mod event;
mod manifest;
mod registry;

pub use embedded::EmbeddedModule;
/// `embedded_module!()` builds a module's `MODULE` const from its `module.json`
/// + `icon.<ext>`. Re-exported here (as well as from `luma_module_sdk`) so the
/// capability-provider modules that sit below the SDK facade (e.g. scene) can
/// use it without depending on the facade.
pub use luma_module_macros::embedded_module;
pub use event::ModuleEvent;
pub use manifest::{
    Capability, CapabilityReq, ConfigField, Dependency, FeRemote, ModuleManifest, Version,
};
pub use registry::{ModuleRegistration, Registry, ResolveError};

/// A module's packaged icon: an `icon.svg` / `icon.png` sitting next to the
/// module's `module.json`, embedded at build time via `include_bytes!` and
/// served at `GET /api/modules/<id>/icon`.
pub struct ModuleIcon {
    /// MIME type, e.g. "image/svg+xml" or "image/png".
    pub content_type: &'static str,
    /// The image bytes.
    pub bytes: &'static [u8],
}

/// A server module.
///
/// The host gathers every module into a [`Registry`], resolves the graph, then
/// serves the manifests. Implementors return a static self-description from
/// [`manifest`](Module::manifest) and record the capabilities they provide in
/// [`register`](Module::register).
pub trait Module: Send + Sync {
    /// Static self-description: id, version, and declared dependencies.
    ///
    /// The `provides` field is filled in by the registry from
    /// [`register`](Module::register); implementors may leave it empty.
    fn manifest(&self) -> ModuleManifest;

    /// Record the capabilities this module contributes. Called once at startup
    /// with a fresh [`ModuleRegistration`]. The default registers nothing.
    fn register(&self, _reg: &mut ModuleRegistration) {}

    /// The module's packaged icon (`icon.svg` / `icon.png` next to its
    /// `module.json`), embedded at build time. Default: none.
    fn icon(&self) -> Option<ModuleIcon> {
        None
    }
}
