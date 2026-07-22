#!/usr/bin/env bash
# Sign deep (incl. the embedded dylibs) + notarize the already-built .app when a
# Developer ID cert is present, then package the dmg. The macsign step already
# validated + then DELETED its keychain, so the cert is re-imported here.
#
# Inputs (step env): SIGN, APPLE_CERTIFICATE, APPLE_CERTIFICATE_PASSWORD,
#   APPLE_SIGNING_IDENTITY, APPLE_ID, APPLE_PASSWORD, APPLE_TEAM_ID
set -euo pipefail

BUNDLE=clients/desktop/src-tauri/target/release/bundle
APP=$(find "$BUNDLE/macos" -maxdepth 1 -name '*.app' | head -n1)
if [ "$SIGN" = "true" ]; then
  kc="$RUNNER_TEMP/kroma-sign.keychain-db"
  security create-keychain -p '' "$kc"
  security default-keychain -s "$kc"
  security unlock-keychain -p '' "$kc"
  echo "$APPLE_CERTIFICATE" | base64 --decode > "$RUNNER_TEMP/cert.p12"
  security import "$RUNNER_TEMP/cert.p12" -k "$kc" -P "$APPLE_CERTIFICATE_PASSWORD" -T /usr/bin/codesign
  security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k '' "$kc" >/dev/null
  codesign --deep --force --timestamp --options runtime --sign "$APPLE_SIGNING_IDENTITY" "$APP"
  ditto -c -k --keepParent "$APP" "$RUNNER_TEMP/app.zip"
  xcrun notarytool submit "$RUNNER_TEMP/app.zip" \
    --apple-id "$APPLE_ID" --password "$APPLE_PASSWORD" --team-id "$APPLE_TEAM_ID" --wait
  xcrun stapler staple "$APP"
fi
mkdir -p "$BUNDLE/dmg"
DMG="$BUNDLE/dmg/KROMA_$(uname -m).dmg"
hdiutil create -volname KROMA -srcfolder "$APP" -ov -format UDZO "$DMG"
if [ "$SIGN" = "true" ]; then
  codesign --force --timestamp --sign "$APPLE_SIGNING_IDENTITY" "$DMG"
  xcrun notarytool submit "$DMG" \
    --apple-id "$APPLE_ID" --password "$APPLE_PASSWORD" --team-id "$APPLE_TEAM_ID" --wait
  xcrun stapler staple "$DMG"
fi
