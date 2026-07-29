#!/usr/bin/env bash
# Resolve BASE/HEAD SHAs for Tessprek preview (PR or push).
# Writes GitHub Actions outputs: base, head, skip.
#
# Env (set by the workflow):
#   EVENT_NAME  pull_request | push | …
#   PR_BASE     github.event.pull_request.base.sha (PR only)
#   PR_HEAD     github.event.pull_request.head.sha (PR only)
#   BEFORE      github.event.before (push only)
#   HEAD_SHA    github.sha (push tip)
#   GITHUB_OUTPUT  (Actions provides this)

set -euo pipefail

: "${EVENT_NAME:?set EVENT_NAME}"
: "${GITHUB_OUTPUT:?set GITHUB_OUTPUT}"

zeros='0000000000000000000000000000000000000000'

if [[ "$EVENT_NAME" == "pull_request" ]]; then
  : "${PR_BASE:?set PR_BASE}"
  : "${PR_HEAD:?set PR_HEAD}"
  {
    echo "base=${PR_BASE}"
    echo "head=${PR_HEAD}"
    echo "skip=false"
  } >>"$GITHUB_OUTPUT"
  exit 0
fi

: "${HEAD_SHA:?set HEAD_SHA}"
before="${BEFORE:-}"

if [[ -z "$before" || "$before" == "$zeros" ]]; then
  # New branch / empty before: diff the tip commit only.
  before="$(git rev-parse "${HEAD_SHA}^" 2>/dev/null || true)"
  if [[ -z "$before" ]]; then
    echo "No parent for ${HEAD_SHA}; skipping Tessprek preview."
    echo "skip=true" >>"$GITHUB_OUTPUT"
    exit 0
  fi
fi

{
  echo "base=${before}"
  echo "head=${HEAD_SHA}"
  echo "skip=false"
} >>"$GITHUB_OUTPUT"
