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

**Do not edit by hand.** Source of truth is [`fixtures/v0/`](../v0/);
`mise run fixtures` / `gen_v0_fixtures` + `cp` syncs goldens here (Windows-safe,
no symlinks). Accept-only addition:

- `unknown_optional_feature.tes` — catalog declares unknown **optional** feature (warn, still ok)

Synced from `v0/`:

- `empty.tes` — superblock-only skeleton
- `note_one_chunk.tes` — tagged note + inline `tes textconv` span
- `note_three_chunks.tes` — agenda (flags / PR preview / vault TOC)
- `hub_links.tes` — hub with `TLNK` edges
- `external_links.tes` — `TLNK` v1 external URI heap (+ mixed internal)
- `layout_v1_text.tes` — spans / captioned math+code+mermaid+table / lang / align
- `attachment_sample.tes` — inert attachment (safe basename + PDF bytes + caption)
- `slide_deck.tes` — two region-based slides
- `research_cite.tes` — cite chunk + citation link
- `figure_sample.tes` — image + figure (with caption)

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
| `caption_on_paragraph.tes` | `caption` set on `paragraph` (only table/math/code_block allowed) |
| `oversized_text_header.tes` | Text header JSON > 4 KiB |
| `unsafe_attachment_filename.tes` | Attachment basename with `../` |
| `unknown_required_feature.tes` | Catalog `features.required` names an unknown id |

### Integer overflow / fuzz seeds (hand-kept; no regen script yet)

| File | Fault |
| --- | --- |
| `region_offset_length_overflow.tes` | Region offset + length overflows |
| `slide_payload_len_overflow.tes` | Slide payload length overflows |
| `tidx_entry_count_mul_overflow.tes` | `TIDX` entry-count multiply overflows |
| `tlnk_entry_count_mul_overflow.tes` | `TLNK` entry-count multiply overflows |

These are also copied into the fuzz corpus via `mise run fuzz-reseed`.

### Feature-flag accept (deep verify ok, may warn)

| File | Note |
| --- | --- |
| `unknown_optional_feature.tes` | Catalog `features.optional` names an unknown id |

## Regenerate

```bash
# Goldens + accept sync (edit v0 only)
cargo run --example gen_v0_fixtures
cp fixtures/v0/*.tes fixtures/conformance/accept/

# Layout-v1 / attachment / feature-flag rejects (+ unknown optional accept)
cargo run --example gen_conformance_rejects

# Structural rejects (from note_one_chunk.tes)
uv run scripts/gen_structural_rejects.py

# Sample vault + vault.tes
cargo run --example gen_vault_fixtures
```

Or: `mise run fixtures` (goldens, rejects, vault sample, browse samples, layout smoke).

CI deep-verifies **`conformance/accept` + `reject`** (not a second pass over identical `v0/` bytes). Golden byte equality stays in `src/tests/golden_v0.rs`.
