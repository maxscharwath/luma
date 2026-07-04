// In-process libmpv engine for macOS.
//
// Mirrors mpv.rs's Tauri surface (`mpv_load` / `mpv_command` + `mpv://…` events) so the
// frontend `MpvEngine` drives it UNCHANGED. Compositing model: mpv renders into its OWN
// borderless window (embedding into our NSViews via `--wid` proved unreliable on macOS -
// it only attaches to a standalone key window, not a subview/child), and on the first
// load we pin that window BEHIND the transparent LUMA window as a child, so it moves +
// composites with it while the React player chrome sits on top - the same "video plane
// behind the page" model as the Deck / Tizen.
//
// libmpv is thread-safe (`Mpv: Send + Sync`): commands run on invoke threads, a pump
// thread drains `wait_event`. track-list has no node variant so it's built on file-load.

use std::ffi::{c_char, c_void, CStr, CString};
use std::sync::{Arc, Mutex};
use std::thread;

use libmpv2::events::{Event, PropertyData};
use libmpv2::{Format, Mpv, MpvStr};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager, State};

extern "C" {
    /// Create the GL view behind the webview + the mpv render context bound to it, and
    /// make the app window + webview see-through. Returns 0 on success. MUST run on the
    /// main thread. `mpv_handle` is the raw `mpv_handle*`.
    fn luma_mpv_render_setup(nswindow: *mut c_void, mpv_handle: *mut c_void) -> i32;
    /// Blank the GL view once (file switch), so the previous video's last frame doesn't
    /// linger while the next one buffers.
    fn luma_mpv_request_clear();
    /// Register MPRemoteCommandCenter handlers + Now Playing info so the MacBook's
    /// hardware media keys (⏯/⏭/⏮) route to us. MUST run on the main thread.
    fn luma_setup_media_keys();
    /// Update the OS Now Playing widget (title/artist/poster/progress/rate). `artwork`
    /// empty = keep the current poster. MUST run on the main thread.
    fn luma_set_now_playing(
        title: *const c_char,
        artist: *const c_char,
        duration: f64,
        position: f64,
        rate: f64,
        artwork: *const u8,
        artwork_len: usize,
    );
}

/// The app handle for the media-key callback below (MPRemoteCommandCenter fires on the
/// main thread; we just forward the action to the UI as a `media-key` event).
static MEDIA_APP: Mutex<Option<AppHandle>> = Mutex::new(None);

/// Called by the Obj-C MPRemoteCommandCenter handlers when a MacBook media key is
/// pressed; forwards the action (`playpause`/`play`/`pause`/`next`/`prev`) to the UI.
#[no_mangle]
pub extern "C" fn luma_media_key_pressed(action: *const c_char) {
    if action.is_null() {
        return;
    }
    let s = unsafe { CStr::from_ptr(action) }.to_string_lossy().into_owned();
    // Recover from a poisoned lock instead of panicking: this is an extern "C" callback, so
    // an unwind across the FFI boundary would abort the whole process.
    let guard = MEDIA_APP.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(app) = guard.as_ref() {
        let _ = app.emit("media-key", s);
    }
}

/// Called by the Obj-C `changePlaybackPositionCommand` handler when the OS scrubber is
/// dragged; forwards the target position (seconds) to the UI as a `media-seek` event.
#[no_mangle]
pub extern "C" fn luma_media_seek(position: f64) {
    let guard = MEDIA_APP.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(app) = guard.as_ref() {
        let _ = app.emit("media-seek", position);
    }
}

/// Managed Tauri state.
#[derive(Default)]
pub struct MpvState {
    mpv: Mutex<Option<Arc<Mpv>>>,
}

/// Build the engine + spawn the event pump. Call once from `setup`. Returns whether the
/// engine came up, so the caller advertises mpv to the frontend ONLY on success (else the
/// webview `<video>` path is used and no early no-op `mpv_load` can strand playback).
pub fn init(app: &AppHandle, nswindow: *mut c_void) -> bool {
    let mpv = match Mpv::with_initializer(|init| {
        // Render API: mpv draws into OUR GL view (no window of its own); the shim creates
        // the render context after init. `vo=libmpv` selects that output.
        init.set_property("vo", "libmpv")?;
        init.set_property("hwdec", "videotoolbox")?; // HW for HEVC/H264; AV1 → dav1d
        init.set_property("hr-seek", "yes")?;
        init.set_property("force-seekable", "yes")?;
        init.set_property("cache", "yes")?;
        // LUMA renders its own subtitle overlay (React, over the transparent webview), so
        // mpv must draw NONE itself: no external auto-load AND no embedded/default track.
        init.set_property("sub-auto", "no")?;
        init.set_property("sid", "no")?;
        init.set_property("terminal", false)?;
        Ok(())
    }) {
        Ok(m) => Arc::new(m),
        Err(e) => {
            eprintln!("LUMA libmpv: init failed: {e:?}");
            return false;
        }
    };

    let _ = mpv.observe_property("time-pos", Format::Double, 1);
    let _ = mpv.observe_property("duration", Format::Double, 2);
    let _ = mpv.observe_property("demuxer-cache-time", Format::Double, 3);
    let _ = mpv.observe_property("pause", Format::Flag, 4);
    let _ = mpv.observe_property("paused-for-cache", Format::Flag, 5);

    // Create the GL view behind the webview + the mpv render context bound to it (we're
    // on the main thread). mpv (vo=libmpv) then draws each frame into it.
    let handle = mpv.ctx.as_ptr() as *mut c_void;
    let rc = unsafe { luma_mpv_render_setup(nswindow, handle) };
    if rc != 0 {
        eprintln!("LUMA libmpv: render setup failed (rc={rc}); falling back to no video");
    }

    if let Some(state) = app.try_state::<MpvState>() {
        *state.mpv.lock().unwrap() = Some(mpv.clone());
    }
    // MacBook hardware media keys (⏯/⏭/⏮) → the `media-key` event (we're on the main
    // thread, which MPRemoteCommandCenter requires).
    *MEDIA_APP.lock().unwrap() = Some(app.clone());
    unsafe { luma_setup_media_keys() };
    let app_pump = app.clone();
    thread::spawn(move || pump_events(app_pump, mpv));
    eprintln!("LUMA libmpv: engine up (render API, GL view behind the webview)");
    true
}

/// Drain mpv's event queue and forward the events the frontend `MpvEngine` listens for.
fn pump_events(app: AppHandle, mpv: Arc<Mpv>) {
    loop {
        match mpv.wait_event(1.0) {
            Some(Ok(Event::PropertyChange { name, change, .. })) => {
                let data = match change {
                    PropertyData::Double(d) => json!(d),
                    PropertyData::Int64(i) => json!(i),
                    PropertyData::Flag(b) => json!(b),
                    PropertyData::Str(s) | PropertyData::OsdStr(s) => json!(s),
                };
                let _ = app.emit("mpv://property", json!({ "name": name, "data": data }));
            }
            Some(Ok(Event::FileLoaded)) => {
                let _ = app.emit("mpv://file-loaded", ());
                emit_track_list(&app, &mpv);
            }
            Some(Ok(Event::EndFile(reason))) => {
                let r = match reason {
                    0 => "eof",
                    4 => "error",
                    _ => "stop",
                };
                let _ = app.emit("mpv://end-file", json!({ "reason": r }));
            }
            Some(Ok(Event::Shutdown)) => break,
            _ => {}
        }
    }
}

/// Build the track list from `track-list/N/{id,type}` and emit it as an `mpv://property`
/// so the frontend can map an audio-relative rendition to an mpv track id.
fn emit_track_list(app: &AppHandle, mpv: &Mpv) {
    let count = mpv.get_property::<i64>("track-list/count").unwrap_or(0);
    let mut tracks = Vec::new();
    for i in 0..count {
        let id = mpv
            .get_property::<i64>(&format!("track-list/{i}/id"))
            .unwrap_or(-1);
        let ty = mpv
            .get_property::<MpvStr>(&format!("track-list/{i}/type"))
            .map(|s| s.to_string())
            .unwrap_or_default();
        tracks.push(json!({ "id": id, "type": ty }));
    }
    let _ = app.emit("mpv://property", json!({ "name": "track-list", "data": tracks }));
}

// ----- commands invoked by the frontend MpvEngine (same names as mpv.rs) -----

/// Load a URL, replacing the current file. `start` > 0 seeks DURING the open (resume),
/// via `loadfile <url> replace 0 start=<sec>`, so playback begins at the resume point
/// instead of buffering at 0 first.
#[tauri::command]
pub fn mpv_load(state: State<'_, MpvState>, url: String, start: f64) {
    if let Some(mpv) = state.mpv.lock().unwrap().as_ref() {
        if start > 0.5 {
            let opt = format!("start={start}");
            let _ = mpv.command("loadfile", &[url.as_str(), "replace", "0", &opt]);
        } else {
            let _ = mpv.command("loadfile", &[url.as_str(), "replace"]);
        }
    }
    // Blank the last frame of the previous video while the new one buffers.
    unsafe { luma_mpv_request_clear() };
}

/// Send a raw mpv command array (`set_property`, `seek`, `stop`, …). libmpv's string
/// command form parses the args, so we stringify each (bools → mpv's `yes`/`no`).
#[tauri::command]
pub fn mpv_command(state: State<'_, MpvState>, args: Vec<Value>) {
    if args.is_empty() {
        return;
    }
    let mut strs: Vec<String> = args.iter().map(value_to_mpv_arg).collect();
    // The frontend speaks mpv's JSON-IPC dialect (matching the Deck's mpv binary), where
    // `set_property` is an IPC-level command. In-process libmpv's mpv_command only knows
    // INPUT commands - the equivalent there is `set` - so translate it, else mpv returns
    // -4 (INVALID_PARAMETER) and pause / audio-track changes silently no-op.
    if strs[0] == "set_property" {
        strs[0] = "set".to_string();
    }
    let rest: Vec<&str> = strs[1..].iter().map(String::as_str).collect();
    if let Some(mpv) = state.mpv.lock().unwrap().as_ref() {
        let _ = mpv.command(&strs[0], &rest);
    }
}

fn value_to_mpv_arg(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Bool(b) => (if *b { "yes" } else { "no" }).to_string(),
        // Numbers (and anything else) stringify to their unquoted Display form.
        other => other.to_string(),
    }
}

/// Update the OS "Now Playing" widget with the current item + progress. `artwork` is the
/// poster bytes (PNG/JPEG) on an item change, empty otherwise (keeps the current poster).
#[tauri::command]
pub fn set_now_playing(
    app: AppHandle,
    title: String,
    artist: String,
    duration: f64,
    position: f64,
    playing: bool,
    artwork: Vec<u8>,
) {
    let title = CString::new(title).unwrap_or_default();
    let artist = CString::new(artist).unwrap_or_default();
    let rate = if playing { 1.0 } else { 0.0 };
    let _ = app.run_on_main_thread(move || {
        // as_ptr() on an empty Vec is valid + len() is 0, and the C side gates on len > 0.
        unsafe {
            luma_set_now_playing(
                title.as_ptr(),
                artist.as_ptr(),
                duration,
                position,
                rate,
                artwork.as_ptr(),
                artwork.len(),
            );
        }
    });
}
