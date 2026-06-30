# @luma/web

> Part of the [LUMA](../../README.md) monorepo the desktop browser client.

LUMA desktop/browser client. Vite + React + TypeScript, composed from `@luma/ui`
(design system) and `@luma/core` (API client, HEVC capability detection, direct-play).

## Develop

```bash
# from the repo root
bun install
bun run server          # start the Rust media server on :4040 (separate terminal)
bun run dev:web         # Vite dev server on http://localhost:5173
```

The client auto-targets `http://<host>:4040`. Point it elsewhere with
`VITE_LUMA_SERVER` (see `.env.example`) or the in-app connection screen.

## Build

```bash
bun run build:web       # tsc typecheck + vite build → clients/web/dist
```

## Playback

Playback is **direct-play**: the `<video>` element streams the original file from
`/api/items/:id/stream` (HTTP range) and the browser decodes it. HEVC plays
directly on Safari and on Chromium builds with hardware HEVC; the sidebar shows
this device's decode capabilities, and each title reports whether direct-play is
supported before you hit Lecture.
