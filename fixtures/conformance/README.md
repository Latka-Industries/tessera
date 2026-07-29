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
- `note_one_chunk.tes` — tagged note + inline `tes textconv` span
- `note_three_chunks.tes` — agenda (flags / PR preview / vault TOC)
- `hub_links.tes` — hub with `TLNK` edges
- `external_links.tes` — `TLNK` v1 external URI heap (+ mixed internal)
- `layout_v1_text.tes` — spans / math / code / feature-id table / lang / align
- `attachment_sample.tes` — inert attachment (safe basename + PDF bytes)
- `slide_deck.tes` — two region-based slides
- `research_cite.tes` — cite chunk + citation link
- `figure_sample.tes` — image + figure
- `unknown_optional_feature.tes` — catalog declares unknown **optional** feature (warn, still ok)

See also `fixtures/vault/` for a sample `vault.tes` TOC (not part of the
must-accept kit).

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

# Layout-v1 / attachment / feature-flag rejects (+ unknown optional accept)
cargo run --example gen_conformance_rejects

# Structural rejects (from note_one_chunk.tes)
uv run scripts/gen_structural_rejects.py

# Sample vault + vault.tes
cargo run --example gen_vault_fixtures
```

Or: `mise run fixtures` (goldens, rejects, vault sample, layout smoke).
