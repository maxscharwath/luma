fn main() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS");
    let macos = target_os.as_deref() == Ok("macos");
    let windows = target_os.as_deref() == Ok("windows");
    let libmpv = std::env::var("CARGO_FEATURE_LIBMPV").is_ok();

    if libmpv && windows {
        // libmpv2-sys emits `-lmpv` with NO link-search path (same as macOS below).
        // Point the linker at the dir holding the MSVC import lib `mpv.lib`, which CI
        // generates from the mpv-dev DLL (see scripts/fetch-libmpv-windows.ps1) and
        // exposes via KROMA_MPV_LIB_DIR. libmpv-2.dll ships next to the .exe at runtime.
        // Absent var (e.g. a webview-only build) = no search path added.
        println!("cargo:rerun-if-env-changed=KROMA_MPV_LIB_DIR");
        if let Ok(dir) = std::env::var("KROMA_MPV_LIB_DIR") {
            println!("cargo:rustc-link-search=native={dir}");
        }
    }

    if libmpv && macos {
        // libmpv2-sys pkg-configs headers but doesn't emit a link-search path, so
        // `-lmpv` fails to link. Add Homebrew's lib dir (dev; a shippable build bundles
        // its own libmpv.dylib - a later milestone).
        println!("cargo:rustc-link-search=native=/opt/homebrew/lib");
        println!("cargo:rustc-link-search=native=/usr/local/lib");
        // The Obj-C render-API shim (an NSOpenGLView mpv draws into, behind the webview).
        // Needs the mpv headers (render_gl.h) + AppKit / OpenGL / CoreVideo.
        cc::Build::new()
            .file("src/window_shim.m")
            .include("/opt/homebrew/include")
            .include("/usr/local/include")
            .flag("-Wno-deprecated-declarations") // NSOpenGLView is deprecated but works
            .compile("kroma_window_shim");
        println!("cargo:rustc-link-lib=framework=Cocoa");
        println!("cargo:rustc-link-lib=framework=OpenGL");
        println!("cargo:rustc-link-lib=framework=CoreVideo");
        println!("cargo:rustc-link-lib=framework=MediaPlayer"); // MacBook media keys
        println!("cargo:rerun-if-changed=src/window_shim.m");
    }

    tauri_build::build();
}
