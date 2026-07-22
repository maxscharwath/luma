#!/usr/bin/env bash
# Publish every artifact this run built onto the rolling `nightly` prerelease,
# refresh nightly-manifest.json and regenerate the release notes.
#
# Inputs (job env): GH_TOKEN, GH_REPO, VERSION, GITHUB_SHA
# Expects the downloaded artifacts under ./assets and a full-history checkout.
set -euo pipefail

gh release view nightly >/dev/null 2>&1 \
  || gh release create nightly --prerelease --title "KROMA nightly" --notes "Rolling nightly build from main."

# Previous nightly sha (for the commit log) before we overwrite it.
LAST="$(gh release download nightly -p nightly-manifest.json -O - 2>/dev/null | jq -r '.sha // empty' || true)"

# Clear last night's assets; keep the per-push canary .spk pair.
for A in $(gh api "repos/${GH_REPO}/releases/tags/nightly" \
    --jq '.assets[] | select(.name | startswith("kroma-nightly-x86_64.spk") | not) | .id'); do
  gh api -X DELETE "repos/${GH_REPO}/releases/assets/${A}" >/dev/null
done

jq -n --arg sha "$GITHUB_SHA" --arg version "$VERSION" \
  --arg date "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
  '{sha: $sha, version: $version, date: $date}' > assets/nightly-manifest.json

find assets -type f | sort
find assets -type f -print0 | xargs -0 gh release upload nightly --clobber

{
  echo "Rolling nightly build \`$VERSION\` from \`${GITHUB_SHA::10}\`."
  echo
  echo "Every artifact of a stable release, rebuilt nightly from main:"
  echo "desktop installers, TV packages, module bundles + modules.json,"
  echo "and the per-push canary Synology .spk. Unstable by definition."
  if [ -n "$LAST" ]; then
    echo
    echo "Changes since the previous nightly:"
    git log --oneline --no-decorate "$LAST"..HEAD 2>/dev/null | head -40 | sed 's/^/- /' || true
  fi
} > notes.md
gh release edit nightly --title "KROMA nightly ($VERSION)" --notes-file notes.md
