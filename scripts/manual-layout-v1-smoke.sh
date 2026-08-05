#!/usr/bin/env bash
# Smoke goldens + conformance kit after fixture regen (no heredoc traps).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
cargo build -q --bin tes

TES="${TES:-$ROOT/target/debug/tes}"
if [[ ! -x "$TES" ]]; then
  TES="$(cargo metadata --format-version 1 --no-deps \
    | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')/debug/tes"
fi

echo "TES=$TES"
echo "=== deep verify fixtures/v0 ==="
"$TES" verify --deep fixtures/v0/*.tes --quiet

echo "=== deep verify conformance accept ==="
"$TES" verify --deep fixtures/conformance/accept/*.tes --quiet

echo "=== deep verify conformance reject (must fail) ==="
shopt -s nullglob
failed=0
for f in fixtures/conformance/reject/*.tes; do
  if "$TES" verify --deep "$f" --quiet; then
    echo "expected reject, but deep verify succeeded: $f" >&2
    failed=1
  else
    echo "ok reject: $(basename "$f")"
  fi
done
[[ "$failed" -eq 0 ]]

echo "=== layout_v1_text textconv (expect math + rust + mermaid + table + captions) ==="
"$TES" textconv fixtures/v0/layout_v1_text.tes | tee /tmp/tessera-layout-v1-textconv.txt
grep -q 'title="Relativity"' /tmp/tessera-layout-v1-textconv.txt
grep -q 'caption="Mass–energy equivalence"' /tmp/tessera-layout-v1-textconv.txt
grep -q 'title="Listing 1"' /tmp/tessera-layout-v1-textconv.txt
grep -q 'caption="Hello Tessera"' /tmp/tessera-layout-v1-textconv.txt
grep -q 'title="Pipeline"' /tmp/tessera-layout-v1-textconv.txt
grep -q 'caption="Authoring flow"' /tmp/tessera-layout-v1-textconv.txt
grep -q 'title="Features"' /tmp/tessera-layout-v1-textconv.txt
grep -q 'caption="Layout feature ids"' /tmp/tessera-layout-v1-textconv.txt
grep -q '```mermaid' /tmp/tessera-layout-v1-textconv.txt
grep -q '\\text{' /tmp/tessera-layout-v1-textconv.txt

echo "=== import layout_v1_sample.md ==="
WORKDIR="$(mktemp -d /tmp/tessera-layout-XXXXXX)"
"$TES" import fixtures/assets/markdown/layout_v1_sample.md "$WORKDIR/imported.tes" --markdown
"$TES" info "$WORKDIR/imported.tes"
"$TES" textconv "$WORKDIR/imported.tes"

echo "=== slide / cite / figure / attachment info ==="
"$TES" info fixtures/v0/slide_deck.tes --quiet
"$TES" info fixtures/v0/research_cite.tes --quiet
"$TES" info fixtures/v0/figure_sample.tes --quiet
"$TES" info fixtures/v0/attachment_sample.tes --quiet

echo "OK (workdir=$WORKDIR)"
