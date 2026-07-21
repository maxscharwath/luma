// Persisted opt-in for the WebKitGTK DMABUF (GPU) renderer on Linux/Deck.
//
// GPU (DMABUF) rendering is the DEFAULT now (DEFAULT_GPU). main.rs used to
// hard-DISABLE the renderer because the Deck's webview aborted on EGL, but that
// workaround predates fix-appimage.sh removing the poisoned bundled libwayland:
// EGL is healthy again (mpv's gpu-next comes up clean on the Deck) and forcing
// software rendering black-screened the transform-composited app layer under the
// transparent window (nothing painted, the mpv plane showed through). It is a
// massive perf win on a 4K TV besides. So we default to GPU and treat the menu
// row as an opt-OUT to software. This module stores the user's choice (the "GPU
// rendering" row in the TV profile menu) in the app config dir and applies it
// before WebKitGTK initialises; flipping it therefore only takes effect on a
// fresh boot, so the frontend relaunches right after.
//
// Boot guard: a GPU boot arms a probe marker that the frontend disarms once it
// is actually running (`webview_boot_ok`). A marker still present at the next
// launch means the web process never came up (the invisible-window failure):
// the setting auto-reverts to software rendering, so a TV-docked Deck recovers
// by itself, no terminal needed. (A user who force-quits within the first
// second of a healthy GPU boot also reverts - rare, and re-enabling is one
// menu row away.)

use std::path::PathBuf;

/// GPU (DMABUF) rendering is on by default (opt-out via the menu). See the
/// module header for why software rendering was retired as the default. The
/// crash guard in `apply_env` reverts a GPU boot that never reaches the frontend
/// back to software, so a genuinely broken GPU stack still recovers on its own.
const DEFAULT_GPU: bool = true;

/// The app's XDG config dir; mirrors Tauri's `app_config_dir` for our
/// identifier without needing an `AppHandle` (this runs before the app is
/// built).
fn config_dir() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("tv.kroma.desktop"))
}

fn settings_path() -> Option<PathBuf> {
    config_dir().map(|d| d.join("webview.json"))
}

fn probe_path() -> Option<PathBuf> {
    config_dir().map(|d| d.join("webview-gpu.probe"))
}

/// Whether the WebKitGTK GPU (DMABUF) renderer should be used this boot. GPU is
/// the default (DEFAULT_GPU), so only an explicit opt-OUT via the menu
/// (`{"dmabuf": false}`) selects software rendering; an absent / unreadable /
/// malformed settings file reads as GPU-enabled.
fn gpu_enabled() -> bool {
    let Some(path) = settings_path() else { return DEFAULT_GPU };
    let Ok(raw) = std::fs::read_to_string(path) else { return DEFAULT_GPU };
    serde_json::from_str::<serde_json::Value>(&raw)
        .ok()
        .and_then(|v| v.get("dmabuf").and_then(serde_json::Value::as_bool))
        .unwrap_or(DEFAULT_GPU)
}

/// Best-effort persist; a write failure only costs the preference.
fn write_enabled(on: bool) {
    let Some(dir) = config_dir() else { return };
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(dir.join("webview.json"), serde_json::json!({ "dmabuf": on }).to_string());
}

/// Decide the renderer for this boot. Called by `prepare_linux_env` BEFORE any
/// webview/GTK init. An explicit env pin (either var, either direction) always
/// wins and leaves the probe state untouched - that's a manual A/B session.
pub fn apply_env() {
    if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_some()
        || std::env::var_os("KROMA_WEBKIT_DMABUF").is_some()
    {
        return;
    }
    if gpu_enabled() {
        if let Some(probe) = probe_path() {
            if probe.exists() {
                eprintln!(
                    "KROMA: the last GPU-rendering boot never reached the frontend; reverting to software rendering"
                );
                write_enabled(false);
                let _ = std::fs::remove_file(probe);
            } else if std::fs::write(&probe, b"").is_ok() {
                return; // GPU boot armed: leave the DMABUF renderer enabled
            }
            // Probe unwritable: no crash guard possible, stay on the safe path.
        }
    }
    std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
}

// ----- commands invoked by the TV profile menu -------------------------------

/// Current persisted choice, read when the menu row mounts.
#[tauri::command]
pub fn webview_gpu_get() -> bool {
    gpu_enabled()
}

/// Persist a new choice. Applies at the NEXT launch (see module docs); the
/// frontend invokes `app_relaunch` right after.
#[tauri::command]
pub fn webview_gpu_set(enabled: bool) {
    write_enabled(enabled);
}

/// The frontend is alive: this GPU boot worked, disarm the auto-revert.
#[tauri::command]
pub fn webview_boot_ok() {
    if let Some(probe) = probe_path() {
        let _ = std::fs::remove_file(probe);
    }
}
