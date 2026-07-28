#!/usr/bin/env bash
# Smoke: `tes merge-file` + real git `merge=tessera` driver (THI-201).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
tes() {
  (cd "$ROOT" && cargo run -q --bin tes -- "$@")
}

WORKDIR="$(mktemp -d)"
trap 'rm -rf "$WORKDIR"' EXIT

# ---------------------------------------------------------------------------
# Part 1: direct CLI
# ---------------------------------------------------------------------------
printf '# Note\n\nAlpha\n\nBeta\n' > "$WORKDIR/note.md"
tes import --markdown "$WORKDIR/note.md" "$WORKDIR/base.tes"
cp "$WORKDIR/base.tes" "$WORKDIR/ours.tes"
cp "$WORKDIR/base.tes" "$WORKDIR/theirs.tes"

HASH=$(tes edit-read "$WORKDIR/ours.tes" 2>&1 >/dev/null | sed -n 's/^source-hash=//p')
printf '%s\n' '[{"op":"set_text","chunk_id":2,"body":"Alpha ours"}]' > "$WORKDIR/ours.json"
tes apply "$WORKDIR/ours.tes" --ops "$WORKDIR/ours.json" --source-hash "$HASH" >/dev/null

HASH=$(tes edit-read "$WORKDIR/theirs.tes" 2>&1 >/dev/null | sed -n 's/^source-hash=//p')
printf '%s\n' '[{"op":"set_text","chunk_id":3,"body":"Beta theirs"}]' > "$WORKDIR/theirs.json"
tes apply "$WORKDIR/theirs.tes" --ops "$WORKDIR/theirs.json" --source-hash "$HASH" >/dev/null

tes merge-file "$WORKDIR/base.tes" "$WORKDIR/ours.tes" "$WORKDIR/theirs.tes"
tes verify --deep "$WORKDIR/ours.tes" >/dev/null
raw="$(tes export "$WORKDIR/ours.tes" --raw)"
echo "$raw" | grep -q 'Alpha ours'
echo "$raw" | grep -q 'Beta theirs'

cp "$WORKDIR/base.tes" "$WORKDIR/overlap-a.tes"
cp "$WORKDIR/base.tes" "$WORKDIR/overlap-b.tes"
HASH=$(tes edit-read "$WORKDIR/overlap-a.tes" 2>&1 >/dev/null | sed -n 's/^source-hash=//p')
printf '%s\n' '[{"op":"set_text","chunk_id":2,"body":"clash A"}]' > "$WORKDIR/clash.json"
tes apply "$WORKDIR/overlap-a.tes" --ops "$WORKDIR/clash.json" --source-hash "$HASH" >/dev/null
before="$(cksum "$WORKDIR/overlap-a.tes")"
HASH=$(tes edit-read "$WORKDIR/overlap-b.tes" 2>&1 >/dev/null | sed -n 's/^source-hash=//p')
printf '%s\n' '[{"op":"set_text","chunk_id":2,"body":"clash B"}]' > "$WORKDIR/clash.json"
tes apply "$WORKDIR/overlap-b.tes" --ops "$WORKDIR/clash.json" --source-hash "$HASH" >/dev/null
if tes merge-file "$WORKDIR/base.tes" "$WORKDIR/overlap-a.tes" "$WORKDIR/overlap-b.tes" 2>/dev/null; then
  echo "expected overlap conflict" >&2
  exit 1
fi
after="$(cksum "$WORKDIR/overlap-a.tes")"
[[ "$before" == "$after" ]]
echo "OK merge-file CLI"

# ---------------------------------------------------------------------------
# Part 2: real git merge with merge=tessera
# ---------------------------------------------------------------------------
REPO="$WORKDIR/repo"
mkdir -p "$REPO"
cd "$REPO"

DRIVER="$WORKDIR/tes-merge-driver"
cat >"$DRIVER" <<EOF
#!/usr/bin/env bash
set -euo pipefail
# Git passes %O %A %B relative to the work tree; resolve before cd'ing to ROOT.
args=()
for p in "\$@"; do
  if [[ "\$p" == /* ]]; then
    args+=("\$p")
  else
    args+=("\$PWD/\$p")
  fi
done
cd "$ROOT" && cargo run -q --bin tes -- merge-file "\${args[@]}"
EOF
chmod +x "$DRIVER"

git init -q -b main
git config user.email "merge-smoke@example.com"
git config user.name "merge-smoke"
git config commit.gpgsign false
git config merge.tessera.name "Tessera verified structural merge"
git config merge.tessera.driver "$DRIVER %O %A %B"
printf '%s\n' '*.tes merge=tessera' >.gitattributes

cp "$WORKDIR/base.tes" "$REPO/note.tes"
git add note.tes .gitattributes
git commit -qm "base"

# Non-overlapping: side-a edits chunk 2, side-b edits chunk 3.
git checkout -qb side-a
HASH=$(tes edit-read "$REPO/note.tes" 2>&1 >/dev/null | sed -n 's/^source-hash=//p')
printf '%s\n' '[{"op":"set_text","chunk_id":2,"body":"Alpha ours"}]' >"$WORKDIR/ops.json"
tes apply "$REPO/note.tes" --ops "$WORKDIR/ops.json" --source-hash "$HASH" >/dev/null
git commit -qam "side-a: alpha"

git checkout -q main
git checkout -qb side-b
HASH=$(tes edit-read "$REPO/note.tes" 2>&1 >/dev/null | sed -n 's/^source-hash=//p')
printf '%s\n' '[{"op":"set_text","chunk_id":3,"body":"Beta theirs"}]' >"$WORKDIR/ops.json"
tes apply "$REPO/note.tes" --ops "$WORKDIR/ops.json" --source-hash "$HASH" >/dev/null
git commit -qam "side-b: beta"

git checkout -q side-a
git merge --no-edit side-b
tes verify --deep "$REPO/note.tes" >/dev/null
raw="$(tes export "$REPO/note.tes" --raw)"
echo "$raw" | grep -q 'Alpha ours'
echo "$raw" | grep -q 'Beta theirs'
echo "OK git merge non-overlapping"

# Overlapping: both edit chunk 2 from base tip.
git checkout -q main
git checkout -qb clash-a
HASH=$(tes edit-read "$REPO/note.tes" 2>&1 >/dev/null | sed -n 's/^source-hash=//p')
printf '%s\n' '[{"op":"set_text","chunk_id":2,"body":"clash A"}]' >"$WORKDIR/ops.json"
tes apply "$REPO/note.tes" --ops "$WORKDIR/ops.json" --source-hash "$HASH" >/dev/null
git commit -qam "clash-a"

git checkout -q main
git checkout -qb clash-b
HASH=$(tes edit-read "$REPO/note.tes" 2>&1 >/dev/null | sed -n 's/^source-hash=//p')
printf '%s\n' '[{"op":"set_text","chunk_id":2,"body":"clash B"}]' >"$WORKDIR/ops.json"
tes apply "$REPO/note.tes" --ops "$WORKDIR/ops.json" --source-hash "$HASH" >/dev/null
git commit -qam "clash-b"

git checkout -q clash-a
if git merge --no-edit clash-b; then
  echo "expected git merge conflict for overlapping edits" >&2
  exit 1
fi
git merge --abort >/dev/null 2>&1 || true
echo "OK git merge overlapping conflict"

echo "OK merge-file smoke (CLI + git driver)"
