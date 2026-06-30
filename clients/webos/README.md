# @luma/webos LG TV (webOS)

> Part of the [LUMA](../../README.md) monorepo the LG TV shell.

Thin shell over **`@luma/tv`** (the shared 10-foot experience). webOS TVs decode
HEVC/H.265 (incl. HDR) in hardware, so playback is direct-play.

## Develop (in a desktop browser)

```bash
bun install
bun run server          # Rust media server :4040
bun run dev:webos       # Vite dev server :5175 use arrow keys + Enter as a remote
```

## Build the web bundle

```bash
bun run build:webos     # → clients/webos/dist (appinfo.json + icons copied from public/)
```

## Package an .ipk and install on a TV

Requires the **webOS TV CLI** (`@webos-tools/cli`, provides `ares-*`), not bundled
here. After `build:webos`:

```bash
ares-package clients/webos/dist                       # → app.luma.webos_0.1.0_all.ipk
ares-setup-device                                     # register your TV (Developer Mode app)
ares-install app.luma.webos_0.1.0_all.ipk -d <tv>
ares-launch app.luma.webos -d <tv>
```

Notes:
- `disableBackHistoryAPI: true` routes the remote Back button to the app, where
  `@luma/core`'s remote mapping (`keyCode 461`) handles it.
- Arrow keys + OK drive spatial focus navigation; media keys control the player.
- Set the server address on first launch (connection screen); it persists in
  `localStorage`.
