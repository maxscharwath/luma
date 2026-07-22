#!/usr/bin/env bash
# Resolve the one version every release artifact is stamped with, plus the
# channel and the nightly "is there anything new?" guard.
#
# Inputs (step env):
#   EVENT_NAME    - github.event_name
#   PUBLISH_INPUT - workflow_dispatch `publish` input (empty on other events)
#   VERSION_INPUT - workflow_dispatch `version` input (empty on other events)
# Outputs (GITHUB_OUTPUT): version, triplet, channel, proceed
set -euo pipefail

CHANNEL=none
PROCEED=true
if [ "$EVENT_NAME" = "push" ]; then
  V="${GITHUB_REF_NAME#v}"            # tag v0.1.0 -> 0.1.0
  CHANNEL=stable
elif [ "$EVENT_NAME" = "schedule" ] \
  || [ "$PUBLISH_INPUT" = "nightly" ]; then
  BASE="$(sed -nE 's/^version[[:space:]]*=[[:space:]]*"([^"]+)".*/\1/p' server/Cargo.toml | head -1)"
  V="$BASE-nightly.$(date -u +%Y%m%d)"
  CHANNEL=nightly
  # Skip the whole fleet when main hasn't moved since the last nightly.
  LAST="$(gh release download nightly -p nightly-manifest.json -O - 2>/dev/null | jq -r '.sha // empty' || true)"
  if [ "$LAST" = "$GITHUB_SHA" ]; then
    PROCEED=false
    echo "No new commits since the last nightly ($LAST); skipping."
  fi
elif [ -n "$VERSION_INPUT" ]; then
  V="$VERSION_INPUT"
else
  V="$(sed -nE 's/^version[[:space:]]*=[[:space:]]*"([^"]+)".*/\1/p' server/Cargo.toml | head -1)"
fi
T="${V%%-*}"                          # 0.1.0-rc1 -> 0.1.0 (TV manifests)
{
  echo "version=$V"
  echo "triplet=$T"
  echo "channel=$CHANNEL"
  echo "proceed=$PROCEED"
} >> "$GITHUB_OUTPUT"
echo "Version: $V (TV manifests: $T, channel: $CHANNEL, proceed: $PROCEED)"
