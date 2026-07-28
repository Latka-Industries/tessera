#!/usr/bin/env bash
# Upsert a sticky PR comment whose body starts with MARKER (THI-212).
#
# Env:
#   GH_TOKEN   GitHub token (required for gh api)
#   REPO       owner/name (default: gh repo view --json nameWithOwner)
#   PR         pull request number (required)
#   BODY_FILE  markdown file to post (default: tes-pr-preview.md)
#   MARKER     sticky identity string (default: <!-- tessera-tes-preview -->)

set -euo pipefail

: "${PR:?set PR to the pull request number}"
BODY_FILE="${BODY_FILE:-tes-pr-preview.md}"
MARKER="${MARKER:-<!-- tessera-tes-preview -->}"
REPO="${REPO:-$(gh repo view --json nameWithOwner -q .nameWithOwner)}"

existing="$(
  gh api "repos/${REPO}/issues/${PR}/comments" --paginate \
    --jq ".[] | select(.body | contains(\"${MARKER}\")) | .id" \
    | head -n1 || true
)"

if [[ -n "${existing}" ]]; then
  gh api --method PATCH "repos/${REPO}/issues/comments/${existing}" \
    -F body=@"${BODY_FILE}"
  echo "updated comment ${existing}"
else
  gh api --method POST "repos/${REPO}/issues/${PR}/comments" \
    -F body=@"${BODY_FILE}"
  echo "created comment"
fi
