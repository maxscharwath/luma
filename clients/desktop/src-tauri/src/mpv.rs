// Native mpv playback for the Steam Deck shell.
//
// We drive the mpv BINARY (not libmpv) over its JSON IPC socket, so there is no
// libmpv build dependency: this compiles anywhere with just std + serde. mpv is
// launched once, idle and windowed, and stays alive across items; the frontend's
// MpvEngine sends it `loadfile` / `set_property` / `seek` / `stop` commands and
// listens for the observed-property + lifecycle events we forward as Tauri events.
//
// mpv renders to its OWN native window (VA-API hardware decode on the Deck's APU).
// The Tauri UI window is transparent + always-on-top, so the web chrome floats
// over the video. The Linux packages bundle their own mpv (the `luma-mpv` sidecar,
// a self-contained mpv AppImage - see scripts/fetch-mpv.sh); a system mpv
// (Flatpak / pacman / PATH) is only a fallback for dev environments.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::Mutex;
use std::time::Duration;

use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};

/// mpv process + the write half of its IPC socket, shared as Tauri managed state.
#[derive(Default)]
pub struct MpvState {
    conn: Mutex<Option<UnixStream>>,
    child: Mutex<Option<Child>>,
}

/// mpv properties we observe and forward to the webview. The index is the mpv
/// observe id; the frontend keys off the name, so the ids only need to be unique.
const OBSERVED: &[&str] = &[
    "time-pos",
    "duration",
    "pause",
    "paused-for-cache",
    "demuxer-cache-time",
    "track-list", // audio-track ids, so the player selects the RIGHT language
];

fn socket_path() -> PathBuf {
    std::env::temp_dir().join("luma-mpv.sock")
}

/// Resolve the mpv binary. The bundled `luma-mpv` sidecar (Tauri externalBin: a
/// self-contained mpv AppImage installed next to the LUMA binary) is probed first.
/// A GUI-launched app (Finder / Steam Game Mode) inherits a minimal PATH that
/// usually omits Homebrew / Flatpak dirs, so probe the common install locations
/// before falling back to a bare PATH lookup. `LUMA_MPV` overrides everything.
fn mpv_binary() -> String {
    if let Ok(p) = std::env::var("LUMA_MPV") {
        if !p.trim().is_empty() {
            return p;
        }
    }
    // Bundled sidecar: $APPDIR/usr/bin/luma-mpv inside the AppImage,
    // /usr/bin/luma-mpv from the .deb.
    if let Ok(exe) = std::env::current_exe() {
        if let Some(cand) = exe.parent().map(|d| d.join("luma-mpv")) {
            if cand.exists() {
                return cand.to_string_lossy().into_owned();
            }
        }
    }
    for cand in [
        "/opt/homebrew/bin/mpv",            // macOS Apple-Silicon Homebrew
        "/usr/local/bin/mpv",               // macOS Intel Homebrew / common Linux
        "/usr/bin/mpv",                     // system package (SteamOS pacman, apt)
        "/var/lib/flatpak/exports/bin/mpv", // Flatpak-exported mpv
    ] {
        if std::path::Path::new(cand).exists() {
            return cand.to_string();
        }
    }
    "mpv".to_string() // last resort: rely on PATH
}

/// Tell the webview the native mpv plane is unusable (`mpv://error`) so an active
/// player can fail fast instead of spinning forever. Startup failures land before
/// any engine listens; those are caught by the `mpv_status` probe instead.
fn emit_error(app: &AppHandle, reason: &str) {
    eprintln!("LUMA: mpv unavailable ({reason})");
    let _ = app.emit("mpv://error", json!({ "reason": reason }));
}

/// Launch mpv (idle + windowed) and spawn the reader thread that forwards its IPC
/// events. Call once at setup; failures are logged, not fatal (the UI still runs).
pub fn spawn(app: AppHandle) {
    std::thread::spawn(move || {
        let sock = socket_path();
        let _ = std::fs::remove_file(&sock);

        let child = Command::new(mpv_binary())
            // The bundled luma-mpv is itself an AppImage; we spawn it from INSIDE the
            // LUMA AppImage, where nested FUSE mounting is unreliable (esp. SteamOS).
            // Force extract-and-run so mpv never depends on FUSE; harmless for a
            // non-AppImage system mpv (it just ignores the var).
            .env("APPIMAGE_EXTRACT_AND_RUN", "1")
            .args([
                "--idle=yes",            // stay alive with no file (we loadfile later)
                "--force-window=yes",    // create the video window up front
                "--fullscreen",          // fill the Deck screen behind the UI
                "--ontop=no",            // stay BELOW the always-on-top Tauri window
                "--no-osc",              // no mpv on-screen controls (LUMA draws its own)
                "--no-input-default-bindings",
                "--no-terminal",
                "--no-config",           // deterministic: ignore any user mpv.conf
                "--keep-open=no",        // let end-file fire, then return to idle
                "--hwdec=auto-safe",     // VA-API hardware decode where available
                "--vo=gpu-next",         // modern GPU video output
                "--cache=yes",
                "--hr-seek=yes",         // frame-accurate seeks for the scrub bar
                "--force-seekable=yes",  // seek HTTP sources even if length is unknown
                "--sub-auto=no",         // LUMA renders its own subtitle overlay
                "--sid=no",
            ])
            .arg(format!("--input-ipc-server={}", sock.display()))
            .spawn();

        let child = match child {
            Ok(c) => c,
            Err(e) => {
                eprintln!("LUMA: failed to launch mpv (is it installed / on PATH?): {e}");
                emit_error(&app, "spawn-failed");
                return;
            }
        };
        if let Some(state) = app.try_state::<MpvState>() {
            *state.child.lock().unwrap() = Some(child);
        }

        // mpv creates the socket asynchronously after launch; connect once it exists.
        let Some(stream) = connect(&sock) else {
            eprintln!("LUMA: mpv IPC socket never appeared at {}", sock.display());
            emit_error(&app, "socket-timeout");
            return;
        };
        let read_half = match stream.try_clone() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("LUMA: could not clone mpv IPC socket: {e}");
                emit_error(&app, "socket-error");
                return;
            }
        };
        if let Some(state) = app.try_state::<MpvState>() {
            *state.conn.lock().unwrap() = Some(stream);
        }

        // Subscribe to the properties the engine consumes, then pump events.
        for (i, prop) in OBSERVED.iter().enumerate() {
            let _ = write_ipc(&app, &json!({ "command": ["observe_property", i + 1, prop] }));
        }
        let reader = BufReader::new(read_half);
        for line in reader.lines() {
            let Ok(line) = line else { break };
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if let Ok(msg) = serde_json::from_str::<Value>(line) {
                forward(&app, &msg);
            }
        }
        // The IPC stream ended: mpv exited (crash or kill). Drop the dead write half
        // so commands fail fast, and tell the webview so an active player errors out
        // instead of spinning forever. (On normal app exit the webview is gone anyway.)
        if let Some(state) = app.try_state::<MpvState>() {
            *state.conn.lock().unwrap() = None;
        }
        let _ = app.emit("mpv://exited", ());
    });
}

/// Retry-connect to mpv's IPC socket for a few seconds after launch.
fn connect(sock: &Path) -> Option<UnixStream> {
    for _ in 0..100 {
        if let Ok(s) = UnixStream::connect(sock) {
            return Some(s);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    None
}

/// Write one newline-delimited JSON IPC message to mpv. Errs when mpv is not
/// running (never launched / crashed) or the socket write fails, so the invoking
/// frontend promise REJECTS and the engine can fail over instead of assuming the
/// command landed. A write failure also retires the dead connection.
fn write_ipc(app: &AppHandle, msg: &Value) -> Result<(), String> {
    let Some(state) = app.try_state::<MpvState>() else {
        return Err("mpv state unavailable".into());
    };
    let mut guard = state.conn.lock().unwrap();
    let Some(stream) = guard.as_mut() else {
        return Err("mpv is not running (no IPC connection)".into());
    };
    let mut line = msg.to_string();
    line.push('\n');
    let res = stream.write_all(line.as_bytes()).and_then(|()| stream.flush());
    res.map_err(|e| {
        *guard = None; // dead socket: fail fast from now on (reader thread emits mpv://exited)
        format!("mpv IPC write failed: {e}")
    })
}

/// Map an mpv IPC event to the Tauri events the frontend MpvEngine listens for.
fn forward(app: &AppHandle, msg: &Value) {
    match msg.get("event").and_then(Value::as_str).unwrap_or("") {
        "property-change" => {
            let name = msg.get("name").and_then(Value::as_str).unwrap_or_default();
            let data = msg.get("data").cloned().unwrap_or(Value::Null);
            let _ = app.emit("mpv://property", json!({ "name": name, "data": data }));
        }
        "file-loaded" => {
            let _ = app.emit("mpv://file-loaded", ());
        }
        "end-file" => {
            let reason = msg.get("reason").and_then(Value::as_str).unwrap_or_default();
            let _ = app.emit("mpv://end-file", json!({ "reason": reason }));
        }
        _ => {}
    }
}

/// Kill the mpv process (called on app exit; Tauri does not reap children).
pub fn shutdown(state: &MpvState) {
    if let Some(mut child) = state.child.lock().unwrap().take() {
        let _ = child.kill();
        let _ = child.wait();
    }
    let _ = std::fs::remove_file(socket_path());
}

// ----- commands invoked by the frontend MpvEngine ----------------------------

/// Load a URL into mpv, replacing the current file. `start` > 0 seeks DURING the open
/// (resume) via `loadfile … start=<sec>`, so playback begins there without buffering
/// at 0 first.
#[tauri::command]
pub fn mpv_load(app: AppHandle, url: String, start: f64) -> Result<(), String> {
    let cmd = if start > 0.5 {
        json!({ "command": ["loadfile", url, "replace", "0", format!("start={start}")] })
    } else {
        json!({ "command": ["loadfile", url, "replace"] })
    };
    write_ipc(&app, &cmd)
}

/// Send a raw mpv command array (`set_property`, `seek`, `stop`, …). The frontend
/// passes JSON-compatible args (string / number / bool).
#[tauri::command]
pub fn mpv_command(app: AppHandle, args: Vec<Value>) -> Result<(), String> {
    write_ipc(&app, &json!({ "command": args }))
}

/// Liveness probe for the frontend engine: `running` (IPC up), `starting` (process
/// launched, socket not connected yet), or `dead` (never launched, or exited - the
/// zombie is reaped here so a crash doesn't read as `starting` forever).
#[tauri::command]
pub fn mpv_status(state: tauri::State<MpvState>) -> String {
    if state.conn.lock().unwrap().is_some() {
        return "running".into();
    }
    let mut child = state.child.lock().unwrap();
    match child.as_mut().map(Child::try_wait) {
        Some(Ok(None)) => "starting".into(),
        Some(Ok(Some(_))) => {
            *child = None;
            "dead".into()
        }
        Some(Err(_)) | None => "dead".into(),
    }
}
