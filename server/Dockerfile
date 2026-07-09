# syntax=docker/dockerfile:1
#
# One image that runs the LUMA server: the Rust API + streaming server AND serves
# the built web SPA on the same port (same single-process model as the .spk).
#
# BUILD FROM THE REPO ROOT (the server include_str!s the shared locale catalogs
# under packages/, and the web SPA lives under clients/):
#     docker build -f server/Dockerfile -t luma .
#
# Run: bind-mount media read-only, a data volume, point LUMA_MEDIA_DIRS at it:
#     docker run -p 4040:4040 -v luma-data:/data -v /media:/media:ro \
#       -e LUMA_MEDIA_DIRS=/media ghcr.io/maxscharwath/luma:latest

# ---- 1) Web SPA (node + bun) -----------------------------------------------
# Node base (not the bun-only image): the TanStack Start prerender spawns a `node`
# render server and the `build:web` script's precompress step calls `node`, both
# absent from oven/bun. bun does the workspace install + runs the scripts.
FROM node:22-bookworm-slim AS web
RUN npm install -g bun@1.3.14
WORKDIR /app
COPY . .
# TanStack Start's SPA prerender spawns a render server on `localhost`; in a
# container node resolves that to IPv6 (::1) while the crawler connects to IPv4
# (127.0.0.1) -> ECONNREFUSED. Force IPv4 loopback so both agree.
RUN bun install --frozen-lockfile \
    && NODE_OPTIONS=--dns-result-order=ipv4first bun run build:web
# → /app/clients/web/dist/client

# ---- 2) Rust server --------------------------------------------------------
FROM rust:1.96-bookworm AS builder
WORKDIR /src
# Warm the dependency cache (the default `whisper-local` feature pulls the candle
# stack, which is slow) before copying sources so a code-only change reuses it.
COPY server/Cargo.toml server/Cargo.lock ./server/
# The workspace members (crates/*) must be present for cargo to resolve the
# dependency graph, otherwise the stub-main build below fails to solve.
COPY server/crates ./server/crates
RUN cd server \
    && mkdir -p src && echo 'fn main() {}' > src/main.rs \
    && cargo build --release \
    && rm -rf src
# Real sources: the server crate + the shared locale catalogs it include_str!s at
# compile time (server/src/i18n.rs -> ../../packages/core/src/locales/*.json).
COPY packages ./packages
COPY server ./server
RUN cd server && touch src/main.rs && cargo build --release
# → /src/server/target/release/luma-server

# ---- 3) Runtime ------------------------------------------------------------
FROM debian:bookworm-slim AS runtime
# ffmpeg = ffprobe (metadata) + remux/HLS; curl = TMDB lookups + artwork + the
# managed-cloudflared download; ca-certificates for outbound TLS. (openssl is
# vendored + statically linked into the binary, so no libssl runtime dep.)
RUN apt-get update \
    && apt-get install -y --no-install-recommends ffmpeg curl ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /src/server/target/release/luma-server /usr/local/bin/luma-server
COPY --from=web /app/clients/web/dist/client /app/web

# Defaults; override at runtime. Media dirs are intentionally unset (bind-mount +
# LUMA_MEDIA_DIRS / LUMA_MOVIES_DIRS / LUMA_SERIES_DIRS).
ENV LUMA_HOST=0.0.0.0 \
    LUMA_PORT=4040 \
    LUMA_DATA_DIR=/data \
    LUMA_WEB_DIR=/app/web \
    RUST_LOG=luma_server=info,tower_http=info

VOLUME ["/data"]
EXPOSE 4040
ENTRYPOINT ["luma-server"]
