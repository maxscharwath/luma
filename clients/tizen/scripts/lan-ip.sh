#!/usr/bin/env bash
# Print this machine's LAN IPv4 the TV connects back to (dev server + HMR socket +
# API). Single source of truth for the Makefile (HOST_IP) and dev-device.sh, so the
# address baked into the installed dev shell and the one Vite's HMR points at can't
# drift. Keep vite.config.ts lanIp() (the fallback used only when KROMA_TV_HOST is
# unset) in sync. macOS: primary interface en0, then en1; callers override with
# KROMA_TV_HOST if it picks the wrong one.
ipconfig getifaddr en0 2>/dev/null || ipconfig getifaddr en1 2>/dev/null || true
