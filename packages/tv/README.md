<div align="center">
  <img src="../../.github/assets/logo.svg" alt="LUMA" height="56">
  <h1>@luma/tv</h1>
  <p><i>The shared 10-foot TV experience mounted by the Samsung & LG shells.</i></p>
</div>

> Part of the [LUMA](../../README.md) monorepo. The **entire** living-room app
> connect, profiles, home, detail, player, subtitles lives here once. The
> [`@luma/tizen`](../../clients/tizen/README.md) and
> [`@luma/webos`](../../clients/webos/README.md) clients are thin shells that just
> mount it; nothing TV-specific is duplicated per platform.

<div align="center">
  <img src="../../design/screenshots/tv-detail.jpg" alt="LUMA TV 10-foot detail page" width="80%">
</div>

## Mount it

```ts
import { mountTv } from '@luma/tv';
import '@luma/tv/tv.css';

mountTv();          // renders the whole TV app into #root
```

Or embed the component directly:

```tsx
import { TvApp } from '@luma/tv';

<TvApp />
```

`react` / `react-dom` are **peer dependencies** (≥ 18). Built on
[`@luma/core`](../core/README.md) (API, capabilities, remote map) and
[`@luma/ui`](../ui/README.md) (components, tokens).

## What it provides

- **Spatial focus navigation** (`useFocusNav`) D-pad / arrow-key driven focus
  with auto-scroll-into-view and an always-visible amber focus ring, the way a
  remote expects.
- **Full screen flow** connection / auto-discovery, profiles & Quick Connect
  (QR pairing), home (hero + rails), movie & show detail (cast, seasons), player
  with audio/subtitle selection and resume.
- **Direct-play player** streams the original file and decodes HEVC/HDR in TV
  hardware; falls back to the audio-only HLS path when needed (all via `@luma/core`).
- **Smart Hub preview** (`preview.ts`) builds the "new movies" carousel data
  Samsung shows on the home screen even while the app is closed (see the
  [Tizen README](../../clients/tizen/README.md#smart-hub-preview-new-movies-carousel)).
- **Tuned for TV hardware** lazy poster decode, `content-visibility`, memoized
  tiles, GPU-only focus animation, single-chunk build (~52 kB gzip).

## Exports

| Export | What |
| ------ | ---- |
| `mountTv(props?)` | Render the TV app into `#root`. |
| `TvApp` / `TvAppProps` | The root React component. |
| `useFocusNav` | Spatial remote-navigation hook. |
| `@luma/tv/tv.css` | TV stylesheet (focus rings, rails, 10-foot scale). |

## Develop

Run any TV shell in a desktop browser **arrow keys + Enter act as the remote**:

```bash
bun run dev:tizen     # :5174   Samsung
bun run dev:webos     # :5175   LG
```

## See also

- [`@luma/core`](../core/README.md) · [`@luma/ui`](../ui/README.md)
- [Samsung Tizen client](../../clients/tizen/README.md) · [LG webOS client](../../clients/webos/README.md)
