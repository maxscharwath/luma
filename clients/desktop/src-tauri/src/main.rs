// LUMA desktop shell (Steam Deck / macOS / Windows). A thin Tauri window hosting the
// shared @luma/tv frontend (built to ../dist).
//
//  - Linux (the Deck): drives a native mpv BINARY over a unix-socket IPC (mpv.rs) for
//    VA-API hardware decode, behind a transparent always-on-top window.
//  - macOS: the WKWebView decodes HEVC natively, so it uses the in-page <video> (a
//    single opaque window). The in-process libmpv engine (behind the `libmpv` feature)
//    is a WIP; off by default.
//  - Windows: uses the in-page <video> (WebView2). No mpv module (it is unix-only).
//
// Prevents an extra console window on Windows in release; a no-op on Linux/macOS.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// The mpv IPC runtime uses a unix socket, so it is compiled only on unix (Linux uses
// it; on macOS it is present but unused - spawn is Linux-gated). Never on Windows.
#[cfg(unix)]
#[allow(dead_code)]
mod mpv;

// In-process libmpv (macOS, `libmpv` feature): native VideoToolbox decode behind the
// webview. WIP + off by default; see Cargo.toml.
#[cfg(all(target_os = "macos", feature = "libmpv"))]
#[allow(dead_code)]
mod libmpv_mac;

fn main() {
    #[allow(unused_mut)]
    let mut builder = tauri::Builder::default();

    // The mpv IPC command surface + state exist only on unix (see above).
    #[cfg(unix)]
    {
        builder = builder
            .manage(mpv::MpvState::default())
            .invoke_handler(tauri::generate_handler![mpv::mpv_load, mpv::mpv_command]);
    }

    builder
        .setup(|_app| {
            // Only the LINUX shell (the Deck's VA-API path) runs mpv.
            #[cfg(target_os = "linux")]
            {
                use tauri::Manager;
                mpv::spawn(_app.handle().clone());
            }
            // Stage 2 (macOS, `libmpv` feature): with LUMA_MPV_TEST_URL set, open that
            // stream in-process and decode its audio. Off otherwise.
            #[cfg(all(target_os = "macos", feature = "libmpv"))]
            if let Ok(url) = std::env::var("LUMA_MPV_TEST_URL") {
                libmpv_mac::play_test(url);
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building the LUMA desktop app")
        .run(|_app, _event| {
            // Tauri does not reap child processes; kill mpv on exit (unix only).
            #[cfg(unix)]
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
