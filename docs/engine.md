# Tessera reference engine — architecture

**Status:** planned module map for the `tessera` Rust crate and `tes` CLI. No implementation yet.

This doc sits **between** the wire spec and the user-facing CLI: how bytes become documents, how documents become exports, and what is **not** in the engine (GUI, query stack, Tetration dependency).

| Layer | Doc |
| --- | --- |
| Bytes on disk | [layout_v0.md](layout_v0.md) |
| Decoded views | [exports.md](exports.md) |
| CLI commands | [cli.md](cli.md) |
| Design choices | [decisions.md](decisions.md) |

---

## What “the engine” is

The **Tessera engine** is the reference library that:

1. **Reads and writes** sealed `.tes` files (mmap, index lookup, payload slice).
2. **Validates** on-disk health (`verify`).
3. **Imports** foreign formats into chunks **once** (`import`).
4. **Exports** decoded views for humans and models (`export`).
5. **Resolves** cross-document links across a vault (`vault`, later).

It is **not**:

- A GUI or workspace (**Aleph** — future product on top).
- The Tetration tensor query engine (`tet query`, reductions, GPU).
- A dependency on the `tetration` crate.

The CLI (`tes`) is a thin wrapper around library entry points.

---

## Layer model

```text
┌─────────────────────────────────────────────────────────────┐
│  tes CLI  (src/bin/tes.rs)                                  │
└───────────────────────────────┬─────────────────────────────┘
                                │
┌───────────────────────────────▼─────────────────────────────┐
│  Domain layer (document semantics)                          │
│  import/ · export/ · vault/                                 │
│  Markdown/HTML/PDF → chunks · views · link resolution       │
└───────────────────────────────┬─────────────────────────────┘
                                │
┌───────────────────────────────▼─────────────────────────────┐
│  Catalog layer (document model in one file)                 │
│  catalog/ — session writer, chunk payloads, link table      │
└───────────────────────────────┬─────────────────────────────┘
                                │
┌───────────────────────────────▼─────────────────────────────┐
│  Container layer (sealed chunked file)                      │
│  layout/ · verify/ · repair? · utils/wire                   │
│  superblock, TIDX, THST, bounds, codecs                     │
└───────────────────────────────┬─────────────────────────────┘
                                │
                                ▼
                         .tes bytes on disk
```

**Rule:** upper layers call lower layers; container code never imports Markdown parsers or HTML templates.

---

## Planned module map

| Module | Responsibility | Spec / doc |
| --- | --- | --- |
| `layout` | `SuperblockV0`, mmap open, region bounds, magic/version checks | [layout_v0.md](layout_v0.md) |
| `utils::wire` | Little-endian primitives, `align8` | Tetration pattern reuse |
| `catalog::index` | `TIDX` header + 48-byte entries | [layout_v0 — chunk index](layout_v0.md#chunk-index-region) |
| `catalog::catalog` | Document catalog JSON parse/serialize | [layout_v0 — catalog](layout_v0.md#document-catalog) |
| `catalog::link` | `TLNK` table read/write | [layout_v0 — link table](layout_v0.md#link-table-optional) |
| `catalog::chunk` | Text/cite payload encode/decode | [layout_v0 — chunk types](layout_v0.md#chunk-types) |
| `catalog::history` | Optional `THST` footer | [layout_v0 — footer](layout_v0.md#optional-history-footer-v0) |
| `catalog::session` | `TesWriterSession` — seal one `.tes` | Phase 1 |
| `verify` | Layout health, findings, exit code 1 | [cli — verify](cli.md#tes-verify) |
| `export` | `--raw`, `--ai-text`, `--chunks-jsonl`, … | [exports.md](exports.md) |
| `import` | `--markdown`, `--html`, `--pdf`, … | [decisions](decisions.md), Phase 4+ |
| `vault` | Multi-file link resolve, backlinks, `vault.tes` | Phase 5 |

`repair/` is **optional** post–v0 (Tetration parity); not in initial module tree.

---

## Read path

```mermaid
flowchart LR
    P["path.tes"] --> M["layout::open_mmap"]
    M --> S["parse SuperblockV0"]
    S --> C["catalog: JSON + TIDX + TLNK"]
    C --> I["index row → payload_offset"]
    I --> SL["slice payload bytes"]
    SL --> X["export view or info summary"]
```

**Steps (library):**

1. **`layout::open_mmap`** — read-only map; file length for bounds.
2. **`layout::read_superblock_v0`** — validate `TESS`, version `0`, offsets.
3. **`catalog::read_catalog`** — optional JSON blob → `DocumentCatalog`.
4. **`catalog::read_index`** — parse `TIDX` header + fixed entries.
5. **`catalog::read_link_table`** — optional `TLNK` entries.
6. **Payload access** — `mmap[off..off+len]`; zstd decode if `codec = 1`.
7. **Consumers** — `tes info` summarizes; `export` decodes text headers + bodies.

Reads do **not** re-parse Markdown or HTML. Canonical text is already in chunk bodies.

---

## Write path

```mermaid
flowchart LR
    W["TesWriterSession::create"] --> SB["write superblock placeholder"]
    SB --> CAT["append catalog JSON"]
    CAT --> PAY["append chunk payloads"]
    PAY --> IDX["write TIDX + patch offsets"]
    IDX --> LNK["optional TLNK"]
    LNK --> SEAL["finalize superblock · optional THST"]
```

**Steps (library):**

1. **`TesWriterSession::create(path)`** — exclusive create; in-memory index rows.
2. **Catalog** — write JSON at planned offset; record in superblock fields.
3. **Chunks** — append payloads (text UTF-8 default raw); push index rows with `chunk_id`, type, offsets.
4. **Link table** — if links/cites added, write `TLNK` after catalog or before index (reference writer: before index).
5. **Chunk index** — write `TIDX` header + rows; set `chunk_index_offset/length` in superblock.
6. **Finalize** — rewrite superblock with final offsets; optional `THST` + set `flags & 1`.

Single-writer, sealed file — same concurrency model as Tetration ([layout_v0 — concurrency](layout_v0.md#concurrency-informative)).

**Import path** builds chunks in memory then calls the same session API (`import` → `session`).

---

## Export and import (domain layer)

| Direction | Module | Input → output |
| --- | --- | --- |
| **Export** | `export` | mmap’d `.tes` → UTF-8 view (stdout or file) |
| **Import** | `import` | foreign file → `TesWriterSession` → `.tes` |

Export **never** writes back to canonical chunks except via explicit re-import. Import **parses once** at boundary; see [decisions — parse once](../README.md#design-principles).

View contracts: [exports.md](exports.md). CLI flags: [cli.md](cli.md).

---

## Verify

`verify` sits in the container/catalog boundary:

| Check | Layer |
| --- | --- |
| Magic, version, offset arithmetic | `layout` |
| Catalog JSON schema | `catalog` |
| `TIDX` / `TLNK` magic and entry bounds | `catalog` |
| Payload bounds, UTF-8 sample decode | `catalog` + `verify` |
| `THST` when flagged | `catalog::history` |

Report shape follows Tetration (`TetVerifyReport`-style): findings, severity, JSON/text formatters — implemented fresh in `tessera`, not imported from `tetration`.

---

## Relationship to Tetration

| Tetration | Tessera engine |
| --- | --- |
| `layout.rs` + `catalog/` | Same **pattern**, different superblock/index/catalog |
| `query/` (~100+ files) | **Absent** — replaced by `export/` views |
| `convert/` (HDF5, NetCDF, Zarr) | **Absent** — replaced by `import/` (MD, HTML, PDF) |
| `export/zarr` | **Absent** — `export` text/HTML/JSONL |
| `verify/`, `repair/` | **Similar structure**, document-specific checks |
| `THST`, `TIDX` header, codec 0/1 | **Reuse ideas**; copy code selectively in v0 |

**No `tetration` dependency.** After v0 ships, compare duplicated footer/wire/index code; extract a thin shared crate only if duplication hurts ([decisions D7](decisions.md#tetration-wire-reuse)).

---

## v0 engine scope (Phase 1–3)

**In:**

- `layout`, `catalog` (session, index, catalog JSON, text chunks)
- `verify` (basic)
- `export` (`--raw`, `--linear`, `--ai-text`, `--chunks-jsonl`)
- `tes info`, `tes verify`, `tes export`

**Out (later phases):**

- `import/` (Phase 4+)
- `vault/` (Phase 5)
- `catalog::link` write path on every save (Phase 5)
- slide/image/page payloads (wire stub only)
- `repair/`, `THST` on every save
- Aleph, CRDT, page tensors

---

## Public API sketch (library)

Embedders import `tessera::prelude` (planned):

| Type / fn | Role |
| --- | --- |
| `TesFile` | mmap’d open document |
| `TesWriterSession` | create / append / commit |
| `verify_tes_file` | health report |
| `export_view(path, ExportView::AiText)` | decoded projection |
| `import_markdown_v0` | Phase 4 |

CLI mirrors these; no duplicate logic in `src/bin/tes/`.

---

## Testing strategy

| Layer | Tests |
| --- | --- |
| Container | Golden byte fixtures ([layout_v0 — fixtures](layout_v0.md#golden-fixtures-planned)) |
| Round-trip | write session → mmap read → same index/catalog |
| Export | golden `.txt` / `.jsonl` diffs |
| Verify | corrupt fixtures → exit 1, expected findings |
| Import | MD → tes → export MD structure (Phase 4) |

---

## See also

- [roadmap.md](roadmap.md) — phase order and first issues
- [format-comparison.md](format-comparison.md) — why HTML is export, not canonical
- [glossary.md](glossary.md) — terms
