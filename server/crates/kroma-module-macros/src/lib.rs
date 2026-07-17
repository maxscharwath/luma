//! Proc-macros for KROMA modules.
//!
//! [`embedded_module!`] collapses the boilerplate every module's server crate
//! used to write by hand:
//!
//! ```ignore
//! pub const MODULE: EmbeddedModule =
//!     EmbeddedModule::new(include_str!("../../module.json"), include_bytes!("../../icon.svg"));
//! ```
//!
//! into `pub const MODULE: EmbeddedModule = kroma_module_sdk::embedded_module!();`.
//! It finds the module's `module.json` and its `icon.<ext>` next to it by
//! convention (the module root is the parent of the server crate) and expands to
//! the right `EmbeddedModule` constructor, picking the icon MIME from the
//! extension. A module with no `icon.*` becomes `iconless`.

use proc_macro::TokenStream;
use std::path::{Path, PathBuf};

/// Build the `MODULE` const for a module server crate by discovering its
/// `module.json` + `icon.<ext>` at compile time. Takes no arguments.
#[proc_macro]
pub fn embedded_module(_input: TokenStream) -> TokenStream {
    // Cargo sets CARGO_MANIFEST_DIR (of the crate being compiled) in rustc's
    // environment, and the proc-macro runs inside that rustc, so this is the
    // CALLER's crate dir: `<module>/server`. The module root (holding
    // module.json + icon) is its parent.
    let manifest_dir = match std::env::var("CARGO_MANIFEST_DIR") {
        Ok(dir) => dir,
        Err(_) => return compile_error("embedded_module!(): CARGO_MANIFEST_DIR is not set"),
    };
    let module_root = match Path::new(&manifest_dir).parent() {
        Some(p) => p.to_path_buf(),
        None => return compile_error("embedded_module!(): the server crate has no parent dir"),
    };

    let manifest_json = module_root.join("module.json");
    if !manifest_json.exists() {
        return compile_error(&format!(
            "embedded_module!(): no module.json at {}",
            manifest_json.display()
        ));
    }
    let json_path = manifest_json.to_string_lossy();

    // `EmbeddedModule` is emitted unqualified so it resolves against whatever the
    // caller has in scope (`use kroma_module_sdk::EmbeddedModule` for the modules
    // above the facade, `use kroma_module_manifest::EmbeddedModule` for the ones
    // below it like scene). The parsed tokens carry call-site hygiene, so this
    // works in both without the macro hardcoding a crate path.
    let expanded = match find_icon(&module_root) {
        Some((icon_path, mime)) => {
            let icon_path = icon_path.to_string_lossy();
            format!(
                "EmbeddedModule::with_icon(include_str!({json:?}), include_bytes!({icon:?}), {mime:?})"
            , json = json_path, icon = icon_path, mime = mime)
        }
        None => format!("EmbeddedModule::iconless(include_str!({json:?}))", json = json_path),
    };

    // The pieces are all path/MIME string literals we formatted ourselves, so
    // this always parses.
    expanded.parse().expect("embedded_module!(): generated a valid const expression")
}

/// Probe for `icon.<ext>` in the module root, preferring vector then raster, and
/// return the path plus the MIME to serve it as. `None` when there is no icon.
fn find_icon(dir: &Path) -> Option<(PathBuf, &'static str)> {
    const CANDIDATES: &[(&str, &str)] = &[
        ("svg", "image/svg+xml"),
        ("png", "image/png"),
        ("webp", "image/webp"),
        ("jpg", "image/jpeg"),
        ("jpeg", "image/jpeg"),
        ("gif", "image/gif"),
        ("avif", "image/avif"),
        ("ico", "image/x-icon"),
    ];
    for (ext, mime) in CANDIDATES {
        let path = dir.join(format!("icon.{ext}"));
        if path.exists() {
            return Some((path, mime));
        }
    }
    None
}

fn compile_error(message: &str) -> TokenStream {
    format!("compile_error!({message:?})").parse().expect("compile_error! parses")
}
