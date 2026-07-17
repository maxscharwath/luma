#!/usr/bin/env bash
# Repair a Tauri-built AppImage for distros newer than the build runner.
#
# Tauri's default AppImage bundling (linuxdeploy) sweeps the runner's
# Wayland/GLib/GStreamer infra libs into usr/lib - its excludelist misses them,
# though the community pkg2appimage excludelist drops them for exactly this
# reason: the runner's stale libwayland-client loaded against a newer host Mesa
# (SteamOS 3.7+, Fedora 41+, Ubuntu 26.04) kills EGL ("Could not create default
# EGL display: EGL_BAD_PARAMETER"), WebKit aborts, and the window never appears
# (verified on the Steam Deck). The system copies are drop-in compatible, so
# stripping them and repacking yields a working AppImage. Upstream:
# tauri-apps/tauri#15665 - no supported exclude knob yet; their rewritten
# bundler (PR #12491) is still experimental, so post-processing it is.
#
# Usage: fix-appimage.sh <path/to/App.AppImage>
# The file is modified IN PLACE; any existing .sig becomes stale, so re-sign
# afterwards wherever updater artifacts matter (desktop-autoupdate.yml does).
set -euo pipefail

[ $# -eq 1 ] || { echo "usage: fix-appimage.sh <AppImage>" >&2; exit 2; }
APPIMAGE="$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"
[ -f "$APPIMAGE" ] || { echo "fix-appimage: no such file: $APPIMAGE" >&2; exit 1; }
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

# Pinned repack tool (immutable versioned release; bump TOOL_* together).
TOOL_VERSION="1.9.1"
TOOL_SHA256="ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

sum_of() {
  if command -v shasum >/dev/null 2>&1; then shasum -a 256 "$1"; else sha256sum "$1"; fi | awk '{print $1}'
}

# Unpack via the embedded runtime (no FUSE needed).
chmod +x "$APPIMAGE"
(cd "$WORK" && "$APPIMAGE" --appimage-extract >/dev/null)
APPDIR="$WORK/squashfs-root"
[ -d "$APPDIR/usr/lib" ] || { echo "fix-appimage: unexpected AppDir layout" >&2; exit 1; }

# The over-bundled infra libs. List from tauri#15665, confirmed drop-in
# replaceable by the system copies on every target distro (SteamOS ships all
# of them; the .deb already relies on them via its package deps).
STRIP_GLOBS=(
  'libwayland-*.so*'
  'libglib-2.0.so*' 'libgio-2.0.so*' 'libgobject-2.0.so*'
  'libgmodule-2.0.so*' 'libgthread-2.0.so*'
  'libgst*.so*'
  'libmount.so*' 'libblkid.so*' 'libselinux.so*' 'libpcre2-8.so*'
  'libzstd.so*' 'libelf.so*' 'libffi.so*'
  # Not in the upstream-issue list, but observed (Ubuntu 24.04 VM): the stale
  # bundled copy shadows the dependency of HOST code dlopen'd into our process
  # (system gio module -> system libcurl-gnutls -> needs a newer nghttp2
  # symbol), the same failure class as libwayland. If similar shadow errors
  # appear later, libdbus-1/libsystemd/libudev/libpsl are the next candidates.
  'libnghttp2.so*'
)
removed=0
for glob in "${STRIP_GLOBS[@]}"; do
  while IFS= read -r -d '' lib; do
    echo "fix-appimage: strip ${lib#"$APPDIR"/}"
    rm -f "$lib"
    removed=$((removed + 1))
  done < <(find "$APPDIR/usr/lib" -maxdepth 1 -name "$glob" -print0)
done
if [ "$removed" -eq 0 ]; then
  # Not fatal: a future Tauri bundler may stop over-bundling, making this a no-op.
  echo "fix-appimage: WARNING: nothing to strip - bundler output changed?" >&2
else
  echo "fix-appimage: stripped $removed over-bundled libs"
fi

# Tauri's linuxdeploy pass patchelf's every executable in usr/bin, which
# CORRUPTS the static-pie runtime of the kroma-mpv sidecar AppImage: the bundled
# copy SIGSEGVs instantly on every machine (so each video-output rung "fails"
# and the IPC socket never appears), while the pristine pkgforge artifact runs
# fine. Restore the pristine bytes fetched by fetch-mpv.sh.
SIDECAR="$APPDIR/usr/bin/kroma-mpv"
PRISTINE="$SCRIPT_DIR/../src-tauri/bin/kroma-mpv-x86_64-unknown-linux-gnu"
if [ -f "$SIDECAR" ]; then
  if [ -f "$PRISTINE" ]; then
    if [ "$(sum_of "$SIDECAR")" != "$(sum_of "$PRISTINE")" ]; then
      install -m 0755 "$PRISTINE" "$SIDECAR"
      echo "fix-appimage: restored pristine kroma-mpv sidecar (bundler patchelf corrupted it)"
    else
      echo "fix-appimage: kroma-mpv sidecar already pristine"
    fi
  elif [ "${CI:-}" = "true" ]; then
    # In CI the pristine file must exist (fetch-mpv.sh runs before the build);
    # shipping the corrupted sidecar would brick native playback.
    echo "fix-appimage: pristine kroma-mpv missing (run scripts/fetch-mpv.sh first)" >&2
    exit 1
  else
    echo "fix-appimage: WARNING: pristine kroma-mpv not found; sidecar left as bundled (likely corrupt)" >&2
  fi
fi

TOOL="$WORK/appimagetool"
curl -fsSL --retry 3 -o "$TOOL" \
  "https://github.com/AppImage/appimagetool/releases/download/${TOOL_VERSION}/appimagetool-x86_64.AppImage"
if [ "$(sum_of "$TOOL")" != "$TOOL_SHA256" ]; then
  echo "fix-appimage: sha256 MISMATCH for appimagetool ${TOOL_VERSION}" >&2
  exit 1
fi
chmod +x "$TOOL"

# appimagetool is itself an AppImage; extract-and-run avoids a FUSE dependency.
ARCH=x86_64 APPIMAGE_EXTRACT_AND_RUN=1 "$TOOL" "$APPDIR" "$WORK/fixed.AppImage" >/dev/null
mv "$WORK/fixed.AppImage" "$APPIMAGE"
chmod +x "$APPIMAGE"
echo "fix-appimage: repacked $(basename "$APPIMAGE") ($(du -h "$APPIMAGE" | cut -f1))"
