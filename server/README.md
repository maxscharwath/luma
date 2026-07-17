# KROMA Server

> Part of the [KROMA](../README.md) monorepo self-hosted, direct-play, HEVC-first media streaming.

A self-hosted, **direct-play** media streaming server for the KROMA project
think a minimal, Plex-like backend.

It does three things:

1. **Scans** one or more media library roots, detecting **movies vs TV shows**
   (Plex-style) and grouping episodes into shows → seasons → episodes.
2. **Exposes metadata** over a small JSON REST API.
3. **Range-streams the original media files** to clients.

It **never transcodes**. Playback is always direct-play: the client (web / TV)
decodes HEVC/H.265, AV1, H.264, etc. itself. `ffprobe` is used *only* to read
metadata there is no ffmpeg encode pipeline. If `ffprobe` is missing, the
server still runs and infers the codec from the file extension.

The library is persisted in **SQLite** (`<data>/kroma.db`, WAL mode). A scan
computes the full set of libraries/shows/items and atomically swaps it in; reads
run on a small connection pool. The storage layer is a hand-rolled WAL pool over
`rusqlite` (bundled SQLite) no system libsqlite3 required.

If no media dirs are configured (or a scan finds nothing), the server seeds
built-in **demo** content (movies + two shows) so clients render out of the box.
Demo items cannot be streamed (their `/stream` endpoint returns a JSON 404).

## Library detection (Plex-style)

The scanner recognises these layouts and naming cues:

- **Movies** `Movies/Blade Runner 2049 (2017)/Blade Runner 2049 (2017) 2160p BluRay x265.mkv`
  or flat `The.Matrix.1999.1080p.x264-GROUP.mp4`. A parenthesised `(YYYY)` is the
  authoritative year, so "Blade Runner 2049 (2017)" → title *Blade Runner 2049*,
  year *2017* (not 2049). Release junk (resolution / source / codec / group) is
  stripped from titles. The `Title (Year)` folder name is used when the filename
  is generic.
- **Episodes** `S01E02`, `s1e2`, `S01E02-E03` (multi-episode), `1x02`. The
  **top-level folder under the library root** is the show identity
  (`TV Shows/The Office (2005)/Season 02/The Office - S02E01 - The Dundies.mkv`),
  with the text after the marker becoming the episode title. Episodes are grouped
  into shows and seasons.

A library's `kind` (`movies` / `shows` / `mixed`) is derived from what it holds.

## Quickstart

```bash
# From server/ runs with demo content (no media dirs configured):
cargo run

# Point it at real libraries (OS path-separator OR comma separated):
KROMA_MEDIA_DIRS="/mnt/movies:/mnt/tv" cargo run

# Then, in another shell:
curl -s http://localhost:4040/api/health | jq
curl -s http://localhost:4040/api/items  | jq
```

The server logs the bound address on startup and listens on
`http://0.0.0.0:4040` by default.

## Configuration

All configuration is via environment variables:

| Variable           | Default     | Description                                                            |
| ------------------ | ----------- | --------------------------------------------------------------------- |
| `KROMA_HOST`        | `0.0.0.0`   | Interface to bind.                                                     |
| `KROMA_PORT`        | `4040`      | TCP port to listen on.                                                 |
| `KROMA_MEDIA_DIRS`  | *(empty)*   | Library roots to scan. OS-path-separator (`:` / `;`) or comma list.    |
| `KROMA_DATA_DIR`    | `./data`    | Where the SQLite database (`kroma.db`) lives.                          |
| `KROMA_TMDB_API_KEY`| *(empty)*   | TMDB API key → enables movie/show metadata. Unset = feature off.       |
| `KROMA_TMDB_LANGUAGE`| `en-US`    | TMDB language for titles/overviews, e.g. `fr-FR`.                      |
| `RUST_LOG`         | `info`      | Standard `tracing` filter, e.g. `kroma_server=debug`.                  |

## Data model

`MediaItem`:

```jsonc
{
  "id": "string",
  "title": "string",
  "kind": "movie" | "episode" | "video",
  "year": 2017,
  "durationMs": 9780000,
  "container": "mkv",
  "video": { "codec": "hevc", "width": 3840, "height": 2160, "hdr": true, "bitDepth": 10 },
  "audio": { "codec": "truehd", "channels": 8, "language": "eng" },
  "subtitles": [ { "language": "eng", "codec": "subrip" } ],
  "library": "string",
  // show/episode fields null for movies:
  "showId": "string", "showTitle": "The Office",
  "season": 2, "episode": 1, "episodeEnd": null, "episodeTitle": "The Dundies",
  "relPath": "The Office (2005)/Season 02/The Office - S02E01.mkv" /* or null for demo items */,
  "addedAt": "2026-06-27T12:00:00Z"
}
```

`Show` (an aggregate built by grouping episodes; `GET /api/shows/:id` returns it
with a `seasons` array of `{ number, episodes }`):

```jsonc
{ "id": "string", "title": "The Office", "year": 2005, "library": "string",
  "seasonCount": 1, "episodeCount": 2,
  "video": { "codec": "h264", "width": 1280, "height": 720, "hdr": false, "bitDepth": 8 },
  "addedAt": "2026-06-27T12:00:00Z" }
```

`Library`:

```jsonc
{ "id": "string", "name": "Movies", "kind": "movies" | "shows" | "mixed", "path": "/mnt/movies", "itemCount": 42 }
```

Codec strings are normalized lowercase: `hevc`, `h264`, `av1`, `vp9`, `aac`,
`eac3`, `ac3`, `dts`, etc.

## API

All routes are prefixed with `/api`. CORS is permissive (self-hosted LAN use).

| Method | Path                       | Description                                   |
| ------ | -------------------------- | --------------------------------------------- |
| GET    | `/api/health`              | Health + ffprobe presence + counts.           |
| GET    | `/api/libraries`           | All scanned libraries.                        |
| GET    | `/api/items`               | All playable items (`?library=<id>` filter).  |
| GET    | `/api/movies`              | Movies only (`?library=<id>` to filter).      |
| GET    | `/api/shows`               | TV shows (`?library=<id>` to filter).         |
| GET    | `/api/shows/:id`           | One show + `seasons[]` of `{ number, episodes }`. |
| GET    | `/api/shows/:id/poster`    | Deterministic SVG show poster.                |
| GET    | `/api/shows/:id/metadata`  | TMDB details + IDs for the show.              |
| GET    | `/api/items/:id`           | One item movie or episode (404 if missing). |
| GET    | `/api/items/:id/stream`    | Range-streamed original file.                 |
| GET    | `/api/items/:id/poster`    | Deterministic SVG placeholder poster.         |
| GET    | `/api/items/:id/metadata`  | TMDB details + IDs (episode → parent show).   |
| POST   | `/api/scan`                | Rescan all dirs.                              |

## Metadata (TMDB)

Items are enriched from [TMDB](https://www.themoviedb.org). KROMA ships a built-in
application key (`BUILTIN_TMDB_API_KEY` in `src/config.rs`) so this works out of
the box with no per-install token the same approach Overseerr/Jellyseerr/Seerr
take. Override it for your own install with `KROMA_TMDB_API_KEY`.

The server resolves a movie/show by its parsed title + year, then returns the
overview, poster/backdrop URLs, genres, rating, and both the **TMDB** and
**IMDb** IDs (via TMDB's `external_ids`). Lookups are cached in memory.

No new dependency is pulled in: like `ffprobe` for media metadata, the lookup
shells out to `curl` (HTTPS). With no key set, the `/metadata` routes return
`503` and the rest of the server is unaffected.

```jsonc
// GET /api/items/<id>/metadata
{
  "provider": "tmdb",
  "tmdbId": 542178,
  "imdbId": "tt8847712",
  "title": "The French Dispatch",
  "tagline": "Read all about it.",
  "overview": "…",
  "releaseDate": "2021-10-21",
  "genres": ["Comedy", "Drama"],
  "rating": 7.4,
  "posterUrl": "https://image.tmdb.org/t/p/w500/….jpg",
  "backdropUrl": "https://image.tmdb.org/t/p/w1280/….jpg",
  "tmdbUrl": "https://www.themoviedb.org/movie/542178"
}
```

### Examples

```bash
# Health
curl -s http://localhost:4040/api/health
# {"status":"ok","version":"0.1.0","ffprobe":true,"libraries":2,"items":10,"shows":2}

# Libraries
curl -s http://localhost:4040/api/libraries

# Movies and shows (optionally ?library=<id>)
curl -s http://localhost:4040/api/movies
curl -s http://localhost:4040/api/shows

# One show with its seasons + episodes
curl -s http://localhost:4040/api/shows/<showId> | jq

# All items (movies + episodes), or filtered by library id
curl -s http://localhost:4040/api/items
curl -s "http://localhost:4040/api/items?library=<libraryId>"

# One item (movie or episode)
curl -s http://localhost:4040/api/items/<id>

# Poster (SVG)
curl -s http://localhost:4040/api/items/<id>/poster -o poster.svg

# Metadata (needs KROMA_TMDB_API_KEY)
curl -s http://localhost:4040/api/items/<id>/metadata | jq

# Stream full file
curl -s http://localhost:4040/api/items/<id>/stream -o out.mkv

# Stream byte range (note the 206 + Content-Range)
curl -s -D - -H "Range: bytes=0-1048575" \
  http://localhost:4040/api/items/<id>/stream -o /dev/null

# Rescan
curl -s -X POST http://localhost:4040/api/scan
# {"scanned":10,"libraries":2,"shows":2}
```

## Docker

```bash
# Build
docker build -t kroma-server .

# Run, mounting a media folder read-only and publishing the port.
docker run --rm \
  -p 4040:4040 \
  -e KROMA_MEDIA_DIRS=/media/movies \
  -v /path/on/host/movies:/media/movies:ro \
  -v kroma-data:/data \
  kroma-server
```

The runtime image installs `ffmpeg` (which provides `ffprobe`).

### Synology / NAS

Build or pull the image for your NAS CPU architecture `linux/amd64` for
Intel/AMD models, `linux/arm64` for ARM models:

```bash
docker buildx build --platform linux/arm64 -t kroma-server:arm64 .
```

Bind-mount your shared folders and point `KROMA_MEDIA_DIRS` at the in-container
paths, exactly as in the `docker run` example above.
