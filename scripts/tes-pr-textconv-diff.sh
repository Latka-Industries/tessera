#!/usr/bin/env bash
# Build a Markdown report of Tessprek diffs for .tes files changed between two
# commits (THI-212 / THI-219). Used by `.github/workflows/tes-pr-preview.yml`
# (PR comments + push summaries) and the vault template under `contrib/github/`.
#
# Usage:
#   TES=./target/debug/tes BASE_SHA=… HEAD_SHA=… OUT=report.md \
#     scripts/tes-pr-textconv-diff.sh
#
# Env:
#   TES       path to `tes` binary (required)
#   BASE_SHA  base / before commit (required)
#   HEAD_SHA  head / after commit (required)
#   OUT       report path (default: tes-pr-preview.md)
#   MAX_BYTES soft cap for report body (default: 60000)

set -euo pipefail

: "${TES:?set TES to the tes binary}"
: "${BASE_SHA:?set BASE_SHA}"
: "${HEAD_SHA:?set HEAD_SHA}"
OUT="${OUT:-tes-pr-preview.md}"
MAX_BYTES="${MAX_BYTES:-60000}"
MARKER='<!-- tessera-tes-preview -->'

normalize_tessprek() {
  # Match src/edit/mod.rs::normalize_tessprek_for_diff — ignore source-hash churn.
  sed -E 's/(<!-- tessera:.*source-hash=)[^[:space:]]+([[:space:]]*-->)/\1<hash>\2/'
}

textconv_blob() {
  local sha="$1" path="$2" dest="$3"
  if ! git cat-file -e "${sha}:${path}" 2>/dev/null; then
    return 1
  fi
  local tmp
  tmp="$(mktemp "${TMPDIR:-/tmp}/tes-blob.XXXXXX.tes")"
  git show "${sha}:${path}" >"$tmp"
  if ! "$TES" textconv "$tmp" >"$dest" 2>/dev/null; then
    rm -f "$tmp"
    return 1
  fi
  rm -f "$tmp"
  normalize_tessprek <"$dest" >"${dest}.norm"
  mv "${dest}.norm" "$dest"
}

# Pick a fence longer than any fence-like line in `file` so nested Markdown
# code fences (and unified-diff lines like " ```rust") do not close early and
# leave an empty copyable block in the GitHub comment.
#
# CommonMark allows up to 3 spaces before a closing fence; unified diffs also
# prefix context lines with a space (and +/- for changes), so a bare
# ^``` check is not enough.
max_fence_ticks() {
  local file="$1"
  awk '
    {
      line = $0
      # Unified diff first column (optional).
      if (line ~ /^[+\- ]/) {
        line = substr(line, 2)
      }
      # CommonMark: up to three spaces of indent before a fence.
      sub(/^ {0,3}/, "", line)
      if (match(line, /^`+/)) {
        n = RLENGTH
        if (n > max) max = n
      }
    }
    END { print max + 0 }
  ' "$file"
}

markdown_fence() {
  local file="$1" info="${2:-}" ticks fence
  ticks=$(($(max_fence_ticks "$file") + 1))
  if ((ticks < 3)); then
    ticks=3
  fi
  fence="$(printf '%*s' "$ticks" '' | tr ' ' '`')"
  printf '%s%s\n' "$fence" "$info"
  cat "$file"
  printf '\n%s\n' "$fence"
}

emit_fenced() {
  local label="$1" file="$2"
  echo "_${label}_:"
  echo
  markdown_fence "$file" "tessprek"
  echo
}

append_section() {
  local section="$1"
  if (( truncated != 0 )); then
    return 0
  fi
  local current add
  current="$(wc -c <"$OUT" | tr -d ' ')"
  add="$(wc -c <"$section" | tr -d ' ')"
  if (( current + add > MAX_BYTES )); then
    {
      echo
      echo "_Report truncated (limit ${MAX_BYTES} bytes)."
      echo "See the workflow log / remaining files locally with \`tes textconv\`._"
      echo
    } >>"$OUT"
    truncated=1
  else
    cat "$section" >>"$OUT"
  fi
}

mapfile -t files < <(
  git diff --name-only --diff-filter=ACMRD "${BASE_SHA}...${HEAD_SHA}" -- '*.tes' \
    | sort -u
)

{
  echo "${MARKER}"
  echo "## Tessera PR preview (\`tes textconv\`)"
  echo
  echo "Readable Tessprek projection of \`.tes\` changes between"
  echo "\`${BASE_SHA:0:7}\` and \`${HEAD_SHA:0:7}\`."
  echo
  echo "> GitHub still shows \`.tes\` blobs as binary on the Files tab."
  echo "> Local clones can use \`tes textconv\` / \`.gitattributes\` \`diff=tessera\`."
  echo
} >"$OUT"

if [[ ${#files[@]} -eq 0 ]]; then
  {
    echo "_No \`.tes\` files changed in this range._"
    echo
  } >>"$OUT"
  echo "wrote ${OUT} (0 .tes files)"
  exit 0
fi

workdir="$(mktemp -d "${TMPDIR:-/tmp}/tes-pr-preview.XXXXXX")"
trap 'rm -rf "$workdir"' EXIT

truncated=0
for path in "${files[@]}"; do
  base_txt="${workdir}/base.txt"
  head_txt="${workdir}/head.txt"
  rm -f "$base_txt" "$head_txt"

  has_base=0
  has_head=0
  if textconv_blob "$BASE_SHA" "$path" "$base_txt"; then
    has_base=1
  fi
  if textconv_blob "$HEAD_SHA" "$path" "$head_txt"; then
    has_head=1
  fi

  section="${workdir}/section.md"
  {
    echo "### \`${path}\`"
    echo
    if [[ $has_base -eq 0 && $has_head -eq 0 ]]; then
      echo "_Could not textconv base or head (missing blob or decode error)._"
      echo
    elif [[ $has_base -eq 0 && $has_head -eq 1 ]]; then
      emit_fenced "Added — Tessprek at head" "$head_txt"
    elif [[ $has_base -eq 1 && $has_head -eq 0 ]]; then
      emit_fenced "Deleted — Tessprek at base" "$base_txt"
    elif cmp -s "$base_txt" "$head_txt"; then
      echo "_No Tessprek content change_ (binary may still differ)."
      echo
    else
      diff_file="${workdir}/diff.txt"
      diff -u --label "a/${path}" --label "b/${path}" "$base_txt" "$head_txt" >"$diff_file" || true
      markdown_fence "$diff_file" "diff"
      echo
    fi
  } >"$section"

  append_section "$section"
done

echo "wrote ${OUT} ($(wc -c <"$OUT" | tr -d ' ') bytes, ${#files[@]} file(s))"
