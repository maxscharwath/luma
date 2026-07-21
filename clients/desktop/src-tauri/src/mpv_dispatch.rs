// Linux playback dispatcher.
//
// The frontend `MpvEngine` calls the same three Tauri commands on Linux
// (`mpv_load` / `mpv_command` / `mpv_status`) and listens for the same `mpv://…`
// events, regardless of which native backend actually plays the video. This module
// owns those command names and routes each call to EITHER:
//   * the in-process libmpv engine (libmpv_linux), when it came up, or
//   * the mpv BINARY over IPC (mpv.rs), the proven default.
//
// In-process libmpv is tried first ONLY when the user opts in with
// `KROMA_LINUX_LIBMPV=1` (it is unvalidated on real Deck hardware, and the binary's
// process isolation + VO fallback ladder is what makes the Deck robust). Without the
// opt-in, or if the in-process engine fails to initialise, the binary is used. Both
// backends emit identical events, so the frontend never knows the difference.
//
// Compiled on Linux in BOTH feature states: with `--no-default-features` (no libmpv)
// the in-process branches vanish and every call routes straight to the binary.

use serde_json::Value;
use tauri::{AppHandle, State};

use crate::mpv;

/// In-process engine backend selection. Only exists in a libmpv build; a no-libmpv
/// build always routes to the mpv binary and needs none of this state.
#[cfg(feature = "libmpv")]
mod inproc {
    use std::sync::atomic::{AtomicBool, Ordering};

    /// Set once, at setup, after the in-process engine initialises. Read on every
    /// command to pick the backend; never flips back (a mid-session engine swap
    /// would strand the player), so a `SeqCst` bool is enough.
    static ACTIVE: AtomicBool = AtomicBool::new(false);

    pub fn mark_active() {
        ACTIVE.store(true, Ordering::SeqCst);
    }
    pub fn is_active() -> bool {
        ACTIVE.load(Ordering::SeqCst)
    }
}

/// Whether the user opted into the in-process libmpv engine (`KROMA_LINUX_LIBMPV=1`).
/// Default false: the binary is the proven path until in-process is Deck-validated.
#[cfg(feature = "libmpv")]
pub fn opt_in() -> bool {
    std::env::var("KROMA_LINUX_LIBMPV")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// Record that the in-process engine came up, so commands route to it.
#[cfg(feature = "libmpv")]
pub fn mark_inproc_active() {
    inproc::mark_active();
}

/// Load a URL (resume at `start` seconds when > 0). Routed to whichever backend is live.
#[tauri::command]
pub fn mpv_load(app: AppHandle, url: String, start: f64) -> Result<(), String> {
    #[cfg(feature = "libmpv")]
    if inproc::is_active() {
        use tauri::Manager;
        if let Some(st) = app.try_state::<crate::libmpv_linux::InprocState>() {
            crate::libmpv_linux::load(&st, &url, start);
            return Ok(());
        }
    }
    mpv::binary_load(&app, url, start)
}

/// Send a raw mpv command array (`set_property` / `seek` / `stop` / …).
#[tauri::command]
pub fn mpv_command(app: AppHandle, args: Vec<Value>) -> Result<(), String> {
    #[cfg(feature = "libmpv")]
    if inproc::is_active() {
        use tauri::Manager;
        if let Some(st) = app.try_state::<crate::libmpv_linux::InprocState>() {
            crate::libmpv_linux::command(&st, &args);
            return Ok(());
        }
    }
    mpv::binary_command(&app, args)
}

/// Liveness probe: `running` / `starting` / `dead`. The in-process engine is a
/// simple up/down (no async socket handshake), so it only reports running or dead.
#[tauri::command]
pub fn mpv_status(app: AppHandle, binary: State<'_, mpv::MpvState>) -> String {
    #[cfg(feature = "libmpv")]
    if inproc::is_active() {
        use tauri::Manager;
        let up = app
            .try_state::<crate::libmpv_linux::InprocState>()
            .map(|s| s.is_active())
            .unwrap_or(false);
        return if up { "running".into() } else { "dead".into() };
    }
    // `app` is only read on the libmpv path; the binary probe uses the managed state.
    let _ = &app;
    mpv::binary_status(&binary)
}
