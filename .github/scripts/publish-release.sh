#!/usr/bin/env bash
# Flip the draft Release for $TAG live and mark it latest.
#
# Inputs (step/job env): GH_TOKEN, TAG (e.g. v0.1.32), GITHUB_REPOSITORY
set -euo pipefail

# The per-artifact upload steps (across this workflow AND synology.yml)
# each attach via softprops/action-gh-release, which is NOT atomic: in a
# concurrency race two runners can both create a release object for the
# same tag, leaving the real (asset-carrying) one plus an EMPTY duplicate
# draft. So do not blindly grab the first draft (`head -1` could pick the
# empty one and 422 because a published same-tag release already exists).
# Instead: publish the release that actually carries the assets, treat an
# already-published one as success, and delete the empty duplicates.
LIST=$(gh api "repos/${GITHUB_REPOSITORY}/releases?per_page=30" \
  --jq "[.[] | select(.tag_name == \"${TAG}\") | {id, draft, assets: (.assets|length)}]")
if [ "$(echo "$LIST" | jq 'length')" = "0" ]; then
  echo "no release object for ${TAG}"; exit 1
fi
# The real release = most assets; on a tie prefer an already-published one.
TARGET=$(echo "$LIST" | jq -r 'sort_by([.assets, (if .draft then 0 else 1 end)]) | last | .id')
DRAFT=$(echo "$LIST" | jq -r --argjson id "$TARGET" '.[] | select(.id==$id) | .draft')
if [ "$DRAFT" = "true" ]; then
  gh api -X PATCH "repos/${GITHUB_REPOSITORY}/releases/${TARGET}" \
    -F draft=false -f make_latest=true >/dev/null
  echo "published ${TAG} (release id ${TARGET})"
else
  # A racing job or a prior run already flipped it live; just re-assert
  # latest (a no-op on the already-latest release, never a 422).
  gh api -X PATCH "repos/${GITHUB_REPOSITORY}/releases/${TARGET}" \
    -f make_latest=true >/dev/null
  echo "already published ${TAG} (release id ${TARGET}); ensured latest"
fi
# Clean up the empty duplicate drafts the race can leave. Guard on
# assets == 0 so a release that carries anything is NEVER deleted.
for DUP in $(echo "$LIST" | jq -r --argjson id "$TARGET" \
  '.[] | select(.id != $id and .draft and .assets == 0) | .id'); do
  if gh api -X DELETE "repos/${GITHUB_REPOSITORY}/releases/${DUP}" >/dev/null 2>&1; then
    echo "deleted empty duplicate draft ${DUP}"
  else
    echo "warn: could not delete duplicate draft ${DUP}"
  fi
done
