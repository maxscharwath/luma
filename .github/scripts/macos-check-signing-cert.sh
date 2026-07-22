#!/usr/bin/env bash
# Decide whether the desktop macOS build can be signed, WITHOUT hard-failing the
# release when it cannot: a merely-importable cert (or no paid membership) has
# to produce an UNSIGNED build, not a red release.
#
# Inputs (step env): APPLE_CERTIFICATE, APPLE_CERTIFICATE_PASSWORD
# Output (GITHUB_OUTPUT): sign=true|false
set -euo pipefail

if [ -z "$APPLE_CERTIFICATE" ]; then
  echo "No APPLE_CERTIFICATE secret; building unsigned."
  echo "sign=false" >> "$GITHUB_OUTPUT"; exit 0
fi
kc="$RUNNER_TEMP/kroma-check.keychain-db"
security create-keychain -p '' "$kc"
security unlock-keychain -p '' "$kc"
echo "$APPLE_CERTIFICATE" | base64 --decode > "$RUNNER_TEMP/cert.p12" 2>/dev/null || true
security import "$RUNNER_TEMP/cert.p12" -k "$kc" -P "$APPLE_CERTIFICATE_PASSWORD" -T /usr/bin/codesign >/dev/null 2>&1 || true
# Only sign if a genuine, valid Developer ID Application identity is present -
# a merely-importable cert (or none, e.g. no paid membership) builds UNSIGNED.
if security find-identity -v -p codesigning "$kc" | grep -q "Developer ID Application"; then
  echo "Valid Developer ID Application cert; signing enabled."
  echo "sign=true" >> "$GITHUB_OUTPUT"
else
  echo "::warning::No valid Developer ID Application cert; building UNSIGNED (open with right-click > Open or xattr -dr com.apple.quarantine)."
  echo "sign=false" >> "$GITHUB_OUTPUT"
fi
security delete-keychain "$kc" >/dev/null 2>&1 || true
rm -f "$RUNNER_TEMP/cert.p12"
