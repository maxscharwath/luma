# @kroma/webos LG TV (webOS)

> Part of the [KROMA](../../README.md) monorepo the LG TV shell.

Thin shell over **`@kroma/tv`** (the shared 10-foot experience). webOS TVs decode
HEVC/H.265 (incl. HDR) in hardware, so playback is direct-play.

## Two bundles, one package (old-TV support)

LG freezes Chromium per webOS major (webOS 4.x = 53, 5.0 = 68, 6 = 79, 22 = 87,
23 = 94, 24 = 108), and Tailwind v4's cascade layers need Chrome 99. The build
therefore emits **two bundles** and `dist/index.html` picks one at runtime (an
ES5 loader gated on `CSSLayerBlockRule`):

- **modern** (`dist/assets/`): ESM, ES2020, Lightning CSS @ Chrome 99 - untouched.
- **legacy** (`dist/legacy/`): one ES2015 IIFE + a flattened stylesheet for
  Chromium 53-94 (webOS 4.x-23, 2018-2023 models). `vite.config.legacy.ts`
  lowers the JS (core-js + AbortController + IntersectionObserver polyfills);
  `../tv-build/legacy-css.ts` shims flex `gap` (negative-margin technique),
  `aspect-ratio` (`::before` strut) and `scale`/`translate` (composed
  transform), then `@csstools/postcss-cascade-layers` compiles `@layer` away
  and Lightning CSS down-levels to Chrome 53. `../tv-build/check-legacy.ts`
  fails the build if anything unparseable for Chromium 53 sneaks back.

The whole thing is driven by `tv.target.ts` (platform, dev port, engine
floors) through the shared factory in `clients/tv-build/shell.ts` - see that
file for how to give any shell a legacy tier.

Playback on those engines: MSE cannot decode HEVC there, so `useDirectPlayback`
flags `nativeHls` (UA Chrome < 99) and the player hands the stream-copied HLS
master straight to the TV's media pipeline (`<video src>`, surround preserved),
the same shape as Safari's native-HLS path. webOS 3.x (Chromium 38, 2016-17)
has no CSS custom properties at all and is NOT supported.

Authoring rules that keep the legacy tier working: flex only (no CSS grid), no
`/opacity` colour modifiers, spacing via `gap-*` (shimmed) or margins.

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
ares-package clients/webos/dist                       # → tv.kroma.webos_0.1.0_all.ipk
ares-setup-device                                     # register your TV (Developer Mode app)
ares-install tv.kroma.webos_0.1.0_all.ipk -d <tv>
ares-launch tv.kroma.webos -d <tv>
```

Notes:
- `disableBackHistoryAPI: true` routes the remote Back button to the app, where
  `@kroma/core`'s remote mapping (`keyCode 461`) handles it.
- Arrow keys + OK drive spatial focus navigation; media keys control the player.
- Set the server address on first launch (connection screen); it persists in
  `localStorage`.
