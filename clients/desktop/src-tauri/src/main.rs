// LUMA desktop shell (Steam Deck / macOS / Windows). A thin Tauri window hosting the
// shared @luma/tv frontend (built to ../dist).
//
//  - Linux (the Deck): drives a native mpv BINARY over a unix-socket IPC (mpv.rs) for
//    VA-API hardware decode, behind a transparent always-on-top window.
//  - macOS (`libmpv` feature): in-process libmpv renders into a native NSView behind
//    the transparent webview (via --wid) - decodes AV1 + everything the WKWebView
//    can't. Same MpvEngine protocol as the Deck. WIP; off by default, so a default
//    macOS build still uses the in-page <video>.
//  - Windows: uses the in-page <video> (WebView2). (libmpv/HWND path is a later step.)
//
// Prevents an extra console window on Windows in release; a no-op on Linux/macOS.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// The mpv BINARY IPC runtime (Deck): unix socket, Linux only.
#[cfg(target_os = "linux")]
#[allow(dead_code)]
mod mpv;

// In-process libmpv (macOS, `libmpv` feature): the chosen native engine, rendering into
// a native NSView behind the webview via `--wid` (verified). See Cargo.toml.
#[cfg(all(target_os = "macos", feature = "libmpv"))]
#[allow(dead_code)]
mod libmpv_mac;

fn main() {
    // WebKitGTK's DMABUF renderer fails on some GPU/driver combos (verified on the
    // Steam Deck: "Could not create default EGL display: EGL_BAD_PARAMETER" - the
    // web process aborts, so the transparent window renders NOTHING and sits
    // invisible-but-focused over the desktop). Disable it before WebKitGTK
    // initializes. Compositing stays on, so window transparency (mpv behind the
    // webview) is unaffected. An explicit user value is respected.
    #[cfg(target_os = "linux")]
    {
        if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
            std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        }
        // Disabling DMABUF is not enough on some Wayland stacks (verified on the
        // Steam Deck): WebKitGTK's *native Wayland* backend still can't create an
        // EGL display ("Could not create default EGL display: EGL_BAD_PARAMETER"),
        // the web process aborts, and the transparent window shows NOTHING. Pin GTK
        // to X11 so the webview runs over XWayland instead, whose GLX/EGL path is the
        // battle-tested one on the Deck (gamescope in Game Mode, KDE in Desktop mode
        // both provide XWayland). Compositing stays on, so window transparency (mpv
        // behind the webview) is unaffected - unlike WEBKIT_DISABLE_COMPOSITING_MODE.
        // This is the webview analog of mpv.rs's "GLX via X11, no EGL" ladder rung.
        // An explicit GDK_BACKEND (e.g. a user pinning wayland) is respected.
        if std::env::var_os("GDK_BACKEND").is_none() {
            std::env::set_var("GDK_BACKEND", "x11");
        }
    }

    #[allow(unused_mut)]
    let mut builder = tauri::Builder::default();

    // Native-engine command surface (same `mpv_load`/`mpv_command` names on both). The
    // two cfgs are mutually exclusive, so exactly one `invoke_handler` compiles.
    #[cfg(target_os = "linux")]
    {
        builder = builder
            .manage(mpv::MpvState::default())
            .invoke_handler(tauri::generate_handler![
                mpv::mpv_load,
                mpv::mpv_command,
                mpv::mpv_status
            ]);
    }
    #[cfg(all(target_os = "macos", feature = "libmpv"))]
    {
        builder = builder
            .manage(libmpv_mac::MpvState::default())
            .invoke_handler(tauri::generate_handler![
                libmpv_mac::mpv_load,
                libmpv_mac::mpv_command,
                libmpv_mac::set_now_playing
            ]);
    }

    // Self-update (all desktop OSes): checks the GitHub Release, verifies the
    // signature against the pinned pubkey, installs, relaunches (driven from JS).
    builder = builder
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build());

    builder
        .setup(|_app| {
            // Deck: launch the mpv binary behind the transparent UI.
            #[cfg(target_os = "linux")]
            {
                use tauri::Manager;
                mpv::spawn(_app.handle().clone());
            }
            // macOS: tell the frontend the mpv engine is available (+ the debug test
            // URL) up front, then build the in-process libmpv engine AFTER the window
            // is on-screen + laid out. mpv's `--wid` embedding needs a visible, non-zero
            // view or it falls back to opening its OWN window - so we can't init in
            // `setup` (the window isn't shown yet); defer to the running loop.
            #[cfg(all(target_os = "macos", feature = "libmpv"))]
            {
                use tauri::Manager;
                let handle = _app.handle().clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(700));
                    let h = handle.clone();
                    let _ = handle.run_on_main_thread(move || {
                        if let Some(win) = h.webview_windows().values().next() {
                            if let Ok(nsw) = win.ns_window() {
                                // Advertise mpv to the frontend ONLY after the engine is up,
                                // so playback started early can't invoke a no-op mpv_load.
                                if libmpv_mac::init(&h, nsw) {
                                    let _ = win.eval("window.__LUMA_MPV__ = true;");
                                }
                            }
                        }
                    });
                });
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building the LUMA desktop app")
        .run(|_app, _event| {
            // Tauri does not reap child processes; kill the mpv binary on exit (Deck).
            #[cfg(target_os = "linux")]
            {
                use tauri::Manager;
                if let tauri::RunEvent::Exit = _event {
                    if let Some(state) = _app.try_state::<mpv::MpvState>() {
                        mpv::shutdown(state.inner());
                    }
                }
            }
        });
}
