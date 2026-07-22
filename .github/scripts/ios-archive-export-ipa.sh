#!/usr/bin/env bash
# Archive the prebuilt iOS project and export an App Store .ipa into ./out.
# Runs with the step's working-directory set to clients/mobile/ios.
#
# Inputs (step env): APPLE_TEAM_ID, VERSION (full release version, names the
# .ipa), and the App Store Connect key trio ASC_KEY_ID / ASC_ISSUER_ID /
# ASC_PRIVATE_KEY.
#
# The Expo template leaves the project on AUTOMATIC signing, so xcodebuild has
# to resolve the App Store profile itself. `-allowProvisioningUpdates` can only
# do that when it is authenticated, hence the API key: without it the archive
# fails with "no profile matching tv.kroma.mobile". The certificate and profile
# imported by the previous step still cover the case where the key is absent.
set -euo pipefail

auth=()
if [ -n "${ASC_KEY_ID:-}" ] && [ -n "${ASC_ISSUER_ID:-}" ] && [ -n "${ASC_PRIVATE_KEY:-}" ]; then
  mkdir -p "$RUNNER_TEMP/asc"
  printf '%s' "$ASC_PRIVATE_KEY" > "$RUNNER_TEMP/asc/AuthKey_${ASC_KEY_ID}.p8"
  auth=(-authenticationKeyPath "$RUNNER_TEMP/asc/AuthKey_${ASC_KEY_ID}.p8"
        -authenticationKeyID "$ASC_KEY_ID"
        -authenticationKeyIssuerID "$ASC_ISSUER_ID")
fi

xcodebuild -workspace KROMA.xcworkspace -scheme KROMA \
  -configuration Release -sdk iphoneos \
  -archivePath "$RUNNER_TEMP/KROMA.xcarchive" \
  DEVELOPMENT_TEAM="$APPLE_TEAM_ID" \
  -allowProvisioningUpdates "${auth[@]}" archive
cat > "$RUNNER_TEMP/ExportOptions.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>method</key><string>app-store-connect</string>
  <key>teamID</key><string>${APPLE_TEAM_ID}</string>
  <key>uploadSymbols</key><true/>
</dict></plist>
PLIST
xcodebuild -exportArchive -archivePath "$RUNNER_TEMP/KROMA.xcarchive" \
  -exportOptionsPlist "$RUNNER_TEMP/ExportOptions.plist" \
  -exportPath "$RUNNER_TEMP/export" \
  -allowProvisioningUpdates "${auth[@]}"
rm -rf "$RUNNER_TEMP/asc"
mkdir -p "$GITHUB_WORKSPACE/out"
cp "$RUNNER_TEMP/export"/*.ipa \
  "$GITHUB_WORKSPACE/out/KROMA-mobile-${VERSION}.ipa"
