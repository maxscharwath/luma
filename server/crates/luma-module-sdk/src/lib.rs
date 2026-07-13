//! The LUMA module SDK: the ONE crate a server module depends on.
//!
//! A module must not depend on `luma-engine`, `luma-db`, `luma-domain`,
//! `luma-http`, etc. directly. This facade re-exports the manifest layer at the
//! crate root (`EmbeddedModule`, `ModuleManifest`, `Registry`, ...) and mirrors
//! the host / engine / domain / http / db / primitives / ports surface under
//! submodules, so a module writes `luma_module_sdk::engine::state::SharedState`
//! instead of reaching into the core crate. Cross-module capabilities go through
//! `luma_module_sdk::ports` (runtime-resolved traits), never a direct dependency
//! on another module's crate.

// Manifest layer (below engine): EmbeddedModule / Module / ModuleManifest /
// Registry / capability + config types. Re-exported at the crate root.
pub use luma_module_manifest::*;

/// Host contract: the `ServerModule` trait, `HostCtx`, `service` / `resolve_port`
/// helpers, and the `async_trait` re-export module impls need.
pub mod host {
    pub use luma_module_host::*;
}

/// Cross-module capability ports (runtime-resolved traits), e.g. `VpnProxyPort`,
/// `TorrentFetchPort`. Depend on these instead of another module's crate.
pub mod ports {
    pub use luma_contracts::*;
}

/// The application surface: `state::SharedState`, `services::*`, `model::*`.
pub mod engine {
    pub use luma_engine::*;
}

/// Domain types: permissions and the shared DTOs.
pub mod domain {
    pub use luma_domain::*;
}

/// The outbound HTTP client (`Fetch`, `Response`).
pub mod http {
    pub use luma_http::*;
}

/// Direct SQLite access via the shared pool.
pub mod db {
    pub use luma_db::*;
}

/// Small shared primitives (`now_ms`, ...).
pub mod primitives {
    pub use luma_primitives::*;
}
