# Conformance fixtures

Must-accept and must-reject `.tes` files for open-format readers
([docs/mime.md](../../docs/mime.md)).

| Tree | Expectation |
| --- | --- |
| `accept/` | `tes verify --deep` exits 0 |
| `reject/` | `tes verify --deep` exits 1 |

Third-party readers that claim Tessera compatibility should run the same gate
(or an equivalent deep decode + validate path).

## Accept set

Copied from `fixtures/v0/` goldens so Windows CI does not depend on symlinks:

- `empty.tes` — superblock-only skeleton
- `note_one_chunk.tes` — catalog + one text chunk
- `note_three_chunks.tes` — heading + paragraph + list
- `hub_links.tes` — hub with `TLNK` edges
- `external_links.tes` — `TLNK` v1 external URI heap (+ mixed internal)
- `layout_v1_text.tes` — spans / math / code lang / structured table / lang / align
- `attachment_sample.tes` — inert attachment (safe basename + PDF bytes)
- `slide_deck.tes` — region-based slide
- `research_cite.tes` — cite chunk + citation link
- `figure_sample.tes` — image + figure
- `unknown_optional_feature.tes` — catalog declares unknown **optional** feature (warn, still ok)

## Reject set

### Structural (from `note_one_chunk.tes`, except `too_short.tes`)

| File | Fault |
| --- | --- |
| `bad_magic.tes` | Byte 0 ≠ `T` |
| `bad_tidx_magic.tes` | `TIDX` magic corrupted |
| `bad_catalog.tes` | Catalog JSON first byte corrupted |
| `truncated.tes` | Last 10 payload bytes removed |
| `history_flag_no_thst.tes` | History flag set without `THST` footer |
| `too_short.tes` | 10 zero bytes (shorter than superblock) |

### Layout-v1 / attachment semantic (deep verify)

| File | Fault |
| --- | --- |
| `span_oob.tes` | Inline span end past body length |
| `span_partial_overlap.tes` | Nested span escapes outer span |
| `table_rowspan_zero.tes` | Structured table `rowspan: 0` |
| `oversized_text_header.tes` | Text header JSON > 4 KiB |
| `unsafe_attachment_filename.tes` | Attachment basename with `../` |
| `unknown_required_feature.tes` | Catalog `features.required` names an unknown id |

### Feature-flag accept (deep verify ok, may warn)

| File | Note |
| --- | --- |
| `unknown_optional_feature.tes` | Catalog `features.optional` names an unknown id |

## Regenerate

```bash
# Goldens + accept sync
cargo run --example gen_v0_fixtures
cp fixtures/v0/*.tes fixtures/conformance/accept/

# Layout-v1 / attachment rejects
cargo run --example gen_conformance_rejects

# Structural rejects (after note_one_chunk changes)
python3 - <<'PY'
from pathlib import Path
src = Path('fixtures/v0/note_one_chunk.tes').read_bytes()
out = Path('fixtures/conformance/reject')
b = bytearray(src); b[0] = ord('X'); (out/'bad_magic.tes').write_bytes(b)
(out/'truncated.tes').write_bytes(src[:-10])
(out/'too_short.tes').write_bytes(bytes(10))
idx = src.find(b'TIDX'); b = bytearray(src); b[idx] = ord('Z'); (out/'bad_tidx_magic.tes').write_bytes(b)
b = bytearray(src); b[64] = ord('?'); (out/'bad_catalog.tes').write_bytes(b)
b = bytearray(src); b[8:12] = (1).to_bytes(4, 'little'); (out/'history_flag_no_thst.tes').write_bytes(b)
PY
```

Or: `mise run fixtures` (goldens + accept + layout smoke), then
`cargo run --example gen_conformance_rejects`.
