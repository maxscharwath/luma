// KROMA desktop shell (Steam Deck / macOS / Windows). A thin Tauri window hosting the
// shared @kroma/tv frontend (built to ../dist).
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

// Persisted WebKitGTK GPU-rendering opt-in + its crash guard (Deck / Linux).
#[cfg(target_os = "linux")]
mod webview_gpu;

// In-process libmpv (macOS, `libmpv` feature): the chosen native engine, rendering into
// a native NSView behind the webview via `--wid` (verified). See Cargo.toml.
#[cfg(all(target_os = "macos", feature = "libmpv"))]
#[allow(dead_code)]
mod libmpv_mac;

/// WebKitGTK env fixups applied before the webview initialises (Deck / Linux).
/// Each var is only set when the user hasn't already pinned an explicit value.
#[cfg(target_os = "linux")]
fn prepare_linux_env() {
    // WebKitGTK's DMABUF renderer fails on some GPU/driver combos (verified on the
    // Steam Deck: "Could not create default EGL display: EGL_BAD_PARAMETER" - the
    // web process aborts, so the transparent window renders NOTHING and sits
    // invisible-but-focused over the desktop). By default it is disabled before
    // WebKitGTK initializes; compositing stays on, so window transparency (mpv
    // behind the webview) is unaffected. Software rendering is the price, so
    // webview_gpu.rs offers a persisted opt-in back onto the GPU path (profile
    // menu row, with a crash guard that auto-reverts an invisible boot); an
    // explicit WEBKIT_DISABLE_DMABUF_RENDERER or KROMA_WEBKIT_DMABUF=1 (WebKit
    // checks the var's PRESENCE, so exporting "0" cannot re-enable) pins the
    // choice for one session without touching the stored setting.
    webview_gpu::apply_env();
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
    // The stock AppRun in Tauri AppImages exports GST_PLUGIN_SYSTEM_PATH(_1_0)
    // pointing at $APPDIR/usr/lib/gstreamer-1.0 even with bundleMediaFramework
    // off, where that directory is never created. GStreamer treats the var as
    // "search ONLY here", so the system plugins are masked: webview audio dies
    // ("GStreamer element autoaudiosink not found") and the user's
    // ~/.cache/gstreamer-1.0 registry is rebuilt EMPTY, breaking other
    // GStreamer apps until cleared (tauri-apps/tauri#15665). Drop the vars
    // when they point at a missing directory; WebKit's child processes
    // inherit our env, so they fall back to the system plugin search.
    for var in ["GST_PLUGIN_SYSTEM_PATH_1_0", "GST_PLUGIN_SYSTEM_PATH"] {
        if let Some(path) = std::env::var_os(var) {
            let single_path = !path.to_string_lossy().contains(':');
            if single_path && !std::path::Path::new(&path).is_dir() {
                std::env::remove_var(var);
            }
        }
    }
}

// macOS: tell the frontend the mpv engine is available (+ the debug test URL) up
// front, then build the in-process libmpv engine AFTER the window is on-screen +
// laid out. mpv's `--wid` embedding needs a visible, non-zero view or it falls
// back to opening its OWN window - so we can't init in `setup` (the window isn't
// shown yet); defer to the running loop.
#[cfg(all(target_os = "macos", feature = "libmpv"))]
fn init_libmpv_deferred(app: &tauri::AppHandle) {
    use tauri::Manager;
    let handle = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(700));
        let h = handle.clone();
        let _ = handle.run_on_main_thread(move || {
            if let Some(win) = h.webview_windows().values().next() {
                if let Ok(nsw) = win.ns_window() {
                    // Advertise mpv to the frontend ONLY after the engine is up,
                    // so playback started early can't invoke a no-op mpv_load.
                    if libmpv_mac::init(&h, nsw) {
                        let _ = win.eval("window.__KROMA_MPV__ = true;");
                    }
                }
            }
        });
    });
}

/// Quit the whole app from the webview. The shell is a fullscreen window with no
/// chrome (no close button), so the TV UI offers an explicit "quit" menu row;
/// exiting through the event loop also runs the Linux mpv teardown.
#[tauri::command]
fn app_quit(app: tauri::AppHandle) {
    app.exit(0);
}

/// Relaunch the app (used after flipping a boot-time setting like the webview
/// GPU renderer). `restart` re-execs without flowing through `RunEvent::Exit`,
/// so the mpv teardown must run here first (idempotent with the Exit handler).
#[tauri::command]
fn app_relaunch(app: tauri::AppHandle) {
    #[cfg(target_os = "linux")]
    {
        use tauri::Manager;
        if let Some(state) = app.try_state::<mpv::MpvState>() {
            mpv::shutdown(state.inner());
        }
    }
    app.restart();
}

/// Deck: Tauri does not reap child processes; kill the mpv binary on exit.
#[cfg(target_os = "linux")]
fn on_run_event(app: &tauri::AppHandle, event: &tauri::RunEvent) {
    use tauri::Manager;
    if let tauri::RunEvent::Exit = event {
        if let Some(state) = app.try_state::<mpv::MpvState>() {
            mpv::shutdown(state.inner());
        }
    }
}

fn main() {
    #[cfg(target_os = "linux")]
    prepare_linux_env();

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
                mpv::mpv_status,
                webview_gpu::webview_gpu_get,
                webview_gpu::webview_gpu_set,
                webview_gpu::webview_boot_ok,
                app_quit,
                app_relaunch
            ]);
    }
    #[cfg(all(target_os = "macos", feature = "libmpv"))]
    {
        builder = builder
            .manage(libmpv_mac::MpvState::default())
            .invoke_handler(tauri::generate_handler![
                libmpv_mac::mpv_load,
                libmpv_mac::mpv_command,
                libmpv_mac::set_now_playing,
                app_quit,
                app_relaunch
            ]);
    }
    // Remaining shells (macOS without libmpv, Windows) still need the app commands.
    #[cfg(not(any(target_os = "linux", all(target_os = "macos", feature = "libmpv"))))]
    {
        builder = builder.invoke_handler(tauri::generate_handler![app_quit, app_relaunch]);
    }

    // Self-update (all desktop OSes): checks the GitHub Release, verifies the
    // signature against the pinned pubkey, installs, relaunches (driven from JS).
    builder = builder
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build());

    builder
        // Deck: the transparent UI window is always-on-top so the chrome floats over
        // the mpv plane - but a PERMANENT keep-above makes every other window
        // unreachable (alt-tab away showed nothing but KROMA). Track focus instead:
        // keep-above only while KROMA is the active window, so alt-tabbing to Steam
        // or a terminal actually reveals it.
        .on_window_event(|_window, _event| {
            #[cfg(target_os = "linux")]
            if let tauri::WindowEvent::Focused(focused) = _event {
                let _ = _window.set_always_on_top(*focused);
            }
        })
        .setup(|_app| {
            // Deck: launch the mpv binary behind the transparent UI.
            #[cfg(target_os = "linux")]
            {
                use tauri::Manager;
                mpv::spawn(_app.handle().clone());
            }
            // macOS: build the in-process libmpv engine once the window is laid out
            // (deferred; see [`init_libmpv_deferred`]).
            #[cfg(all(target_os = "macos", feature = "libmpv"))]
            {
                use tauri::Manager;
                init_libmpv_deferred(_app.handle());
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building the KROMA desktop app")
        .run(|_app, _event| {
            #[cfg(target_os = "linux")]
            on_run_event(_app, &_event);
        });
}
