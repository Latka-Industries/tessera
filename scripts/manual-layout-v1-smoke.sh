#!/usr/bin/env bash
# Smoke the layout-v1 / shipped golden fixtures (no heredoc traps).
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

echo "=== layout_v1_text textconv (expect math + rust + table) ==="
"$TES" textconv fixtures/v0/layout_v1_text.tes

echo "=== import layout_v1_sample.md ==="
WORKDIR="$(mktemp -d /tmp/tessera-layout-XXXXXX)"
"$TES" import fixtures/assets/markdown/layout_v1_sample.md "$WORKDIR/imported.tes" --markdown
"$TES" info "$WORKDIR/imported.tes"
"$TES" textconv "$WORKDIR/imported.tes"

echo "=== slide / cite / figure info ==="
"$TES" info fixtures/v0/slide_deck.tes --quiet
"$TES" info fixtures/v0/research_cite.tes --quiet
"$TES" info fixtures/v0/figure_sample.tes --quiet

echo "OK (workdir=$WORKDIR)"
