#!/usr/bin/env bash
# Archive the prebuilt iOS project and export an App Store .ipa into ./out.
# Runs with the step's working-directory set to clients/mobile/ios.
#
# Inputs (step env): APPLE_TEAM_ID, VERSION (full release version, names the .ipa)
set -euo pipefail

xcodebuild -workspace KROMA.xcworkspace -scheme KROMA \
  -configuration Release -sdk iphoneos \
  -archivePath "$RUNNER_TEMP/KROMA.xcarchive" \
  -allowProvisioningUpdates archive
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
  -exportPath "$RUNNER_TEMP/export"
mkdir -p "$GITHUB_WORKSPACE/out"
cp "$RUNNER_TEMP/export"/*.ipa \
  "$GITHUB_WORKSPACE/out/KROMA-mobile-${VERSION}.ipa"
