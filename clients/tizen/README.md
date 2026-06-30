# @luma/tizen Samsung TV (Tizen)

> Part of the [LUMA](../../README.md) monorepo the Samsung TV shell.

Thin shell over **`@luma/tv`** (the shared 10-foot experience). Tizen TVs decode
HEVC/H.265 (incl. 10-bit / HDR) in hardware, so playback is direct-play.

## Develop (in a desktop browser)

```bash
bun install
bun run server          # Rust media server :4040
bun run dev:tizen       # Vite dev server :5174 use arrow keys + Enter as a remote
```

## Build / prepare the app

```bash
bun run tizen:prepare   # builds → clients/tizen/dist + prints the packaging command
# (equivalent to `bun run build:tizen`; dist gets config.xml + icon.png from public/)
```

## Smart Hub preview (new-movies carousel)

When the LUMA tile is focused on the TV home screen **even while the app isn't
running** Samsung expands it into a carousel of the newest movies. Selecting a
tile opens that movie's detail page in LUMA.

How it works:

- A **background service** ([`public/service/preview-service.js`](./public/service/preview-service.js))
  is declared in `config.xml` (`use.preview = bg_service`). The TV runs it on its
  own schedule to fetch the carousel data.
- The foreground app ([`@luma/tv` `preview.ts`](../../packages/tv/src/preview.ts))
  builds the tile JSON from the live catalog and writes it to the package-private
  `wgt-private/preview.json`; the service reads that file and calls
  `webapis.preview.setPreviewData()`.
- Each tile carries a `PAYLOAD` (`{type:'movie', id}`); on launch the app reads it
  via `getRequestedAppControl()` / the `appcontrol` event and opens the page. The
  platform may deliver the payload verbatim or wrapped as
  `{"values": encodeURIComponent(...)}` `parsePayload` handles both.

**Where to see it:** the carousel only shows on the Smart Hub home, when the LUMA
tile is **added to the launcher and highlighted** never from inside the app.
After a fresh install, open LUMA once (so it writes the data), return Home, and
focus the tile. A full power-off/on forces a refresh.

Notes / caveats:

- **Images must be PNG/JPG, not WebP** (Samsung carousel limit; also ≤360 KB,
  height ≤360 px). LUMA caches posters as WebP, so tiles request the server's
  on-the-fly JPEG rendition (`/api/images/<hash>.webp.jpg`, see
  [`server/src/image.rs`](../../server/src/image.rs) `jpeg_rendition`). The
  server must be running a build with that endpoint.
- Only movies with resolved TMDB art are included (un-enriched titles, which
  would fall back to a non-raster SVG poster, are skipped until enrichment).
- Data refreshes whenever the app is opened (it rewrites the file); the service
  re-asserts it to the home screen on the TV's schedule while LUMA is closed.
- Image URLs point at the LAN server, so the TV must be able to reach it.
- Debugging on a **retail TV**: `sdb dlog`/`sdb shell` are disabled
  (`intershell_support:disabled`), so the service/app can't log to the device.
  Mirror logs to a LAN HTTP collector (Samsung's own sample does this) the
  service can POST via `require('http')`, the app via `fetch` (its `console.*` is
  stripped from the production build).
- `devel.api.version` in `config.xml` targets the Samsung Product API level; bump
  it toward the device's version if a newer `webapis` is ever needed.

## Performance built to feel like Netflix / Disney+

TVs have weak CPUs/GPUs and slow storage, so the shell is tuned for that:

- **Lazy, async poster decoding** every tile is a real `<img loading="lazy"
  decoding="async">`; off-screen artwork in long rails is never fetched or
  decoded until it nears the viewport.
- **Off-screen tiles cost ~nothing** `content-visibility: auto` lets the
  browser skip layout + paint for poster tiles that aren't on screen, while they
  stay in the DOM so the remote can still focus and scroll to them.
- **Memoised tiles** `PosterCard` is `React.memo`'d, so scrolling a rail doesn't
  re-render unaffected tiles.
- **GPU-only focus animation** focus uses `transform`/`box-shadow` (composited),
  never layout-triggering properties, for a smooth 60 fps highlight.
- **Lean bundle** production build is a single JS + single CSS file (fewer TV
  round-trips), `console`/`debugger` stripped, ES2018 target for the Tizen webview.
  Ships ~**52 kB gzip** JS.
- **Early connection warm-up** a `<link rel="preconnect">` to the media server
  is injected as soon as the client is created.

These improvements live in `@luma/ui` + `@luma/tv`, so the LG/webOS app gets them too.

## Package + deploy to a real TV

A [`Makefile`](./Makefile) automates the whole pipeline. One-time setup (Tizen
CLI, Samsung certificate, TV Developer Mode) is documented in
**[SETUP.md](./SETUP.md)** it can't be scripted because it needs your Samsung
account and your TV.

```bash
make doctor                       # check tools + config
make deploy TV_IP=192.168.1.50    # build → sign → install → launch on the TV
make logs                         # watch the app's console output
make redeploy                     # fast iteration after a code change
```

Or via bun from the repo root: `bun run --filter @luma/tizen deploy` (after a
`.tizen.env` is configured).

Notes:
- `config.xml` targets Tizen 6.0+ (2021+ TVs), package id `LumaTV0001`.
- Retail Samsung TVs require a **Samsung** signing certificate tied to the TV's
  DUID see [SETUP.md](./SETUP.md) step 3. A self-signed cert only works on the
  emulator.
- Media/colour remote keys are registered at runtime via `@luma/core`'s
  `registerTvMediaKeys()`; arrow keys + OK drive spatial focus navigation.
- Set the server address on first launch (connection screen); it persists in
  `localStorage`.
