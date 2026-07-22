#!/usr/bin/env bash
# Import the Apple Distribution identity + the App Store provisioning profile
# into a throwaway keychain so xcodebuild can sign the mobile .ipa.
#
# Inputs (step env): IOS_CERTIFICATE, IOS_CERTIFICATE_PASSWORD, IOS_PROFILE
set -euo pipefail

kc="$RUNNER_TEMP/kroma-ios.keychain-db"
pw=$(uuidgen)
security create-keychain -p "$pw" "$kc"
security set-keychain-settings -lut 21600 "$kc"
security unlock-keychain -p "$pw" "$kc"
echo "$IOS_CERTIFICATE" | base64 --decode > "$RUNNER_TEMP/ios-cert.p12"
security import "$RUNNER_TEMP/ios-cert.p12" -k "$kc" -P "$IOS_CERTIFICATE_PASSWORD" \
  -T /usr/bin/codesign -T /usr/bin/security
security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$pw" "$kc" >/dev/null
security list-keychain -d user -s "$kc" login.keychain-db
mkdir -p "$HOME/Library/MobileDevice/Provisioning Profiles"
echo "$IOS_PROFILE" | base64 --decode \
  > "$HOME/Library/MobileDevice/Provisioning Profiles/kroma.mobileprovision"
rm -f "$RUNNER_TEMP/ios-cert.p12"
