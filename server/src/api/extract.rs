//! HTTP request extractors. The `Authorization: Bearer <token>` gate now lives
//! in the host-seam crate (`luma-module-host`), generic over `HostCtx` so module
//! crates share the exact same extractor. Re-exported here so the historical
//! `crate::api::extract::{AuthUser, OptionalAuthUser, bearer_from_headers}` call
//! sites are unchanged.

pub use luma_module_host::{bearer_from_headers, AuthUser, OptionalAuthUser};
