# @luma/desktop

LUMA's **native desktop client**: a [Tauri](https://tauri.app) shell over the shared
`@luma/tv` 10-foot experience, with native **mpv** playback and a **gamepad** input
bridge. The **Steam Deck is the primary target** (shipped as an AppImage), but the same
shell is a native Linux app and the architecture is not Deck-specific.

## Architecture (why mpv)

Modelled on how Jellyfin Media Player runs on the Deck: the web UI renders in the app
window, but **video is decoded by a native mpv process, not the browser**. mpv
hardware-decodes HEVC (incl. 10-bit) and surround audio via **VA-API**, sidestepping the
weak/gated HEVC support of a Linux webview. This is LUMA's "server just sends bytes, the
client decodes" model - the desktop analog of the Tizen build's AVPlay.

It rides LUMA's existing player-engine seam:

- `selectEngine` (in `@luma/core`) returns `desktop-mpv` for the `desktop` platform.
- `MpvEngine` (in `@luma/tv`) implements the same `TvEngine` interface as
  `AvplayEngine` / `HtmlEngine`: direct-play the original file, native seeks, in-place
  audio switching (`aid`), with a direct→HLS-master fallback for anything mpv can't demux.
- The Rust shell (`src-tauri/`) launches mpv once (idle, fullscreen, `hwdec=auto-safe`,
  `vo=gpu-next`) and bridges its **JSON IPC** to the webview: two commands (`mpv_load`,
  `mpv_command`) and forwarded property/lifecycle events (`mpv://property`,
  `mpv://file-loaded`, `mpv://end-file`). No libmpv build dependency - it drives the
  mpv binary over a unix socket.

**Compositing:** the Tauri window is `transparent` + `alwaysOnTop`; mpv renders to its
own fullscreen window behind it. Browsing screens paint an opaque background (hiding
idle mpv); the player screen is transparent so the video shows through - the same
"video plane behind the page" trick AVPlay uses. See **Known risks** below.

### Playback per OS

The client uses the best native decoder for each platform, via the same `TvEngine` seam:

- **Linux / Steam Deck → native mpv** (VA-API HW decode). The Linux webview can't do
  HEVC, so mpv is essential. mpv is gated to Linux (`#[cfg(target_os = "linux")]`).
- **macOS → in-webview `<video>`** (`HtmlEngine`). WKWebView decodes HEVC via
  VideoToolbox, so **no mpv is spawned** and the app stays a single, normal window.
  `detectTvEnv` reports macOS Tauri as Safari-class so codec selection is correct.
- **Windows** (later): WebView2 does HEVC with the HEVC Video Extension, so it would
  also use the `<video>` path.

So on macOS you're testing the full UI + gamepad + `<video>` playback; the **mpv engine
is exercised on Linux/the Deck**, where it matters.

## Layout

```
clients/desktop/
  src/
    main.tsx      # installs the stage + gamepad bridge, mounts @luma/tv
    stage.ts      # scales the 1920x1080 TV canvas to the screen (transparent under Tauri)
    gamepad.ts    # Gamepad API -> the TV nav's synthetic key events (D-pad + stick)
  src-tauri/
    src/main.rs   # Tauri app: window + mpv lifecycle
    src/mpv.rs     # mpv process + JSON-IPC bridge (commands + forwarded events)
    tauri.conf.json, Cargo.toml, capabilities/
  scripts/luma-kiosk.sh   # alternative: plain Chromium kiosk (no mpv) - see below
```

The mpv engine itself lives with the shared player:
`packages/tv/src/features/playback/player/mpvEngine.ts`.

## Develop

Frontend only (in a desktop browser, no mpv):

```bash
bun run dev:desktop         # vite on :5178
```

Full app (Tauri window + mpv), on a Linux machine / the Deck with the toolchain:

```bash
cd clients/desktop
bun run tauri:dev           # builds the frontend, opens the Tauri window, launches mpv
```

Needs the [Tauri v2 prerequisites](https://v2.tauri.app/start/prerequisites/) (Rust +
WebKitGTK 4.1) and the **mpv binary** on PATH (dev only; release bundles carry their
own, see below).

## Build the AppImage

Only builds on **Linux** (Ubuntu 22.04 base recommended - the oldest with WebKitGTK
4.1, which keeps the AppImage's glibc floor low enough for SteamOS):

```bash
cd clients/desktop
./scripts/fetch-mpv.sh          # once: the luma-mpv sidecar the bundle embeds
bun run tauri:build
# -> src-tauri/target/release/bundle/appimage/LUMA_0.1.0_amd64.AppImage
```

The Linux bundles embed a self-contained mpv (pkgforge-dev's "anylinux" mpv AppImage,
pinned + sha256-verified by `scripts/fetch-mpv.sh`) as the `luma-mpv` Tauri sidecar
(`tauri.linux.conf.json` -> `bundle.externalBin`). It installs next to the LUMA binary
and is probed first at runtime, so SteamOS needs no mpv install; `LUMA_MPV` still
overrides, and system mpv (Flatpak/pacman/PATH) remains the fallback.

CI does this on every `v*` tag (`.github/workflows/release.yml`, job `desktop`) and
attaches the AppImage to the GitHub Release.

## Build on macOS (for local testing)

```bash
cd clients/desktop
bun run tauri:build:mac        # -> src-tauri/target/release/bundle/macos/LUMA.app
bun run tauri:build:mac:dmg    # also a .dmg (needs Finder automation / a GUI session)
```

`tauri:build:mac` produces a normal `.app` (opaque window, traffic lights, resize,
the LUMA icon) with no mpv process. The `.dmg` step styles its window via AppleScript,
so it needs a real GUI session (it fails in headless/automated shells).

## Install on the Steam Deck

1. Copy `LUMA_*.AppImage` to the Deck and `chmod +x` it. (mpv is bundled inside;
   nothing else to install.)
2. In Desktop mode: **Steam → Add a Non-Steam Game → Browse** → pick the AppImage.
3. Launch it from Game Mode. Set the controller layout to **Gamepad** so the sticks and
   D-pad reach the app's Gamepad API. Point it at your LUMA server via the connect flow.

### Controls

| Control              | Action        |
| -------------------- | ------------- |
| D-pad / left stick   | Move focus    |
| A                    | Select / OK   |
| B / View             | Back          |
| X                    | Play / Pause  |
| L1 / L2              | Seek back     |
| R1 / R2              | Seek forward  |

Directions and seek auto-repeat while held; A/B/X are discrete. (This handles both
D-pad and stick - JMP's client is stick-only.)

## AppImage post-processing (required)

CI runs `scripts/fix-appimage.sh` on every Linux AppImage after `tauri build`
(release.yml and desktop-autoupdate.yml; the autoupdate flow also re-signs and
patches `latest.json`). It fixes two bundler defects - do the same for any
hand-built AppImage you distribute:

- **Over-bundled infra libs** (tauri-apps/tauri#15665): the default bundler
  sweeps the build host's `libwayland-*`, glib family, `libgst*`, `libnghttp2`
  etc. into `usr/lib`. On hosts newer than the builder (SteamOS 3.7+, Mesa 25+)
  the stale `libwayland-client` kills EGL ("Could not create default EGL
  display: EGL_BAD_PARAMETER"): WebKit aborts and no window appears. The script
  strips them; the system copies are drop-in compatible.
- **patchelf-corrupted mpv sidecar**: linuxdeploy patchelf's every executable
  in `usr/bin`, which corrupts the static-pie runtime of the `luma-mpv`
  AppImage - the bundled copy segfaults instantly on EVERY machine (all VO
  rungs "fail", `socket-timeout`). The script restores the pristine bytes from
  `src-tauri/bin/` (run `scripts/fetch-mpv.sh` first). The `.deb` is unaffected
  (no linuxdeploy pass; verified pristine).

The bundled mpv AppImage also ships a `get-yt-dlp.hook` (and a self-updater
hook) that pops a **modal** kdialog before mpv starts when yt-dlp is missing -
which blocks the IPC socket, times out every VO rung, and re-nags on each
re-spawn ("popup every 5s" on the Deck). LUMA never plays online video, so
`mpv.rs` neutralizes both: a no-op `yt-dlp` shim on the child's PATH (the hook
only probes it with `command -v`), `DISABLE_AUTO_UPDATES=1`, and `--ytdl=no`.

Related in-app guards (also part of this fix): `main.rs` drops the stock
AppRun's stale `GST_PLUGIN_SYSTEM_PATH(_1_0)` export (else webview audio dies
with "GStreamer element autoaudiosink not found" AND the user's
`~/.cache/gstreamer-1.0` registry is rebuilt empty - if a broken build already
ran, delete that cache dir once); `mpv.rs` scrubs `LD_LIBRARY_PATH`/
`LD_PRELOAD`/`APPDIR` from the mpv child so the outer AppImage env can't
poison it.

## Known risks (validate on real hardware)

- **Two-window compositing under gamescope.** Transparent-UI-over-mpv-window layering is
  the least-certain part on the Deck's Game Mode compositor. If the UI or video doesn't
  layer correctly, this is the first thing to check (it may need gamescope-specific
  window hints, or driving mpv via `--wid` embedding instead).
- **mpv GPU context / EGL.** mpv's default `--vo=gpu-next` needs an EGL/GL context that
  aborts on some driver stacks (the Deck's KDE-Wayland *desktop* session: "Could not
  create default EGL display: EGL_BAD_PARAMETER" - the same driver bug the webview dodges
  with `WEBKIT_DISABLE_DMABUF_RENDERER`). `mpv.rs` now walks a fallback ladder
  (`gpu-next` → `gpu-next`+Vulkan → `gpu`+GLX/X11 → software `x11`) and keeps the first
  rung whose IPC socket comes up; the winning args are printed as `LUMA: mpv up [...]`.
  Override with `LUMA_MPV_VO` (+ optional `LUMA_MPV_GPU_API` / `LUMA_MPV_GPU_CONTEXT`) to
  pin one output and skip the ladder.
- **HEVC via VA-API** - expected to work on the APU; confirm no software-decode fallback
  kicks in on 10-bit HEVC.
- **HDR** is OLED-only (LCD Decks are SDR); a hardware limit, not ours.
- **Audio-track mapping** assumes mpv assigns `aid` 1,2,3… in file order (rendition R →
  `aid` R+1); verify on a multi-audio title.
- The `icons/icon.png` is a placeholder (upscaled from the webOS icon); replace with real
  art and re-run `bun run tauri:icon` before shipping.

## Fallback: plain Chromium kiosk (no mpv)

`scripts/luma-kiosk.sh` serves the built frontend over http and opens it in a fullscreen
Chromium kiosk (add as a Non-Steam Game). This path uses the browser's own `<video>`
decode - simpler, but relies on Chromium-on-Linux HEVC, which is exactly what the mpv
build avoids. Kept as a quick stepping stone; the AppImage is the intended client.
