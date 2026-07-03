#!/usr/bin/env bash
# LUMA Steam Deck kiosk launcher.
#
# Serves the built Steam Deck client over http and opens it fullscreen in Chromium
# with the flags SteamOS needs for AMD (RDNA2 / Van Gogh) hardware video decode via
# VA-API. Add this script to Steam as a "Non-Steam Game" so it launches straight into
# Gaming Mode.
#
# The client MUST be served over http, not opened via file:// - a Vite build ships ES
# modules and browsers refuse to load `type="module"` scripts from a file:// origin.
# By default this script hosts the bundled `dist/` itself (python3, which SteamOS
# ships) so the Deck needs nothing else running.
#
# Usage:
#   ./luma-kiosk.sh                                   # serve ../dist locally, open it
#   LUMA_DIR=/home/deck/luma-deck ./luma-kiosk.sh     # serve a dist elsewhere
#   LUMA_URL=http://192.168.1.50:8080/ ./luma-kiosk.sh  # already hosted; just open it
#
# NOTE: verify HEVC-over-VA-API on a real Deck (see README "HEVC decode"). If HEVC
# fails, the app falls back to a server remux; check chrome://gpu + media-internals.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PORT="${LUMA_PORT:-8791}"

# Resolve a Chromium/Chrome binary (Flatpak is the usual SteamOS install).
if command -v flatpak >/dev/null 2>&1 && flatpak info com.google.Chrome >/dev/null 2>&1; then
  CHROME=(flatpak run com.google.Chrome)
elif command -v flatpak >/dev/null 2>&1 && flatpak info org.chromium.Chromium >/dev/null 2>&1; then
  CHROME=(flatpak run org.chromium.Chromium)
elif command -v google-chrome-stable >/dev/null 2>&1; then
  CHROME=(google-chrome-stable)
elif command -v chromium >/dev/null 2>&1; then
  CHROME=(chromium)
else
  echo "No Chromium/Chrome found. Install one from Discover (Desktop mode), e.g. com.google.Chrome." >&2
  exit 1
fi

SERVER_PID=""
cleanup() { [ -n "$SERVER_PID" ] && kill "$SERVER_PID" 2>/dev/null || true; }
trap cleanup EXIT

if [ -n "${LUMA_URL:-}" ]; then
  URL="$LUMA_URL"
else
  # Host the built bundle locally over http (default: the dist beside this script).
  DIR="${LUMA_DIR:-$HERE/../dist}"
  if [ ! -f "$DIR/index.html" ]; then
    echo "No built client at $DIR. Run 'bun run build:steamdeck' first, or set LUMA_DIR / LUMA_URL." >&2
    exit 1
  fi
  ( cd "$DIR" && exec python3 -m http.server "$PORT" --bind 127.0.0.1 ) >/dev/null 2>&1 &
  SERVER_PID=$!
  URL="http://127.0.0.1:$PORT/"
  sleep 1
fi

# A dedicated profile dir forces a fresh Chromium instance, so the GPU/VA-API flags
# actually apply (an already-running Chrome would ignore them) and there is no
# "restore pages" prompt on a kiosk.
"${CHROME[@]}" \
  --user-data-dir="$HOME/.luma-deck-chrome" \
  --kiosk \
  --app="$URL" \
  --start-fullscreen \
  --ozone-platform=wayland \
  --enable-features=VaapiVideoDecoder,VaapiVideoDecodeLinuxGL,AcceleratedVideoDecodeLinuxGL,PlatformHEVCDecoderSupport \
  --disable-features=UseChromeOSDirectVideoDecoder \
  --ignore-gpu-blocklist \
  --autoplay-policy=no-user-gesture-required \
  --overscroll-history-navigation=0 \
  --no-first-run \
  --no-default-browser-check \
  --disable-pinch
