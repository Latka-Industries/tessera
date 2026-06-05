# Design decisions (v0)

**Status:** accepted defaults for the first implementation pass. Revise via new ADR entries when layout v1+ requires breaking changes.

Related: [layout_v0.md](layout_v0.md), [format-comparison.md](format-comparison.md), [README](../README.md).

---

## Vault layout

**Decision:** A vault is a **folder of `.tes` files** plus an optional **`vault.tes`** sidecar with `doc_kind = index`.

| Choice | Rationale |
| --- | --- |
| One file per document | Matches note/wiki mental model; mmap one note without loading corpus |
| Optional `vault.tes` | `doc_id → title, tags, modified` for search/graph without opening every file |
| No multi-doc archive v0 | Simpler writer; object-store bundling deferred |

**Rejected for v0:** single tarball archive containing many docs (may revisit for sync/backup).

---

## Hub documents

**Decision:** Hub docs (`doc_kind = hub`) use **ordered text chunks** where each chunk body is a **title + blurb** and the **link table** holds the stable pointer.

| Element | Storage |
| --- | --- |
| Section order | Ascending `chunk_id` with `chunk_flags & 1` |
| Target | Link table: `source_chunk_id`, `target_doc_id`, optional `target_chunk_id` |
| Nested sections | v0: flat list only; nesting via heading levels in chunk headers |

**Rejected for v0:** hub-only binary section type separate from text chunks.

---

## Slide model

**Decision:** v0 spec defines **slide chunk type `5`** but reference writer **does not emit slides** until Phase 8. Slides use **template regions** first.

| v0 | v1+ |
| --- | --- |
| Wire type stubbed | `layout_id` + blocks mapped to CSS grid regions (`title`, `body`, `media`) |
| No freeform pixel layout | Freeform blocks within regions optional later |

**Rejected for v0:** PowerPoint-compatible masters, animations, speaker-notes sync beyond text chunks.

---

## Tables in v0

**Decision:** Tables are **one text chunk** with `"role": "table"` and **body = UTF-8 TSV** (rows newline-separated, cells tab-separated). No mid-row chunk splits.

| Rule | Reason |
| --- | --- |
| Never split a table across chunks in v0 | RAG and export simplicity |
| HTML/Markdown import flattens to TSV on ingest | Parse once |

**Future:** optional structured JSON table body in v1 if TSV proves lossy.

---

## RAG chunking policy

**Decision:** Default export chunk boundaries follow **Tessera text chunks** (authoring boundaries), not arbitrary token windows.

| Parameter | v0 default |
| --- | --- |
| Primary unit | One index row = one RAG row in `chunks_jsonl` |
| Overlap | **None** in v0 |
| Max size | Soft limit **8 KiB** UTF-8 body per text chunk (writer splits paragraphs) |
| Tables | Whole table chunk = one RAG row |

Optional **`tes export --chunks-jsonl --max-bytes N --overlap N`** added in a later issue; not v0.

---

## Collaboration / revision

**Decision:** v0 is **single-writer, sealed files**. No CRDT, no live multi-user edit.

| Shipped | Later |
| --- | --- |
| Optional `THST` history footer (append-only ops log) | Revision log per chunk, sync protocol |

**Rejected for v0:** operational transforms on open file, Google Docs–style sessions.

---

## Tetration wire reuse

**Decision:** **Standalone copy of the pattern**, not a shared Rust crate with Tetration in v0.

| Reuse | Diverge |
| --- | --- |
| Superblock → index → payloads → footer mental model | Magic `TESS`, chunk types, catalog JSON, link table |
| `TIDX`-style index header magic | 48-byte rows (not 104-byte tensor rows) |
| zstd codec id `1` | Text-first codecs policy |

**Rationale:** Domains differ; coupling releases slows both projects. Revisit shared `latka-chunk-io` crate only if duplication hurts.

---

## HTML import

**Decision:** Import strips to **semantic blocks**; HTML **class** attributes map to **optional `class` list in text header JSON** for theme hints.

| Imported | Discarded |
| --- | --- |
| `h1`–`h6`, `p`, `ul/ol/li`, `blockquote`, `pre/code`, `table`, `a[href]` | `script`, `style`, inline `style=""` (v0) |
| `href` → link table entry | DOM id/class soup without semantic role |

**Export:** HTML is generated from chunks + theme CSS — not round-tripped from imported HTML source.

See [format-comparison.md — HTML](format-comparison.md#html--the-closest-cousin-and-the-main-antagonist).

---

## Markdown import / export

**Decision:** **CommonMark subset** for v0 import:

| Supported | Deferred |
| --- | --- |
| ATX headings, paragraphs, `-` / `*` lists, ordered lists, fenced code, blockquotes | GFM tables (import via HTML path first), footnotes, raw HTML blocks |
| `[text](url)` external links only | Wikilinks `[[page]]` compile to link table if vault resolver provided |

**Export:** `tes export --markdown` generates GFM-ish Markdown from chunks; **lossy** for cite/slide richness.

---

## PDF import

**Decision:** Phase 2 import = **text extraction + optional page rasters** stored as **`page` chunks (type 6)** alongside text.

| v0 import | Not v0 |
| --- | --- |
| One `.tes` per PDF; text chunks in reading order | Perfect layout reconstruction |
| Page raster at 150 DPI PNG optional flag | Vector figure extraction |

---

## Export defaults by `doc_kind`

| `doc_kind` | Default human export | Default git/diff export |
| --- | --- | --- |
| `note` | `--raw` or `--markdown` | `--markdown` |
| `document`, `manuscript` | `--markdown` | `--markdown` |
| `research` | `--ai-text` + `--markdown` | `--markdown` |
| `hub` | `--markdown` (link list) | `--markdown` |
| `deck` | `--html` (when implemented) | N/A |

---

## Page tensors (vision models)

**Decision:** **Out of v0**. Text + images + slides first; page-as-tensor (`[H,W,C]`) is Phase 9+ research.

---

## Naming: Tessera vs Aleph

**Decision:** **Tessera** = format (`.tes`, spec, `tes` CLI). **Aleph** reserved for a future GUI/workspace product on top. No Aleph references in v0 wire format.

---

## Decision log index

| ID | Topic | Status |
| --- | --- | --- |
| D1 | Vault = folder + optional index file | Accepted |
| D2 | Hub = text chunks + link table | Accepted |
| D3 | Slides stubbed; template regions later | Accepted |
| D4 | Tables = single TSV text chunk | Accepted |
| D5 | RAG unit = text chunk, no overlap v0 | Accepted |
| D6 | No CRDT v0 | Accepted |
| D7 | Standalone wire crate | Accepted |
| D8 | HTML import → semantic blocks | Accepted |
| D9 | CommonMark subset import | Accepted |
| D10 | PDF = text + optional rasters | Accepted |
| D11 | Page tensors deferred | Accepted |
