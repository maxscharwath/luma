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
// over the video. The Linux packages bundle their own mpv (the `kroma-mpv` sidecar,
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

/// mpv args shared by every video-output attempt (everything except the
/// `--vo`/`--gpu-*` selection and the `--input-ipc-server`, which vary per rung).
const BASE_ARGS: &[&str] = &[
    "--idle=yes",         // stay alive with no file (we loadfile later)
    "--force-window=yes", // create the video window up front
    "--fullscreen",       // fill the Deck screen behind the UI
    "--ontop=no",         // stay BELOW the always-on-top Tauri window
    "--title=KROMA Player", // stable window title (not the media filename) in alt-tab
    "--no-osc",           // no mpv on-screen controls (KROMA draws its own)
    "--no-input-default-bindings",
    "--no-terminal",
    "--no-config",          // deterministic: ignore any user mpv.conf
    "--keep-open=no",       // let end-file fire, then return to idle
    "--hwdec=auto-safe",    // VA-API hardware decode where available
    "--cache=yes",
    "--hr-seek=yes",        // frame-accurate seeks for the scrub bar
    "--force-seekable=yes", // seek HTTP sources even if length is unknown
    "--sub-auto=no",        // KROMA renders its own subtitle overlay
    "--sid=no",
    "--ytdl=no",            // never invoke yt-dlp: KROMA only opens its own HTTP file URLs
];

fn socket_path() -> PathBuf {
    std::env::temp_dir().join("kroma-mpv.sock")
}

/// Ensure a directory holding a no-op `yt-dlp` executable exists, and return it.
///
/// The bundled `kroma-mpv` is a pkgforge mpv AppImage whose launcher sources a
/// `get-yt-dlp.hook`: when `yt-dlp` isn't on PATH it pops a **modal** kdialog
/// ("kroma-mpv needs yt-dlp ... install it now?") *before* exec'ing mpv. That
/// dialog blocks startup, so mpv's IPC socket never appears, every VO rung
/// times out, and each re-spawn stacks another dialog (the "popup every 5s"
/// the Deck showed). KROMA never plays online video, so we make `yt-dlp` appear
/// present - a stub the hook only ever probes with `command -v`, never runs
/// (we also pass `--ytdl=no`). Robust to the AppImage's name/cache layout,
/// unlike the hook's per-file denyfile. Best-effort: any error just means the
/// nag may return, not a playback failure.
fn ytdlp_shim_dir() -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join("kroma-mpv-shim");
    std::fs::create_dir_all(&dir).ok()?;
    let stub = dir.join("yt-dlp");
    // Idempotent: write once, keep it executable.
    if !stub.exists() {
        std::fs::write(&stub, "#!/bin/sh\nexit 0\n").ok()?;
    }
    std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).ok()?;
    Some(dir)
}

/// Video-output fallback ladder, most-capable first. mpv aborts (and its IPC
/// socket never appears) when a video output can't initialise its GPU context;
/// [`start_mpv`] detects that early exit and drops to the next rung.
///
/// The default `gpu-next` needs an EGL/GL context that fails on some stacks -
/// notably the Steam Deck's KDE-Wayland *desktop* session, which aborts with
/// "Could not create default EGL display: EGL_BAD_PARAMETER" (the very same
/// driver bug the webview dodges via `WEBKIT_DISABLE_DMABUF_RENDERER`). The
/// later rungs sidestep that EGL path: Vulkan (the Deck's native API, no EGL),
/// then GLX on X11/XWayland (no EGL), then plain software output (always works).
///
/// `KROMA_MPV_VO` pins exactly one output and skips the ladder (with optional
/// `KROMA_MPV_GPU_API` / `KROMA_MPV_GPU_CONTEXT`) handy to lock in a known-good
/// combo, or to probe one on a specific box without a rebuild.
fn vo_ladder() -> Vec<Vec<String>> {
    if let Ok(vo) = std::env::var("KROMA_MPV_VO") {
        let vo = vo.trim();
        if !vo.is_empty() {
            let mut cfg = vec![format!("--vo={vo}")];
            for (var, flag) in [
                ("KROMA_MPV_GPU_API", "--gpu-api"),
                ("KROMA_MPV_GPU_CONTEXT", "--gpu-context"),
            ] {
                if let Ok(val) = std::env::var(var) {
                    let val = val.trim();
                    if !val.is_empty() {
                        cfg.push(format!("{flag}={val}"));
                    }
                }
            }
            return vec![cfg];
        }
    }
    vec![
        vec!["--vo=gpu-next".into()],                         // modern GPU output (auto context)
        vec!["--vo=gpu-next".into(), "--gpu-api=vulkan".into()], // Vulkan: no EGL, ideal on the Deck
        vec!["--vo=gpu".into(), "--gpu-context=x11".into()], // GLX via X11/XWayland: no EGL
        vec!["--vo=x11".into()],                             // pure software: last-resort, always works
    ]
}

/// Resolve the mpv binary. The bundled `kroma-mpv` sidecar (Tauri externalBin: a
/// self-contained mpv AppImage installed next to the KROMA binary) is probed first.
/// A GUI-launched app (Finder / Steam Game Mode) inherits a minimal PATH that
/// usually omits Homebrew / Flatpak dirs, so probe the common install locations
/// before falling back to a bare PATH lookup. `KROMA_MPV` overrides everything.
fn mpv_binary() -> String {
    if let Ok(p) = std::env::var("KROMA_MPV") {
        if !p.trim().is_empty() {
            return p;
        }
    }
    // Bundled sidecar: $APPDIR/usr/bin/kroma-mpv inside the AppImage,
    // /usr/bin/kroma-mpv from the .deb.
    if let Ok(exe) = std::env::current_exe() {
        if let Some(cand) = exe.parent().map(|d| d.join("kroma-mpv")) {
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
    eprintln!("KROMA: mpv unavailable ({reason})");
    let _ = app.emit("mpv://error", json!({ "reason": reason }));
}

/// Launch mpv (idle + windowed) and spawn the reader thread that forwards its IPC
/// events. Call once at setup; failures are logged, not fatal (the UI still runs).
pub fn spawn(app: AppHandle) {
    std::thread::spawn(move || {
        let Some(read_half) = connect(&app) else { return };
        pump_events(&app, read_half);
        finish(&app);
    });
}

/// Bring mpv up, store the process + write half of its IPC socket in the managed
/// state, and hand back the read half for the event loop. Returns `None` (after
/// emitting the failure to the webview) when mpv can't launch or the socket can't
/// be cloned.
fn connect(app: &AppHandle) -> Option<UnixStream> {
    let sock = socket_path();
    let binary = mpv_binary();

    // Bring mpv up on a video output this machine can actually initialise. The
    // default gpu-next needs an EGL/GL context that aborts on some driver stacks
    // (the Steam Deck's KDE-Wayland desktop: "Could not create default EGL
    // display: EGL_BAD_PARAMETER"), so mpv dies and its IPC socket never appears.
    // `start_mpv` walks the fallback ladder (Vulkan → GLX → software) until one
    // stays up. On a healthy machine the first rung wins instantly.
    let (child, stream) = match start_mpv(&binary, &sock) {
        Ok(v) => v,
        Err(reason) => {
            if reason == "socket-timeout" {
                eprintln!("KROMA: mpv IPC socket never appeared at {}", sock.display());
            }
            emit_error(app, reason);
            return None;
        }
    };
    if let Some(state) = app.try_state::<MpvState>() {
        *state.child.lock().unwrap() = Some(child);
    }

    let read_half = match stream.try_clone() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("KROMA: could not clone mpv IPC socket: {e}");
            emit_error(app, "socket-error");
            return None;
        }
    };
    if let Some(state) = app.try_state::<MpvState>() {
        *state.conn.lock().unwrap() = Some(stream);
    }
    Some(read_half)
}

/// Subscribe to the observed properties, then forward every IPC event to the
/// webview until the socket closes (mpv exited).
fn pump_events(app: &AppHandle, read_half: UnixStream) {
    // Best-effort focus-stealing prevention for a SYSTEM mpv, which doesn't get
    // the sidecar-only `--focus-on=never` (see start_mpv): setting it over IPC
    // just returns an error reply on pre-0.39 builds instead of killing mpv.
    let _ = write_ipc(app, &json!({ "command": ["set_property", "focus-on", "never"] }));
    for (i, prop) in OBSERVED.iter().enumerate() {
        let _ = write_ipc(app, &json!({ "command": ["observe_property", i + 1, prop] }));
    }
    let reader = BufReader::new(read_half);
    for line in reader.lines() {
        let Ok(line) = line else { break };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Ok(msg) = serde_json::from_str::<Value>(line) {
            forward(app, &msg);
        }
    }
}

/// The IPC stream ended: mpv exited (crash or kill). Drop the dead write half so
/// commands fail fast, and tell the webview so an active player errors out instead
/// of spinning forever. (On normal app exit the webview is gone anyway.)
fn finish(app: &AppHandle) {
    if let Some(state) = app.try_state::<MpvState>() {
        *state.conn.lock().unwrap() = None;
    }
    let _ = app.emit("mpv://exited", ());
}

/// Launch mpv, trying each rung of the video-output [`vo_ladder`] until one comes
/// up with a live IPC socket. Failed rungs (mpv aborts on a bad GPU context) are
/// killed and reaped before the next is tried. Returns the winning process + its
/// connected socket, or an error reason (`spawn-failed` if the binary itself can't
/// launch, `socket-timeout` if every rung failed to produce a socket).
fn start_mpv(binary: &str, sock: &Path) -> Result<(Child, UnixStream), &'static str> {
    let ladder = vo_ladder();
    // PATH with a no-op yt-dlp shim prepended, so the AppImage's get-yt-dlp.hook
    // sees yt-dlp as "present" and skips its blocking install dialog. See
    // [`ytdlp_shim_dir`]. Computed once; None only if the temp write failed.
    let shim_path = ytdlp_shim_dir().map(|dir| {
        let mut p = std::ffi::OsString::from(dir);
        if let Some(existing) = std::env::var_os("PATH") {
            p.push(":");
            p.push(existing);
        }
        p
    });
    // The pinned sidecar (mpv 0.41) understands --focus-on; a PRE-0.39 system mpv
    // would abort on the unknown option and sink every ladder rung, so only the
    // bundled binary gets it. Without it the fullscreen mpv window grabs focus the
    // moment it maps (KROMA launches to a black screen until an alt-tab back), and
    // a focused fullscreen window outranks the keep-above UI window on KDE.
    let sidecar_focus_flag = Path::new(binary)
        .file_name()
        .is_some_and(|n| n.to_string_lossy().starts_with("kroma-mpv"));
    for cfg in &ladder {
        let _ = std::fs::remove_file(sock);
        let mut command = Command::new(binary);
        {
            // Own process group, so shutdown can kill the WHOLE tree: in
            // extract-and-run mode the spawned pid is the AppImage runtime and the
            // real mpv is a grandchild that Child::kill alone would orphan (the
            // player kept running after the app was closed on the Deck).
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        if sidecar_focus_flag {
            command.arg("--focus-on=never");
        }
        command
            // The bundled kroma-mpv is itself an AppImage; we spawn it from INSIDE the
            // KROMA AppImage, where nested FUSE mounting is unreliable (esp. SteamOS).
            // Force extract-and-run so mpv never depends on FUSE; harmless for a
            // non-AppImage system mpv (it just ignores the var).
            .env("APPIMAGE_EXTRACT_AND_RUN", "1")
            // Silence the AppImage's self-updater.hook, which otherwise pops a modal
            // "Allow kroma-mpv to check for updates?" dialog once the yt-dlp nag is
            // gone - same startup-blocking failure mode. KROMA updates the whole
            // desktop bundle via the Tauri updater; the sidecar rides along.
            .env("DISABLE_AUTO_UPDATES", "1")
            // Never let the outer AppImage's runtime env leak into mpv: AppRun's
            // LD_LIBRARY_PATH points at $APPDIR/usr/lib, whose over-bundled stale
            // libs (tauri-apps/tauri#15665) would shadow the self-contained mpv's
            // own stack, and a user's libwayland LD_PRELOAD workaround (fixes the
            // webview on pre-fix builds) would poison mpv the same way. mpv is
            // not a GTK app; it needs none of this env. (The historic all-rungs
            // socket-timeout on the Deck was the patchelf-corrupted sidecar,
            // repaired at bundle time by scripts/fix-appimage.sh.)
            .env_remove("LD_LIBRARY_PATH")
            .env_remove("LD_PRELOAD")
            .env_remove("APPDIR")
            // Keep mpv on XWayland like the UI window (GDK_BACKEND=x11, see main.rs):
            // the keep-above sandwich and --focus-on=never both rely on X11 WM
            // semantics. A native-Wayland mpv gets ACTIVATED by the compositor the
            // moment its fullscreen window maps (a client can't refuse focus on
            // Wayland), which is exactly the launch black screen the Deck showed.
            .env_remove("WAYLAND_DISPLAY")
            .args(BASE_ARGS)
            .args(cfg)
            .arg(format!("--input-ipc-server={}", sock.display()));
        if let Some(ref p) = shim_path {
            command.env("PATH", p);
        }
        let child = command.spawn();

        let mut child = match child {
            Ok(c) => c,
            Err(e) => {
                // A missing / unspawnable binary won't be fixed by a different VO.
                eprintln!("KROMA: failed to launch mpv (is it installed / on PATH?): {e}");
                return Err("spawn-failed");
            }
        };

        match await_socket(&mut child, sock) {
            Some(stream) => {
                eprintln!("KROMA: mpv up [{}]", cfg.join(" "));
                return Ok((child, stream));
            }
            None => {
                kill_tree(&mut child);
                eprintln!(
                    "KROMA: mpv could not start [{}]; trying a more compatible video output",
                    cfg.join(" ")
                );
            }
        }
    }
    Err("socket-timeout")
}

/// Wait for mpv's IPC socket to appear (it is created asynchronously after
/// launch), short-circuiting the instant the process exits first - a failed video
/// output aborts in well under a second, so we fail over fast rather than block
/// the whole window on a rung that already died. The window is generous (~15s)
/// because the bundled kroma-mpv is an AppImage running in extract-and-run mode:
/// a cold launch unpacks ~50 MB before mpv even starts, which can exceed 5s on
/// slow disks; only ALIVE-but-not-ready rungs pay it, dead rungs exit instantly.
fn await_socket(child: &mut Child, sock: &Path) -> Option<UnixStream> {
    for _ in 0..300 {
        if matches!(child.try_wait(), Ok(Some(_))) {
            return None; // mpv aborted (e.g. EGL/GPU-context failure)
        }
        if let Ok(s) = UnixStream::connect(sock) {
            return Some(s);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    None // still no socket after ~15s and the process is alive: treat as stuck
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
        kill_tree(&mut child);
    }
    let _ = std::fs::remove_file(socket_path());
}

/// Kill mpv and every descendant via its process group ([`start_mpv`] spawns it
/// as a group leader). SIGTERM first so mpv tears its window/VA-API state down
/// cleanly, SIGKILL after a short grace for anything still standing.
fn kill_tree(child: &mut Child) {
    let pgid = child.id() as libc::pid_t;
    let _ = unsafe { libc::kill(-pgid, libc::SIGTERM) };
    for _ in 0..20 {
        if matches!(child.try_wait(), Ok(Some(_))) {
            return;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    let _ = unsafe { libc::kill(-pgid, libc::SIGKILL) };
    let _ = child.wait();
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
