#!/usr/bin/env bash
# Fetch the pinned self-contained mpv (pkgforge-dev "anylinux" AppImage) that the
# Linux packages bundle as the `kroma-mpv` Tauri sidecar (externalBin, see
# src-tauri/tauri.linux.conf.json). SteamOS ships no mpv and its rootfs is
# read-only, so KROMA carries its own; named kroma-mpv so the .deb never collides
# with a system mpv package.
#
# Idempotent: skips the download when the file is already present with the right
# sha256. Bump VERSION_TAG + ASSET + SHA256 together to upgrade.
set -euo pipefail

VERSION_TAG="v0.41.0@2026-07-01_1782914175"
ASSET="mpv-v0.41.0-anylinux-x86_64.AppImage"
SHA256="9ba489eb78c39fa4d5ef9cfaf9e80b92dcb9f69a05dd365d30255e6dca3c8fbd"

DIR="$(cd "$(dirname "$0")/.." && pwd)"
# Tauri externalBin naming: `bin/kroma-mpv` in the config resolves to this
# triple-suffixed file on disk; it installs as plain `kroma-mpv` next to the
# KROMA binary ($APPDIR/usr/bin in the AppImage, /usr/bin from the .deb).
OUT="$DIR/src-tauri/bin/kroma-mpv-x86_64-unknown-linux-gnu"

# Portable across GNU coreutils (CI) and BSD/macOS.
sum_of() {
  if command -v shasum >/dev/null 2>&1; then shasum -a 256 "$1"; else sha256sum "$1"; fi | awk '{print $1}'
}
sum_ok() { [ -f "$1" ] && [ "$(sum_of "$1")" = "$SHA256" ]; }

if sum_ok "$OUT"; then
  echo "fetch-mpv: $ASSET already present, sha256 OK"
  exit 0
fi

mkdir -p "$(dirname "$OUT")"
URL="https://github.com/pkgforge-dev/mpv-AppImage/releases/download/${VERSION_TAG//@/%40}/$ASSET"
echo "fetch-mpv: downloading $ASSET"
curl -fsSL --retry 3 -o "$OUT.tmp" "$URL"
if ! sum_ok "$OUT.tmp"; then
  echo "fetch-mpv: sha256 MISMATCH for $ASSET" >&2
  rm -f "$OUT.tmp"
  exit 1
fi
chmod +x "$OUT.tmp"
mv "$OUT.tmp" "$OUT"
echo "fetch-mpv: bundled mpv ready at $OUT"
