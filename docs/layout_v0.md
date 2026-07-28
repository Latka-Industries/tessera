# Tessera on-disk layout — version 0

**Status:** draft spec for the reference implementation. Describes the **v0** `.tes` container: a fixed **64-byte superblock** at offset 0, optional **document catalog**, optional **link table**, **chunk index**, **chunk payloads**, and an optional **`THST` history footer**.

Related: [layout v1 structure freeze](structure_v1.md),
[exports](exports.md) (decoded views), [engine](engine.md) (crate architecture),
[CLI](cli.md), [decisions](decisions.md), [glossary](glossary.md).

---

## Design goals (v0)

- **mmap-friendly** — fixed index rows; jump to a text chunk by offset without parsing the whole file.
- **Parse once** — markup (Markdown, HTML, DOCX) compiles into chunks on import or GUI save; reads are index + slice.
- **Sibling pattern to Tetration** — superblock → catalog/index → payloads → optional footer; verify on read; sealed single-writer files.
- **Minimal v0** — one document per file; text + link + cite chunks first; slide/image payloads defined but optional in reference writer.

---

## File map

Byte offsets increase left → right. All integers are **little-endian**.  
`align8(n) = (n + 7) & !7`.

### Regions at a glance

| # | Region | Starts at | When present |
| --- | --- | --- | --- |
| 1 | **Superblock** | `0` | always (64 B) |
| 2 | **Document catalog** | `catalog_offset` | `catalog_length > 0` |
| 3 | **Link table** | `link_table_offset` | `link_table_length > 0` |
| 4 | **Chunk index** | `chunk_index_offset` | `chunk_index_length > 0` |
| 5 | **Chunk payloads** | per index row | one span per index entry |
| 6 | **History footer** | EOF suffix | `flags & 1` |

**Empty skeleton file** (valid, no chunks): `chunk_index_length = 0`, `catalog_length` may still hold title/metadata. Minimum file size: **64 bytes** (superblock only).

**Typical note:** superblock → small catalog JSON → chunk index (1+ text rows) → UTF-8 payloads packed after index.

```text
┌──────────┬──────────┬──────────┬──────────┬─────────────┬──────────┐
│ Superblk │ Catalog  │ Link tbl │ TIDX     │ Payloads…   │ THST?    │
│ 64 B     │ variable │ optional │ 32+N×48B │ per row     │ optional │
└──────────┴──────────┴──────────┴──────────┴─────────────┴──────────┘
```

---

## Magic and `layout_version`

| Field | Value |
| --- | --- |
| Magic (bytes `0..4`) | ASCII **`TESS`** |
| `layout_version` (`u32` at offset 4) | **`0`** only (v0) |

Readers reject unknown `layout_version` unless explicitly upgraded.

---

## Superblock v0 (64 bytes)

| Offset | Size | Type | Field | Notes |
| --- | --- | --- | --- | --- |
| 0 | 4 | `[u8;4]` | `magic` | `TESS` |
| 4 | 4 | `u32` | `layout_version` | `0` |
| 8 | 4 | `u32` | `flags` | Bit **`1`**: optional **`THST`** footer at EOF. Otherwise **0**. |
| 12 | 4 | `u32` | `doc_kind` | See [Document kind](#document-kind-doc_kind). |
| 16 | 8 | `u64` | `catalog_offset` | Start of document catalog blob. **0** if absent. |
| 24 | 8 | `u64` | `catalog_length` | Byte length of catalog. **0** if absent. |
| 32 | 8 | `u64` | `link_table_offset` | Start of link table. **0** if absent. |
| 40 | 8 | `u64` | `link_table_length` | Byte length of link table. **0** if absent. |
| 48 | 8 | `u64` | `chunk_index_offset` | Start of chunk index region. |
| 56 | 8 | `u64` | `chunk_index_length` | Byte length of chunk index region. |

**Invariants:**

- `catalog_offset + catalog_length ≤ file_len` when catalog present.
- Same for link table and chunk index.
- Index and catalog regions should be **8-byte aligned** (reference writer uses `align8`).

---

## Document kind (`doc_kind`)

| Value | Name | Typical use |
| --- | --- | --- |
| `0` | `note` | Short-form capture |
| `1` | `document` | Long-form prose |
| `2` | `manuscript` | Fiction / chapters |
| `3` | `research` | Papers, lit notes (+ cite chunks) |
| `4` | `deck` | Presentation (+ slide chunks) |
| `5` | `wiki_page` | Standalone wiki article |
| `6` | `hub` | Map-of-content / TOC index |
| `7` | `index` | Vault catalog sidecar (special file) |

`doc_kind` is duplicated in the catalog JSON for human tools; superblock copy enables fast `tes info` without parsing catalog.

---

## Document catalog

When `catalog_length > 0`, bytes at `catalog_offset` are **UTF-8 JSON** (no BOM):

```json
{
  "doc_id": "550e8400-e29b-41d4-a716-446655440000",
  "title": "Meeting notes",
  "created": "2026-06-05T12:00:00Z",
  "modified": "2026-06-05T12:30:00Z",
  "doc_kind": "note",
  "tags": ["work", "standup"],
  "template_id": "minimal",
  "theme_id": "dark-notes"
}
```

| Field | Required | Notes |
| --- | --- | --- |
| `doc_id` | yes | Stable UUID string; used in cross-doc links |
| `title` | yes | Display title |
| `created`, `modified` | yes | RFC 3339 UTC |
| `doc_kind` | yes | String mirror of superblock enum |
| `tags` | no | String array |
| `template_id`, `theme_id` | no | Export / GUI hints |

v0 does not spill large catalog fields; keep catalog **≤ 16 KiB** (reference writer limit).

Layout v1 plans optional `lang`, `cite_style_id`, template/theme hashes, and
publish metadata (`slug`, description, author, publish date, draft/published,
navigation order, canonical URL). Catalog projections round-trip through
JSON/YAML/TOML; the binary catalog remains canonical.

---

## Link table (optional)

Outbound and internal links for **backlink resolution** without scanning text payloads.

### Link table header (24 bytes)

| Offset | Size | Field | Notes |
| --- | --- | --- | --- |
| 0 | 4 | magic | ASCII **`TLNK`** |
| 4 | 4 | `table_version` | `u32` = **0** |
| 8 | 8 | `entry_count` | Number of fixed entries following |
| 16 | 8 | reserved | Write **0** |

### Link table entry (48 bytes, fixed)

| Field | Type | Notes |
| --- | --- | --- |
| `source_chunk_id` | `u64` | Chunk containing the link anchor |
| `source_byte_start` | `u32` | UTF-8 byte offset in text chunk (optional anchor) |
| `source_byte_end` | `u32` | Exclusive end |
| `target_doc_id` | `[u8;16]` | UUID bytes (RFC 4122 binary) |
| `target_chunk_id` | `u64` | **`0`** = whole document |
| `link_kind` | `u32` | `0` = wiki, `1` = footnote, `2` = citation stub |
| `reserved` | `u32` | **0** |

Total table size: `24 + entry_count × 48`.

**Hub docs** may have many entries with `source_chunk_id` pointing at hub list chunks; see [decisions — hub format](decisions.md#hub-documents).

---

## Chunk index region

Same structural role as Tetration’s `TIDX`: random access to payloads.

### Chunk index header (32 bytes)

| Offset | Size | Field | Notes |
| --- | --- | --- | --- |
| 0 | 4 | magic | ASCII **`TIDX`** |
| 4 | 4 | `index_version` | `u32` = **0** |
| 8 | 8 | `entry_count` | Number of index rows |
| 16 | 16 | reserved | Write **0** |

Total index size: **`32 + entry_count × 48`**.

### Chunk index entry (48 bytes, fixed)

| Field | Type | Notes |
| --- | --- | --- |
| `chunk_id` | `u64` | Stable within file; **1-based** in v0 reference writer |
| `chunk_type` | `u32` | See [Chunk types](#chunk-types) |
| `chunk_flags` | `u32` | Bit **`1`**: reading-order member |
| `payload_offset` | `u64` | File offset to stored bytes |
| `raw_byte_len` | `u64` | Uncompressed payload size |
| `stored_byte_len` | `u64` | On-disk size at `payload_offset` |
| `codec` | `u32` | **`0`** = raw, **`1`** = zstd |
| `reserved` | `u32` | **0** |

**Invariant:** `payload_offset + stored_byte_len ≤ file_len`.

**Reading order:** chunks with `chunk_flags & 1` are sorted by ascending `chunk_id` for `export_linear_text` unless a catalog override is added in v1.

---

## Chunk types

| `chunk_type` | Name | Payload summary |
| --- | --- | --- |
| `1` | `text` | Semantic header + UTF-8 body |
| `2` | `image` | MIME, dimensions, raw image bytes |
| `3` | `link` | Display + resolved target (redundant with link table; optional) |
| `4` | `cite` | Quote span + target doc/chunk/range |
| `5` | `slide` | Layout id + ordered block list |
| `6` | `page` | Imported PDF page raster (optional v0) |
| `7` | `figure` | Contextual use of an image chunk (alt, caption, placement) |

v0 reference implementation **must** support **`text`**; **`image` + `figure`** are
available for media; **`link` table + cite** recommended before slide/page.

---

## Text chunk payload (type `1`)

```text
┌────────────────┬─────────────────────────┐
│ header (JSON)  │ body (UTF-8, no NUL)    │
│ UTF-8, ≤ 4 KiB │ length = raw − header_len │
└────────────────┴─────────────────────────┘
```

Wire layout:

| Offset | Size | Field |
| --- | --- | --- |
| 0 | 4 | `header_byte_len` (`u32` LE) |
| 4 | `header_byte_len` | UTF-8 JSON header |
| 4+header | rest | UTF-8 **body** (reading-order prose) |

### Text header JSON (v0)

```json
{
  "role": "paragraph",
  "level": 2,
  "list_kind": null,
  "emphasis": []
}
```

| `role` | Meaning |
| --- | --- |
| `paragraph` | Body text |
| `heading` | `level` 1–6 |
| `list_item` | `list_kind`: `bullet` \| `ordered` |
| `blockquote` | Pull quote / block quote |
| `code_block` | Monospace block |
| `table` | v0: body holds tab-separated or structured JSON in body (see [decisions](decisions.md#tables-in-v0)) |

**Default codec:** raw (`codec = 0`); text chunks are **uncompressed UTF-8** unless body &gt; 64 KiB (reference writer may zstd at `codec = 1`).

---

## Cite chunk payload (type `4`)

UTF-8 JSON:

```json
{
  "quote": "We measured …",
  "target_doc_id": "660e8400-e29b-41d4-a716-446655440001",
  "target_chunk_id": 12,
  "target_byte_start": 0,
  "target_byte_end": 42,
  "label": "Smith2024",
  "page": 7,
  "source": {
    "cite_key": "Smith2024",
    "entry_type": "article",
    "author": "Smith, Ada",
    "title": "Example",
    "year": "2024"
  }
}
```

`source` holds bibliographic fields for BibTeX/CSL interchange; display style is
selected by catalog/template `cite_style_id`, never stored here. When
`target_doc_id` is set, the writer also mirrors a **link table** row with
`link_kind = 2`. See [exports](exports.md).

---

## Image / slide / page payloads (v0 stubs)

Defined for layout stability; the reference writer may emit
**`UnsupportedChunkType`** until the M7 image and later payload work lands.

- **Image (`2`):** `u32 mime_len | mime | u32 width | u32 height | u64 data_len | bytes`.
  Not reading-order. See [structure_v1 — media](structure_v1.md#media-and-attachments).
- **Figure (`7`):** UTF-8 JSON figure ref pointing at an image chunk id with required
  `alt_text`, optional `caption`, and `placement`. Reading-order.
- **Slide (`5`):** UTF-8 JSON `{ "layout_id", "regions": [{ "name", "chunk_id" }] }`.
  Reading-order. Region targets are text, figure, cite, or image chunks. Theme CSS
  maps region names to grid/flex areas — no freeform coordinates.
- **Page (`6`):** raster bytes + source page number for PDF import.

Image + figure + slide are implemented in the reference writer; page remains deferred.

---

## Payload codecs

| Codec | `stored_byte_len` vs `raw_byte_len` | Use |
| --- | --- | --- |
| **0** raw | equal | Text chunks (default), small metadata |
| **1** zstd | stored ≤ raw | Large images, page rasters, history |

Decode: zstd frame → exactly `raw_byte_len` bytes.

---

## Optional history footer (v0)

When superblock **`flags & 1`**, suffix at EOF (same pattern as Tetration **`THST`**):

| Region | Notes |
| --- | --- |
| `history_json` | UTF-8 JSON: `{ "history": [...], "metadata": {...} }` |
| `history_json_len` | `u64` LE |
| `history_version` | `u32` LE = **0** |
| magic | ASCII **`THST`** |

**`history` row:** `{ "op", "source", "at", "parents"? }` — e.g. `import`, `gui_save`, `tes import`.

Chunk payload bounds use **`file_len − footer_suffix`** when flag set.

---

## Reference writer subset (v0)

The first shipped writer (`TesWriterSession`) MUST:

1. Write valid 64-byte superblock + catalog JSON.
2. Append one or more **text** chunks with raw UTF-8.
3. Write **`TIDX`** index with contiguous payloads after index (typical).
4. Optionally populate **link table** on save when links/cites edited.

NOT required in first merge:

- zstd text compression
- slide/image/page payloads
- `THST` footer (flag **0**)

---

## Concurrency (informative)

Same as Tetration v1:

| Pattern | Supported |
| --- | --- |
| Sealed file, many readers | Yes |
| One writer → many readers | Yes |
| Parallel writers one file | No |
| Read during write | No |

---

## File health (`tes verify`)

Checks (library: `verify_tes_file`):

1. Magic, `layout_version`, offset/length bounds.
2. Catalog JSON parse + required keys.
3. Link table magic + entry bounds.
4. Chunk index magic + `32 + n×48` length match.
5. Payload bounds; optional decode sample (first 32 chunks).
6. Footer `THST` when `flags & 1`.

Exit code **1** on failure (CI-friendly). See [cli.md](cli.md).

---

## Golden fixtures (planned)

| Fixture | Description |
| --- | --- |
| `fixtures/v0/empty.tes` | Superblock only |
| `fixtures/v0/note_one_chunk.tes` | Single paragraph note |
| `fixtures/v0/note_three_chunks.tes` | Heading + two paragraphs |
| `fixtures/v0/hub_links.tes` | Hub doc + link table |

Regenerate via `cargo run --example gen_v0_fixtures` (issue-tracked).

---

## Version evolution

| Version | Theme |
| --- | --- |
| **v0** (this doc) | Text + link table + catalog; mmap index |
| **v1** (frozen direction) | Ranged spans, structured tables, typed links, image/figure split, attachments, templates, feature policy |

Readers MUST reject newer `layout_version` with a clear error until upgraded.

That final rule applies to v0 readers and an unknown whole layout. Layout v1
will distinguish unknown **optional** features (skip with warning) from
unknown **must-understand** features (fail), so additive optional chunks do
not force a format-wide break.

### Accepted v1 direction (not yet wire offsets)

The normative semantic design is [structure_v1.md](structure_v1.md):

- text bodies stay plain UTF-8; inline formatting uses validated ranged enums;
- LaTeX is stored only for math;
- tables become structured rows/cells and supersede v0 TSV;
- code/document language and semantic alignment are explicit;
- links have internal, external URI, and attachment targets;
- image bytes are reusable while each `FigureRef` owns alt/caption/placement;
- generic attachments are inert;
- slides use named template regions, never freeform coordinates;
- `THST` evolves toward content-addressed logical full revisions in M10.

Exact field layouts, discriminants, migration behavior, and golden fixtures
must be specified before incrementing `layout_version`.
