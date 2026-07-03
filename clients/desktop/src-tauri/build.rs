fn main() {
    // With the `libmpv` feature, libmpv2-sys resolves headers via pkg-config but
    // doesn't emit a link-search path, so `-lmpv` fails to link. Add Homebrew's lib
    // dir on macOS. (No-op for default builds, which don't link libmpv.)
    if std::env::var("CARGO_FEATURE_LIBMPV").is_ok()
        && std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos")
    {
        println!("cargo:rustc-link-search=native=/opt/homebrew/lib");
        println!("cargo:rustc-link-search=native=/usr/local/lib");
    }
    tauri_build::build();
}
