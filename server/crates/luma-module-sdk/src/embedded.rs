//! A ready-made [`Module`] backed by a module's embedded `module.json` + icon.
//!
//! Every compile-time module used to hand-copy the same ~15-line `impl Module`
//! (parse `module.json`, re-export its `provides`, serve the icon). This collapses
//! that to one line at the module crate:
//!
//! ```ignore
//! pub const MODULE: EmbeddedModule =
//!     EmbeddedModule::new(include_str!("../../module.json"), include_bytes!("../../icon.svg"));
//! ```
//!
//! `include_str!`/`include_bytes!` stay at the module crate so their paths resolve
//! there (a cross-crate `macro_rules!` would resolve them against this crate).

use crate::{Module, ModuleIcon, ModuleManifest, ModuleRegistration};

#[derive(Clone, Copy)]
struct EmbeddedIcon {
    content_type: &'static str,
    bytes: &'static [u8],
}

/// A module whose manifest and icon are embedded at compile time.
#[derive(Clone, Copy)]
pub struct EmbeddedModule {
    manifest_json: &'static str,
    icon: Option<EmbeddedIcon>,
}

impl EmbeddedModule {
    /// A module with an embedded SVG icon (the common case).
    pub const fn new(manifest_json: &'static str, icon_svg: &'static [u8]) -> Self {
        Self {
            manifest_json,
            icon: Some(EmbeddedIcon { content_type: "image/svg+xml", bytes: icon_svg }),
        }
    }

    /// A module with an embedded PNG icon.
    pub const fn with_png(manifest_json: &'static str, icon_png: &'static [u8]) -> Self {
        Self {
            manifest_json,
            icon: Some(EmbeddedIcon { content_type: "image/png", bytes: icon_png }),
        }
    }

    /// A module with no packaged icon.
    pub const fn iconless(manifest_json: &'static str) -> Self {
        Self { manifest_json, icon: None }
    }
}

impl Module for EmbeddedModule {
    fn manifest(&self) -> ModuleManifest {
        serde_json::from_str(self.manifest_json).expect("valid embedded module.json")
    }

    fn register(&self, reg: &mut ModuleRegistration) {
        for cap in self.manifest().provides {
            reg.provide(cap.kind, cap.id);
        }
    }

    fn icon(&self) -> Option<ModuleIcon> {
        self.icon.map(|i| ModuleIcon { content_type: i.content_type, bytes: i.bytes })
    }
}
