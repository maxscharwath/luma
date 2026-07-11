//! This module's registry entry: its manifest + packaged icon come from the
//! `module.json` / `icon.svg` at the module root, embedded at compile time.

use luma_module_sdk::EmbeddedModule;

/// Registered into the server's module registry (see `build_registry`).
pub const MODULE: EmbeddedModule =
    EmbeddedModule::new(include_str!("../../module.json"), include_bytes!("../../icon.svg"));
