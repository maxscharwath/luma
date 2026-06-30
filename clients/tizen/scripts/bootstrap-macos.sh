#!/usr/bin/env bash
# ============================================================================
# LUMA one-shot macOS setup for deploying to a Samsung Tizen TV.
#
#   bash clients/tizen/scripts/bootstrap-macos.sh
#
# Automates everything that is machine-local:
#   1. Rosetta 2 (Tizen's tools are x86_64)
#   2. Download the Tizen Studio installer
#   3. Mount + open it (click through; install to ~/tizen-studio)
#   4. Verify with `make doctor`
#
# It CANNOT do these they are Samsung's device security, bound to YOU:
#   • TV Developer Mode  (toggle on the TV with the remote: Apps → 12345)
#   • Your TV's IP address
#   • The Samsung certificate (Certificate Manager → your Samsung account +
#     the connected TV's DUID). A self-signed cert only works on the emulator.
# ============================================================================
set -euo pipefail

VER=6.1
DMG="Baseline_Tizen_Studio_${VER}_macos-64.dmg"
URL="https://download.tizen.org/sdk/Installer/tizen-studio_${VER}/${DMG}"
DEST="$HOME/Downloads/$DMG"
TIZEN_HOME="$HOME/tizen-studio"
HERE="$(cd "$(dirname "$0")/.." && pwd)"

say(){ printf "\n\033[1;33m▶ %s\033[0m\n" "$1"; }
ok(){  printf "\033[1;32m  ✓ %s\033[0m\n" "$1"; }

# ---- 1. Rosetta -----------------------------------------------------------
if /usr/bin/pgrep -q oahd; then
  ok "Rosetta already installed"
else
  say "Installing Rosetta 2 (may ask for your password)…"
  softwareupdate --install-rosetta --agree-to-license
fi

# ---- 2/3. Tizen Studio ----------------------------------------------------
if [ -x "$TIZEN_HOME/tools/ide/bin/tizen" ]; then
  ok "Tizen Studio already installed at $TIZEN_HOME"
else
  if [ ! -s "$DEST" ]; then
    say "Downloading Tizen Studio $VER (a few hundred MB)…"
    curl -L --fail --progress-bar -o "$DEST" "$URL"
  else
    ok "Installer already downloaded: $DEST"
  fi

  say "Opening the installer install to the default ~/tizen-studio"
  open "$DEST"   # mounts the DMG and shows it in Finder
  cat <<'EOF'

  In the installer window:
    • Run the installer app, accept, keep install path  ~/tizen-studio
  When it finishes it launches Package Manager install BOTH:
    • Extension SDK → "Samsung Certificate Extension"
    • Extension SDK → "TV Extensions"  (latest)

  Then return here and press Enter to continue…
EOF
  read -r _
fi

# ---- PATH hint ------------------------------------------------------------
if ! command -v tizen >/dev/null 2>&1; then
  say "Add the Tizen tools to your PATH (one-time):"
  echo "  echo 'export PATH=\"$TIZEN_HOME/tools/ide/bin:$TIZEN_HOME/tools:\$PATH\"' >> ~/.zshrc && source ~/.zshrc"
fi

# ---- 4. Verify ------------------------------------------------------------
say "Verifying toolchain…"
make -C "$HERE" doctor || true

cat <<EOF

$(printf "\033[1;32m✓ Toolchain setup done.\033[0m")  Three steps remain that only you can do:

  1) TV → Apps → press 1 2 3 4 5 → Developer Mode ON,
     Host PC IP = this Mac's IP, then reboot the TV.
  2) Tizen Studio → Tools → Certificate Manager → + → Samsung → TV:
     sign in with your Samsung account, create Author + Distributor certs
     (the TV must be connected so it reads its DUID). Name the profile  LUMA.
  3) Deploy:
        cp clients/tizen/.tizen.env.example clients/tizen/.tizen.env   # set TV_IP
        make -C clients/tizen deploy TV_IP=<your-tv-ip>

See clients/tizen/SETUP.md for the full walkthrough.
EOF
