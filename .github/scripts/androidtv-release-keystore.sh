#!/usr/bin/env bash
# Pick the Android TV gradle task: release-sign when the keystore secrets exist,
# otherwise fall back to the debug-signed APK (installable via adb / Downloader
# for sideloading).
#
# Inputs (step env): ANDROID_KEYSTORE, ANDROID_KEYSTORE_PASSWORD,
#   ANDROID_KEY_ALIAS, ANDROID_KEY_PASSWORD
# Outputs (GITHUB_OUTPUT): task, apk. Exports the keystore to GITHUB_ENV.
set -euo pipefail

if [ -z "$ANDROID_KEYSTORE" ]; then
  echo "No ANDROID_KEYSTORE secret; building a debug-signed APK."
  echo "task=assembleDebug" >> "$GITHUB_OUTPUT"
  echo "apk=debug/app-debug.apk" >> "$GITHUB_OUTPUT"
  exit 0
fi
echo "$ANDROID_KEYSTORE" | base64 -d > "$RUNNER_TEMP/kroma.keystore"
{
  echo "KROMA_ANDROID_KEYSTORE_FILE=$RUNNER_TEMP/kroma.keystore"
  echo "KROMA_ANDROID_KEYSTORE_PASSWORD=$ANDROID_KEYSTORE_PASSWORD"
  echo "KROMA_ANDROID_KEY_ALIAS=$ANDROID_KEY_ALIAS"
  echo "KROMA_ANDROID_KEY_PASSWORD=$ANDROID_KEY_PASSWORD"
} >> "$GITHUB_ENV"
echo "task=assembleRelease" >> "$GITHUB_OUTPUT"
echo "apk=release/app-release.apk" >> "$GITHUB_OUTPUT"
