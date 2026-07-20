# syntax=docker/dockerfile:1
#
# One image that runs the KROMA server: the Rust API + streaming server AND serves
# the built web SPA on the same port (same single-process model as the .spk).
#
# BUILD FROM THE REPO ROOT (the server include_str!s the shared locale catalogs
# under packages/, and the web SPA lives under clients/):
#     docker build -f server/Dockerfile -t kroma .
#
# Run: a data volume + media bind-mounts, KROMA_MEDIA_DIRS pointing at them.
# Mount media READ-WRITE if you use the built-in requests/downloads (the import
# step writes into the library); :ro is fine for a purely pre-existing library:
#     docker run -p 4040:4040 -v kroma-data:/data -v /media:/media \
#       -e KROMA_MEDIA_DIRS=/media ghcr.io/maxscharwath/kroma:latest

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
# The workspace members must be present for cargo to resolve the dependency
# graph, otherwise the stub-main build below fails to solve. Modules (crates
# under modules/<id>/server) also include_str! their module.json at compile time,
# so copy the whole modules/ tree (its ui/ dirs are ignored by cargo).
COPY server/crates ./server/crates
COPY server/modules ./server/modules
# kroma-engine (a workspace member, so it's compiled by the warm-up build below)
# include_str!s the shared locale catalogs at compile time
# (crates/kroma-engine/src/i18n.rs -> ../../../packages/core/src/locales/*.json),
# so they must exist before this build. Copy just the catalogs here they change
# rarely, so the expensive dependency cache stays warm; the rest of packages/
# lands with the real sources below.
COPY packages/core/src/locales ./packages/core/src/locales
WORKDIR /src/server
RUN mkdir -p src && echo 'fn main() {}' > src/main.rs \
    && cargo build --locked --release \
    && rm -rf src
# Real sources: the server crate + the shared locale catalogs it include_str!s at
# compile time (crates/kroma-engine/src/i18n.rs -> ../../../packages/core/src/locales/*.json).
WORKDIR /src
COPY packages ./packages
COPY server ./server
WORKDIR /src/server
RUN touch src/main.rs && cargo build --locked --release
# → /src/server/target/release/kroma-server

# ---- 3) Runtime ------------------------------------------------------------
FROM debian:bookworm-slim AS runtime
# ffmpeg = ffprobe (metadata) + remux/HLS; curl = TMDB lookups + artwork + the
# managed-cloudflared download; ca-certificates for outbound TLS. (openssl is
# vendored + statically linked into the binary, so no libssl runtime dep.)
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl ffmpeg \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /src/server/target/release/kroma-server /usr/local/bin/kroma-server
COPY --from=web /app/clients/web/dist/client /app/web

# Defaults; override at runtime. Media dirs are intentionally unset (bind-mount +
# KROMA_MEDIA_DIRS / KROMA_MOVIES_DIRS / KROMA_SERIES_DIRS).
ENV KROMA_HOST=0.0.0.0 \
    KROMA_PORT=4040 \
    KROMA_DATA_DIR=/data \
    KROMA_WEB_DIR=/app/web \
    RUST_LOG=kroma_server=info,tower_http=info

VOLUME ["/data"]
EXPOSE 4040
ENTRYPOINT ["kroma-server"]
