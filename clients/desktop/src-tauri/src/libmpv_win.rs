// In-process libmpv engine for Windows.
//
// Mirrors mpv.rs / libmpv_mac.rs's Tauri surface (`mpv_load` / `mpv_command` +
// `mpv://…` events) so the frontend `MpvEngine` drives it UNCHANGED. Unlike
// macOS (where `--wid` embedding into an NSView proved unreliable, forcing a
// render-API + Obj-C GL shim), on Windows libmpv's `--wid` embedding is the
// supported, simple path: we hand mpv the app window's HWND and its built-in
// gpu/d3d11 video output renders a child surface inside it. No C shim.
//
// Compositing model (same "video plane behind the page" as the Deck / macOS):
// the KROMA window is transparent so the React player chrome floats over the
// video mpv draws behind it. Enabling this engine therefore needs the window's
// `transparent: true` (see tauri.windows.conf.json) and mpv's child surface
// kept at the BOTTOM of the z-order; that window-layering is the on-device
// tuning step (this box can't build/run Windows). The engine itself - decode,
// IPC command mapping, event pump - is platform-independent and lives here.
//
// libmpv is thread-safe (`Mpv: Send + Sync`): commands run on invoke threads, a
// pump thread drains `wait_event`.

use std::sync::{Arc, Mutex};
use std::thread;

use libmpv2::events::{Event, PropertyData};
use libmpv2::{Format, Mpv, MpvStr};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager, State};

/// Managed Tauri state.
#[derive(Default)]
pub struct MpvState {
    mpv: Mutex<Option<Arc<Mpv>>>,
}

/// Build the engine embedded in `hwnd` and spawn the event pump. Call once the
/// window exists. Returns whether the engine came up, so the caller advertises
/// mpv to the frontend ONLY on success (else the webview `<video>` path is used
/// and no early no-op `mpv_load` can strand playback).
pub fn init(app: &AppHandle, hwnd: i64) -> bool {
    let mpv = match Mpv::with_initializer(|init| {
        // Embed into the app window's HWND: mpv creates its child render surface
        // inside it (its normal gpu VO), rather than opening a window of its own.
        init.set_property("wid", hwnd)?;
        // gpu output + hardware decode: d3d11va for HEVC/H264, dav1d for AV1.
        init.set_property("vo", "gpu")?;
        init.set_property("hwdec", "auto-safe")?;
        init.set_property("hr-seek", "yes")?;
        init.set_property("force-seekable", "yes")?;
        init.set_property("cache", "yes")?;
        // KROMA renders its own subtitle overlay (React, over the transparent
        // webview), so mpv must draw NONE itself.
        init.set_property("sub-auto", "no")?;
        init.set_property("sid", "no")?;
        init.set_property("terminal", false)?;
        Ok(())
    }) {
        Ok(m) => Arc::new(m),
        Err(e) => {
            eprintln!("KROMA libmpv(win): init failed: {e:?}");
            return false;
        }
    };

    let _ = mpv.observe_property("time-pos", Format::Double, 1);
    let _ = mpv.observe_property("duration", Format::Double, 2);
    let _ = mpv.observe_property("demuxer-cache-time", Format::Double, 3);
    let _ = mpv.observe_property("pause", Format::Flag, 4);
    let _ = mpv.observe_property("paused-for-cache", Format::Flag, 5);

    if let Some(state) = app.try_state::<MpvState>() {
        *state.mpv.lock().unwrap() = Some(mpv.clone());
    }
    let app_pump = app.clone();
    thread::spawn(move || pump_events(app_pump, mpv));
    eprintln!("KROMA libmpv(win): engine up (wid embed, hwnd={hwnd})");
    true
}

/// Drain mpv's event queue and forward the events the frontend `MpvEngine`
/// listens for (identical mapping to the Deck binary / macOS libmpv).
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

/// Build the track list from `track-list/N/{id,type}` and emit it as an
/// `mpv://property` so the frontend can map an audio-relative rendition to an
/// mpv track id.
fn emit_track_list(app: &AppHandle, mpv: &Mpv) {
    let count = mpv.get_property::<i64>("track-list/count").unwrap_or(0);
    let mut tracks = Vec::new();
    for i in 0..count {
        let id = mpv.get_property::<i64>(&format!("track-list/{i}/id")).unwrap_or(-1);
        let ty = mpv
            .get_property::<MpvStr>(&format!("track-list/{i}/type"))
            .map(|s| s.to_string())
            .unwrap_or_default();
        tracks.push(json!({ "id": id, "type": ty }));
    }
    let _ = app.emit("mpv://property", json!({ "name": "track-list", "data": tracks }));
}

// ----- commands invoked by the frontend MpvEngine (same names as mpv.rs) -----

/// Load a URL, replacing the current file. `start` > 0 seeks DURING the open
/// (resume) via `loadfile <url> replace 0 start=<sec>`, so playback begins at
/// the resume point instead of buffering at 0 first.
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
}

/// Send a raw mpv command array (`set_property`, `seek`, `stop`, …). libmpv's
/// string command form parses the args, so we stringify each (bools → `yes`/`no`).
#[tauri::command]
pub fn mpv_command(state: State<'_, MpvState>, args: Vec<Value>) {
    if args.is_empty() {
        return;
    }
    let mut strs: Vec<String> = args.iter().map(value_to_mpv_arg).collect();
    // The frontend speaks mpv's JSON-IPC dialect (matching the Deck's mpv
    // binary), where `set_property` is an IPC-level command. In-process libmpv's
    // mpv_command only knows INPUT commands - the equivalent there is `set` - so
    // translate it, else mpv returns -4 (INVALID_PARAMETER) and pause / audio-
    // track changes silently no-op.
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
        other => other.to_string(),
    }
}
