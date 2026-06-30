<div align="center">

<img src=".github/assets/banner.svg" alt="LUMA self-hosted, direct-play, HEVC-first media streaming" width="100%">

<br/>

**A personal Netflix/Plex you run on your own NAS.**
Rust server · web + TV clients · one cinematic design language.

[![License: MIT](https://img.shields.io/badge/License-MIT-F4B642.svg?style=flat-square)](LICENSE)
[![Bun ≥ 1.3](https://img.shields.io/badge/Bun-%E2%89%A5%201.3-0A0A0C.svg?style=flat-square&logo=bun&logoColor=F4B642)](https://bun.sh)
[![Rust ≥ 1.85](https://img.shields.io/badge/Rust-%E2%89%A5%201.85-0A0A0C.svg?style=flat-square&logo=rust&logoColor=F4B642)](https://www.rust-lang.org)
[![TypeScript](https://img.shields.io/badge/TypeScript-strict-0A0A0C.svg?style=flat-square&logo=typescript&logoColor=3178C6)](https://www.typescriptlang.org)
[![Platforms](https://img.shields.io/badge/platforms-web%20%C2%B7%20Samsung%20%C2%B7%20LG-0A0A0C.svg?style=flat-square)](#platforms)
[![PRs welcome](https://img.shields.io/badge/PRs-welcome-F4B642.svg?style=flat-square)](CONTRIBUTING.md)

</div>

---

LUMA is a self-hosted, multi-platform **video streaming** stack. It scans your
media library (Plex-style **movie / TV-show detection**, grouping episodes into
shows → seasons), persists it in **SQLite**, enriches it from **TMDB**, and
streams the original files to a web app and to your living-room TV all wrapped
in one calm, cinematic, amber-on-charcoal design language.

> **Playback is direct-play, HEVC-first.** The server never transcodes video: it
> **range-streams the original files** and every client decodes HEVC/H.265 (incl.
> 10-bit / HDR) natively Samsung & LG TVs in hardware, modern browsers where
> supported so your NAS CPU stays idle. The one exception is an **audio-only**
> HLS path for browsers that can't decode AC3/EAC3/DTS (video is copied, only the
> audio is re-encoded to stereo AAC).

<div align="center">

<table>
  <tr>
    <td width="50%" valign="top">
      <img src="design/screenshots/home.jpg" alt="Home cinematic hero + rails" width="100%"><br/>
      <sub><b>Web home</b> · full-bleed hero + horizontal rails</sub>
    </td>
    <td width="50%" valign="top">
      <img src="design/screenshots/tv-detail.jpg" alt="TV 10-foot detail page" width="100%"><br/>
      <sub><b>TV detail</b> · 10-foot, remote-driven spatial focus</sub>
    </td>
  </tr>
  <tr>
    <td width="50%" valign="top">
      <img src="design/screenshots/films.jpg" alt="Films grid" width="100%"><br/>
      <sub><b>Films</b> · library grid</sub>
    </td>
    <td width="50%" valign="top">
      <img src="design/screenshots/spotlight.jpg" alt="Spotlight search" width="100%"><br/>
      <sub><b>Spotlight</b> · instant search</sub>
    </td>
  </tr>
  <tr>
    <td width="50%" valign="top">
      <img src="design/screenshots/mobile.jpg" alt="Mobile layout" width="100%"><br/>
      <sub><b>Mobile</b> · responsive design (client in progress)</sub>
    </td>
    <td width="50%" valign="top">
      <img src="design/screenshots/requests.jpg" alt="Requests" width="100%"><br/>
      <sub><b>Requests</b> · ask for a title</sub>
    </td>
  </tr>
</table>

</div>

## Features

- **Direct-play, HEVC-first** original files are range-streamed; clients decode
  HEVC/H.265, AV1, H.264 themselves. No transcode pipeline, no hot NAS.
- **Plex-style library scan** detects movies vs. TV shows, parses `S01E02` /
  `1x02` / multi-episode markers, strips release junk from titles, groups shows →
  seasons → episodes. Hardened against 4000+ real-world filenames.
- **TMDB metadata + artwork** overviews, posters, backdrops, genres, ratings,
  keywords, IMDb IDs; cached to disk as WebP. Works out of the box with a built-in key.
- **Smart, automatic home** the server assembles the home screen: For You,
  "because you watched…", themed/seasonal rows, trending and recently-added built
  from on-device content embeddings + watch history. No cloud, no per-user
  training; an optional multilingual semantic model upgrades the themed rows.
- **Typo-tolerant search** full-text catalogue search over titles, cast and
  genres, tuned for imperfect input (incl. TV voice queries).
- **One design language, three shells** web (desktop), Samsung Tizen and LG
  webOS TVs share `@luma/core`, `@luma/ui` and the entire `@luma/tv` experience.
- **10-foot TV UX** spatial remote navigation, lazy poster decoding,
  `content-visibility`, memoized tiles, a single-chunk ~52 kB build. Feels like
  Netflix / Disney+.
- **Real-time sync** a WebSocket event bus pushes scan/enrich/library updates;
  posters appear live as TMDB resolves, no client relaunch.
- **Zero-config discovery** the server advertises over mDNS and clients
  subnet-scan the LAN, so TVs find it with no manual IP entry.
- **Resume, profiles & Quick Connect** picks up where you left off; TV pairs to
  an account by scanning a QR code.
- **Self-hosted & private** a single Rust binary (or Docker image) on your NAS.
  Your library never leaves your network.

## Architecture

The web, Tizen and webOS clients are **thin shells**: all UI lives in `@luma/ui`,
all logic in `@luma/core`, and the entire TV experience in `@luma/tv`. HEVC
detection and the API contract are written once.

```
luma/
├─ server/                 Rust media server (axum) Plex-style scan, SQLite, range streaming
├─ packages/
│  ├─ core/   @luma/core    API client · types · HEVC capability detection · remote map · direct-play
│  ├─ ui/     @luma/ui      design-system React components + tokens (from design/)
│  └─ tv/     @luma/tv      shared 10-foot experience (spatial focus nav, home, detail, player)
├─ clients/
│  ├─ web/    @luma/web     desktop browser shell (sidebar) TanStack Start SSR + Tailwind v4
│  ├─ tizen/  @luma/tizen   Samsung TV thin shell + config.xml → .wgt
│  └─ webos/  @luma/webos   LG TV thin shell + appinfo.json → .ipk
└─ design/                  imported design source (tokens, components, guidelines, LUMA.dc.html)
```

| Package / app | What it is | README |
| ------------- | ---------- | ------ |
| `server` | Rust media server scan, SQLite, TMDB, range/HLS streaming | [server/README.md](server/README.md) |
| `@luma/core` | API client, types, HEVC detection, remote map, direct-play | [packages/core/README.md](packages/core/README.md) |
| `@luma/ui` | Design-system React components + tokens | [packages/ui/README.md](packages/ui/README.md) |
| `@luma/tv` | Shared 10-foot TV experience | [packages/tv/README.md](packages/tv/README.md) |
| `@luma/web` | Desktop browser client | [clients/web/README.md](clients/web/README.md) |
| `@luma/tizen` | Samsung TV (Tizen) shell | [clients/tizen/README.md](clients/tizen/README.md) |
| `@luma/webos` | LG TV (webOS) shell | [clients/webos/README.md](clients/webos/README.md) |
| `design` | Design system source (tokens, guidelines) | [design/readme.md](design/readme.md) |

## Prerequisites

- **[Bun](https://bun.sh)** ≥ 1.3 package manager + runner (the repo is a Bun workspace)
- **[Rust](https://www.rust-lang.org)** ≥ 1.85 + **ffmpeg/ffprobe** for the server's metadata + HLS path
- Optional, only to package TV apps: **Tizen Studio** (Samsung) · **webOS TV CLI**
  [`@webos-tools/cli`](https://www.npmjs.com/package/@webos-tools/cli) (LG)

## Quickstart

```bash
bun install
bun run dev      # ONE command: media server (:4040) + web client (:3000) together
```

Open <http://localhost:3000>. In dev, Vite reverse-proxies `/api` to the Rust
server on :4040, so the whole app is one origin. With no media configured, the server seeds demo
titles (movies + two shows, a HEVC/HDR 4K hero among them) so the UI is populated
immediately. Point it at real media with:

```bash
LUMA_MEDIA_DIRS=/volume1/media bun run server
```

```bash
bun start        # also builds (prepares) the Tizen app first, then runs dev
```

Prefer separate terminals? `bun run server`, then `bun run dev:web`.

## Platforms

Each TV client runs in a normal desktop browser for development **arrow keys +
Enter act as the remote**:

```bash
bun run dev:tizen     # :5174   Samsung
bun run dev:webos     # :5175   LG
```

| Platform | Dev | Package & install |
| -------- | --- | ----------------- |
| **Web** (desktop browser) | `bun run dev:web` | `bun run build:web` → static/SSR bundle ([web README](clients/web/README.md)) |
| **Samsung TV** (Tizen) | `bun run dev:tizen` | `make -C clients/tizen deploy TV_IP=…` → `.wgt` ([tizen README](clients/tizen/README.md) · [SETUP](clients/tizen/SETUP.md)) |
| **LG TV** (webOS) | `bun run dev:webos` | `ares-package clients/webos/dist` → `.ipk` ([webos README](clients/webos/README.md)) |

> A **mobile** client is the next planned shell the design source already covers it.

## Build

```bash
bun run build          # all frontends + typecheck every package
bun run typecheck      # typecheck only
bun run server:build   # cargo release build

bun run build:tizen && cd clients/tizen && tizen package -t wgt -s <profile> -- dist
bun run build:webos && ares-package clients/webos/dist
```

See each client's README for full device install steps.

## Server API

`http://<host>:4040/api`:

- **Catalogue** `GET /health`, `/libraries`, `/movies`, `/shows`, `/shows/:id`
  (seasons + episodes), `/items`, `/items/:id`, `/items/:id/metadata` (TMDB), posters.
- **Streaming** `/items/:id/stream` (HTTP range), `/items/:id/hls/…` (audio-only HLS).
- **Discovery** `/search?q=` (typo-tolerant full-text), `/home` (generated
  sections), `/for-you`, `/items/:id/similar`, `/themed?q=`, `/continue`.
- **Accounts & control** `/auth/*` (incl. Quick Connect), `/progress`,
  `/admin/*`, `GET /events` (WebSocket), `POST /scan`.

Configure via `LUMA_HOST` / `LUMA_PORT` / `LUMA_MEDIA_DIRS` / `LUMA_DATA_DIR` /
`LUMA_TMDB_API_KEY`. Library persisted in SQLite (`<data>/luma.db`, WAL). Optional
semantic recommendations are a `--features semantic-embeddings` build (a BERT
sentence model in `LUMA_EMBED_MODEL_DIR`). **Full reference → [server/README.md](server/README.md).**

## Deploy on a Synology NAS

The server ships a multi-stage [Dockerfile](server/Dockerfile) (bundles ffmpeg):

```bash
docker build -t luma-server ./server
docker run -d -p 4040:4040 \
  -e LUMA_MEDIA_DIRS=/media \
  -v /volume1/video:/media:ro \
  -v luma-data:/data \
  luma-server
```

Build for the NAS CPU arch `linux/amd64` (Intel/AMD) or `linux/arm64` (ARM) via
`docker buildx`. Then point each TV/web client at `http://<nas-ip>:4040` on first
launch (or let auto-discovery find it).

## Design system

`design/` is the imported design source deep-charcoal + amber, Bricolage
Grotesque / Hanken Grotesk, French copy, no emoji. Its tokens and components are
ported into `@luma/ui`; `design/LUMA.dc.html` is the full clickable reference.

```bash
open design/LUMA.dc.html
```

More in [design/readme.md](design/readme.md).

## Contributing

Issues and PRs are welcome see [CONTRIBUTING.md](CONTRIBUTING.md) for setup,
conventions (keep clients thin), and how to report playback bugs.

## License

[MIT](LICENSE) © 2026 [Maxime Scharwath](https://github.com/maxscharwath)
