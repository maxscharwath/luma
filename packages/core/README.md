<div align="center">
  <img src="../../.github/assets/logo.svg" alt="LUMA" height="56">
  <h1>@luma/core</h1>
  <p><i>Shared, framework-agnostic core for every LUMA client.</i></p>
</div>

> Part of the [LUMA](../../README.md) monorepo. Zero UI, zero framework just the
> API contract, media types, and the playback/runtime logic that every shell
> (web, Tizen, webOS) depends on. **Write it once, run it everywhere.**

## Install

```ts
// workspace dependency already wired in clients
import { LumaClient, canDirectPlay, detectCapabilities } from '@luma/core';
```

Pure TypeScript, no build step (consumed as source via the workspace), `react`-free.

## What's inside

| Module | Exports | Purpose |
| ------ | ------- | ------- |
| `api` | `LumaClient`, `LumaApiError`, `LumaClientOptions` | Typed REST client for the server (`/movies`, `/shows`, `/items`, `/stream`, `/hls`, `/metadata`, auth, Quick Connect, progress…). |
| `types` | `MediaItem`, `Show`, `ShowDetail`, `Season`, `Library`, `VideoTrack`, `AudioTrack`, `SubtitleTrack`, `Metadata`, `CastMember`, `User`, `Permission`, `hasPermission`, … | The complete API data model, shared with the Rust server's JSON. |
| `hevc` | `detectCapabilities`, `capabilities`, `canDirectPlay`, `audioSupport` | **Capability detection** what this device can decode (HEVC 10-bit/HDR, AV1, AC3/EAC3/DTS) and whether a given item direct-plays. |
| `player` | `attachDirectPlay`, `formatRuntime` | Wire a `MediaItem` to a `<video>` element for direct-play streaming. |
| `remote` | `resolveRemoteKey`, `registerTvMediaKeys`, `RemoteKey` | Normalize TV remote / keyboard input into semantic keys (`back`, `play`, colour buttons, D-pad). |
| `discover` | `discoverServer`, `subnetCandidates`, `getLocalIPv4` | Zero-config LAN discovery (mDNS candidates + `/24` subnet scan TVs can't resolve `.local`). |
| `events` | `LumaEvents`, `ServerEvent` | Reconnecting WebSocket to `/api/events` for live scan/enrich/library updates. |
| `session` | `loadSession`, `saveSession`, `clearSession`, `loadAccounts`, `forgetAccount` | Persisted auth sessions + multi-account storage. |
| `format` | `metaLine`, `qualityBadge`, `codecLabel`, `langCode`, `formatTimecode`, `channelLabel`, `posterColors` | Brand-consistent text formatting (e.g. `2024 · 2h08 · Thriller`, `4K HDR`, `H.265`). |
| `subtitles` | `parseVtt`, `activeCueText`, `isTextSubtitle`, `Cue` | Minimal WebVTT parsing + cue lookup for the custom subtitle layer. |

## Direct-play, in one decision

The heart of LUMA: the server never transcodes video, so the client decides up
front whether a title will play.

```ts
import { capabilities, canDirectPlay, audioSupport } from '@luma/core';

const caps = capabilities();              // cached device probe
const verdict = canDirectPlay(item, caps);

if (verdict.canDirectPlay) {
  // stream client.streamUrl(item.id) straight into <video>
} else if (audioSupport(item, caps).canPlay === false) {
  // video decodes but audio (AC3/EAC3/DTS) doesn't → use the audio-only HLS path
  // client.hlsAudioUrl(item.id) server copies video, re-encodes audio to AAC
}
```

`detectCapabilities()` probes HEVC (incl. Main 10 / HDR), AV1, VP9 and the audio
codecs the platform can decode, so TVs (hardware HEVC + AC3) and browsers (HEVC on
Safari / HW-Chromium, no AC3) each get the right path.

## Talking to the server

```ts
import { LumaClient } from '@luma/core';

const client = new LumaClient({ baseUrl: 'http://nas.local:4040' });

const movies  = await client.movies();
const show    = await client.show(id);          // seasons + episodes
const url     = client.streamUrl(item.id);      // range-streamed original
const poster  = client.posterFor(item);         // resolved TMDB/cached art
```

## See also

- [`@luma/ui`](../ui/README.md) design-system components built on these types
- [`@luma/tv`](../tv/README.md) the 10-foot experience that ties it together
- [server/README.md](../../server/README.md) the API this client speaks to
