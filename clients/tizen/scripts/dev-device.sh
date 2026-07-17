#!/usr/bin/env bash
# KROMA Tizen on-device live-dev: serve the TV shell over the LAN with Vite HMR so a
# real Samsung TV hot-reloads on save. Run from the repo root:
#
#     bun run dev:tizen:device
#
# One-time pairing (installs the shell that loads this dev server):
#
#     cd clients/tizen && make dev-shell TV_IP=<tv-ip>
#
# The KROMA server must be running and reachable on the LAN (it binds 0.0.0.0:4040):
#
#     bun run server:watch      # in another terminal
set -euo pipefail

# This machine's LAN IPv4 the TV connects back to for the app, the HMR socket, and
# the API. Detected by lan-ip.sh, the same source of truth make dev-shell uses, so
# the baked-in address and the HMR address match. Override with KROMA_TV_HOST.
HOST_IP="${KROMA_TV_HOST:-$("$(dirname "$0")/lan-ip.sh")}"
if [ -z "$HOST_IP" ]; then
  echo "dev-device: could not detect a LAN IP." >&2
  echo "  Set it explicitly:  KROMA_TV_HOST=192.168.1.20 bun run dev:tizen:device" >&2
  exit 1
fi

echo "TV dev server → http://$HOST_IP:5174/   (seeds API → http://$HOST_IP:4040)"
echo "Needs: KROMA server running (bun run server:watch) + dev shell installed (make dev-shell)."

export KROMA_TV_DEVICE=1
export KROMA_TV_HOST="$HOST_IP"
# Seed a fresh dev shell's initial server to this machine so it finds the API on
# first launch (the TV can't use localhost). Respects an explicit override.
export VITE_KROMA_SERVER="${VITE_KROMA_SERVER:-http://$HOST_IP:4040}"

exec bun run --filter '@kroma/tizen' dev
