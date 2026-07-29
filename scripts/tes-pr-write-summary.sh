#!/usr/bin/env bash
# Append a Tessprek preview report to the Actions job summary.
#
# Env:
#   BASE_SHA    short label / full sha
#   HEAD_SHA    short label / full sha
#   BODY_FILE   report path (default: tes-pr-preview.md)
#   GITHUB_STEP_SUMMARY  (Actions provides this)
#   MAX_SUMMARY_BYTES    soft cap (default: 200000)

set -euo pipefail

: "${BASE_SHA:?set BASE_SHA}"
: "${HEAD_SHA:?set HEAD_SHA}"
: "${GITHUB_STEP_SUMMARY:?set GITHUB_STEP_SUMMARY}"
BODY_FILE="${BODY_FILE:-tes-pr-preview.md}"
MAX_SUMMARY_BYTES="${MAX_SUMMARY_BYTES:-200000}"

{
  echo "## Tessera Tessprek preview"
  echo
  echo "Range \`${BASE_SHA}\`…\`${HEAD_SHA}\`"
  echo
  # Cap summary size (GitHub ~1 MiB); full report is the artifact.
  head -c "$MAX_SUMMARY_BYTES" "$BODY_FILE" || true
} >>"$GITHUB_STEP_SUMMARY"
