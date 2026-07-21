// In-process libmpv engine for Linux (the Steam Deck / desktop).
//
// Mirrors mpv.rs / libmpv_win.rs's Tauri surface (`mpv_load` / `mpv_command` +
// `mpv://…` events) so the frontend `MpvEngine` drives it UNCHANGED. Like Windows,
// libmpv embeds into the app window via `--wid`: on X11 we hand mpv the GTK
// window's X11 XID and its gpu VO renders a child surface inside it, behind the
// transparent webview (the same "video plane behind the page" model as the mpv
// binary's separate window, but in-process - no second window, no IPC socket).
//
// WHY this is the PRIMARY-but-guarded path: the mpv BINARY (mpv.rs) exists because
// the Deck's EGL/Wayland GPU stack is fragile, and a separate process can walk a VO
// fallback ladder (gpu-next -> vulkan -> GLX -> software) and crash without taking
// the app down. In-process libmpv loses that isolation, so:
//   * init() returns false on ANY failure -> the dispatcher (mpv_dispatch.rs) falls
//     back to spawning the binary, so the Deck is never left without a player.
//   * it is OFF by default (opt in with KROMA_LINUX_LIBMPV=1) until validated on a
//     real Deck; the proven binary stays the default. See mpv_dispatch::opt_in.
//
// libmpv is thread-safe (`Mpv: Send + Sync`): commands run on invoke threads, a
// pump thread drains `wait_event`.

use std::sync::{Arc, Mutex};
use std::thread;

use libmpv2::events::{Event, PropertyData};
use libmpv2::{Format, Mpv, MpvStr};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};

/// Managed Tauri state for the in-process engine. Empty until [`init`] succeeds; the
/// dispatcher checks `is_active()` to route commands here vs to the mpv binary.
#[derive(Default)]
pub struct InprocState {
    mpv: Mutex<Option<Arc<Mpv>>>,
}

impl InprocState {
    /// Whether the in-process engine came up (so the dispatcher routes to it).
    pub fn is_active(&self) -> bool {
        self.mpv.lock().unwrap().is_some()
    }
}

/// Build the engine embedded in the X11 window `xid` and spawn the event pump. Call
/// once the window exists. Returns whether the engine came up, so the caller falls
/// back to the mpv binary on failure (the Deck must never be left without a player).
pub fn init(app: &AppHandle, xid: u64) -> bool {
    let mpv = match Mpv::with_initializer(|init| {
        // Embed into the app window's X11 XID: mpv creates its child render surface
        // inside it (its normal gpu VO), rather than opening a window of its own.
        init.set_property("wid", xid as i64)?;
        // gpu output + hardware decode: VA-API (the Deck's APU) for HEVC/H264,
        // dav1d for AV1. `auto-safe` avoids the copy-back hwdec modes that flicker.
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
            eprintln!("KROMA libmpv(linux): init failed: {e:?}");
            return false;
        }
    };

    let _ = mpv.observe_property("time-pos", Format::Double, 1);
    let _ = mpv.observe_property("duration", Format::Double, 2);
    let _ = mpv.observe_property("demuxer-cache-time", Format::Double, 3);
    let _ = mpv.observe_property("pause", Format::Flag, 4);
    let _ = mpv.observe_property("paused-for-cache", Format::Flag, 5);

    if let Some(state) = app.try_state::<InprocState>() {
        *state.mpv.lock().unwrap() = Some(mpv.clone());
    }
    let app_pump = app.clone();
    thread::spawn(move || pump_events(app_pump, mpv));
    eprintln!("KROMA libmpv(linux): engine up (wid embed, xid={xid})");
    true
}

/// Drain mpv's event queue and forward the events the frontend `MpvEngine`
/// listens for (identical mapping to the binary / macOS / Windows engines).
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

// ----- called by the dispatcher (mpv_dispatch.rs) when in-process is active ------

/// Load a URL, replacing the current file. `start` > 0 seeks DURING the open
/// (resume) via `loadfile <url> replace 0 start=<sec>`, so playback begins at the
/// resume point instead of buffering at 0 first.
pub fn load(state: &InprocState, url: &str, start: f64) {
    if let Some(mpv) = state.mpv.lock().unwrap().as_ref() {
        if start > 0.5 {
            let opt = format!("start={start}");
            let _ = mpv.command("loadfile", &[url, "replace", "0", &opt]);
        } else {
            let _ = mpv.command("loadfile", &[url, "replace"]);
        }
    }
}

/// Send a raw mpv command array (`set_property`, `seek`, `stop`, …). libmpv's
/// string command form parses the args, so we stringify each (bools -> `yes`/`no`).
pub fn command(state: &InprocState, args: &[Value]) {
    if args.is_empty() {
        return;
    }
    let mut strs: Vec<String> = args.iter().map(value_to_mpv_arg).collect();
    // The frontend speaks mpv's JSON-IPC dialect (matching the binary), where
    // `set_property` is an IPC-level command. In-process libmpv's mpv_command only
    // knows INPUT commands - the equivalent there is `set` - so translate it, else
    // mpv returns -4 (INVALID_PARAMETER) and pause / audio-track changes no-op.
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
