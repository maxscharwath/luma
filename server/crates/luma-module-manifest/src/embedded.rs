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

use crate::{Module, ModuleIcon, ModuleManifest};

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

    // No `register` override: the default no-op keeps the manifest's declared
    // `provides` verbatim (with their `label` / `fields` / `flow` UI metadata).
    // Re-providing them here would flatten each back to a bare `(kind, id)`, since
    // `ModuleRegistration::provide` only records those two (see `Registry::register`).

    fn icon(&self) -> Option<ModuleIcon> {
        self.icon.map(|i| ModuleIcon { content_type: i.content_type, bytes: i.bytes })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Registry;

    /// An `EmbeddedModule`'s rich `provides` (the engine `label` / `fields` / `flow`
    /// UI metadata) must survive registration, so `/api/modules` can drive the
    /// admin's data-driven add-pickers. Regression guard: the old `register()`
    /// override flattened every capability back to a bare `(kind, id)`.
    #[test]
    fn embedded_provides_keep_ui_metadata() {
        const MANIFEST: &str = r#"{
            "id": "dev.luma.engine.example",
            "name": "Example engine",
            "version": "0.1.0",
            "provides": [{
                "kind": "download-client",
                "id": "example",
                "label": "Example",
                "fields": [
                    { "key": "url", "label": "field.url", "type": "string", "required": true },
                    { "key": "password", "label": "field.password", "type": "string", "secret": true }
                ]
            }]
        }"#;
        let mut reg = Registry::new();
        reg.register(Box::new(EmbeddedModule::iconless(MANIFEST)));
        let m = reg.manifests().into_iter().find(|m| m.id == "dev.luma.engine.example").unwrap();
        let cap = &m.provides[0];
        assert_eq!(cap.kind, "download-client");
        assert_eq!(cap.label.as_deref(), Some("Example"));
        assert_eq!(cap.fields.len(), 2);
        assert!(cap.fields[1].secret, "the secret flag must survive registration");
    }
}
