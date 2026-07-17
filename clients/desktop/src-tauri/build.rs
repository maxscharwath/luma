fn main() {
    let macos = std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos");
    let libmpv = std::env::var("CARGO_FEATURE_LIBMPV").is_ok();

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
