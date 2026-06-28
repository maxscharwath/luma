#!/usr/bin/env bash
# Build a single self-contained LUMA Synology package (.spk) for x86_64 DSM 7.
#
# Produces ONE installable package containing:
#   • luma-server      — Rust API + streaming, AND serves the web SPA (one process)
#   • web/             — the built static web SPA (served on the same origin)
#   • ffmpeg/          — static ffmpeg + ffprobe (scan + audio HLS remux)
#
# The Rust binary is cross-compiled to x86_64-unknown-linux-musl (fully static,
# runs on every x86_64 DSM 7 model) inside a Docker musl image — no host Rust
# cross-toolchain needed. Run this on your Mac/Linux dev machine; install the
# resulting .spk on the NAS via Package Center → Manual Install.
#
# Usage:   clients/synology/build.sh [version]
# Env:     RUST_IMAGE   musl cross image (default messense/rust-musl-cross:x86_64-musl)
#          SKIP_RUST=1  reuse an existing musl binary (faster re-packaging)
#          SKIP_WEB=1   reuse an existing web build
set -euo pipefail

VERSION="${1:-0.1.0}"
ARCH="x86_64"
TARGET="x86_64-unknown-linux-musl"
RUST_IMAGE="${RUST_IMAGE:-messense/rust-musl-cross:x86_64-musl}"
FFMPEG_URL="https://johnvansickle.com/ffmpeg/releases/ffmpeg-release-amd64-static.tar.xz"

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SKEL="$ROOT/clients/synology/spk"
OUT="$ROOT/clients/synology/dist"
WORK="$(mktemp -d)"
CACHE="$ROOT/clients/synology/.cache"
trap 'rm -rf "$WORK"' EXIT
mkdir -p "$OUT" "$CACHE"

say() { printf '\033[1;33m▶ %s\033[0m\n' "$*"; }

# 1) Web SPA -----------------------------------------------------------------
if [ "${SKIP_WEB:-}" != "1" ]; then
  say "Building web SPA"
  ( cd "$ROOT" && bun run build:web )
fi
[ -f "$ROOT/clients/web/dist/client/_shell.html" ] || { echo "web build missing (_shell.html)"; exit 1; }

# 2) Rust server → static musl binary ----------------------------------------
BIN="$ROOT/server/target/$TARGET/release/luma-server"
if [ "${SKIP_RUST:-}" != "1" ]; then
  command -v docker >/dev/null || { echo "Docker required for the musl cross-build (or set SKIP_RUST=1 after a manual build)"; exit 1; }
  say "Cross-compiling luma-server → $TARGET (Docker: $RUST_IMAGE)"
  docker run --rm -v "$ROOT/server":/home/rust/src -v "$CACHE/cargo":/root/.cargo/registry \
    "$RUST_IMAGE" cargo build --release --target "$TARGET"
fi
[ -f "$BIN" ] || { echo "musl binary missing: $BIN"; exit 1; }

# 3) Static ffmpeg + ffprobe (x86_64) ----------------------------------------
FF="$CACHE/ffmpeg-amd64-static"
if [ ! -x "$FF/ffmpeg" ]; then
  say "Fetching static ffmpeg/ffprobe"
  curl -fSL "$FFMPEG_URL" -o "$WORK/ff.tar.xz"
  mkdir -p "$FF" && tar xJf "$WORK/ff.tar.xz" -C "$FF" --strip-components=1
fi

# 4) Stage the payload (→ /var/packages/luma/target) -------------------------
say "Staging payload"
PAY="$WORK/payload"
mkdir -p "$PAY/bin" "$PAY/web" "$PAY/ffmpeg"
install -m755 "$BIN" "$PAY/bin/luma-server"
cp -R "$ROOT/clients/web/dist/client/." "$PAY/web/"
install -m755 "$FF/ffmpeg" "$PAY/ffmpeg/ffmpeg"
install -m755 "$FF/ffprobe" "$PAY/ffmpeg/ffprobe"
# Strip macOS xattrs (com.apple.provenance etc.) so bsdtar doesn't embed SCHILY.xattr
# pax records that DSM's busybox tar can reject. COPYFILE_DISABLE stops AppleDouble.
xattr -cr "$PAY" 2>/dev/null || true
( cd "$PAY" && COPYFILE_DISABLE=1 tar --no-mac-metadata --no-xattrs --no-acls -czf "$WORK/package.tgz" . )

# 5) Assemble the .spk -------------------------------------------------------
say "Assembling .spk"
SPK="$WORK/spk"
mkdir -p "$SPK"
cp -R "$SKEL/scripts" "$SKEL/conf" "$SKEL/WIZARD_UIFILES" "$SPK/"
chmod 755 "$SPK/scripts/"*
cp "$WORK/package.tgz" "$SPK/package.tgz"
EXT_SIZE="$(gzip -dc "$WORK/package.tgz" | wc -c | tr -d ' ')"
sed -e "s/@VERSION@/$VERSION/g" -e "s/@ARCH@/$ARCH/g" -e "s/@SIZE@/$EXT_SIZE/g" \
  "$SKEL/INFO.template" > "$SPK/INFO"
# Icons: the LUMA brand mark (gold ring + dot), checked in alongside the skeleton.
cp "$SKEL/PACKAGE_ICON.PNG" "$SKEL/PACKAGE_ICON_256.PNG" "$SPK/"

OUT_SPK="$OUT/luma-$VERSION-$ARCH.spk"
# Pristine, deterministic outer tar: INFO first (DSM reads it first), no macOS
# metadata/xattrs/AppleDouble. Members listed explicitly rather than globbed.
xattr -cr "$SPK" 2>/dev/null || true
( cd "$SPK" && COPYFILE_DISABLE=1 tar --no-mac-metadata --no-xattrs --no-acls \
    -cf "$OUT_SPK" INFO package.tgz conf scripts WIZARD_UIFILES \
    PACKAGE_ICON.PNG PACKAGE_ICON_256.PNG )
say "Done → $OUT_SPK"
ls -lh "$OUT_SPK"
shasum -a 256 "$OUT_SPK"
