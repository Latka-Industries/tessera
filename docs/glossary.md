# Glossary

Terms used consistently across Tessera docs, issues, and code.

---

## Format and file

| Term | Definition |
| --- | --- |
| **Tessera** | Open document format and reference Rust crate; `.tes` files. |
| **`.tes`** | Tessera document file; one primary document per file in v0. |
| **`tes`** | CLI binary for info, verify, export, import, link. |
| **Aleph** | Reserved name for a future GUI/knowledge workspace built on Tessera — not part of v0 wire format. |
| **Wire format** | On-disk bytes: superblock, catalog, index, payloads. See [layout_v0.md](layout_v0.md). |
| **Layout version** | `u32` after magic; **`0`** in this spec. |
| **Superblock** | Fixed 64-byte header at file offset 0. |
| **Canonical** | Stored in `.tes`; not Markdown/HTML/PDF source. |

---

## Document structure

| Term | Definition |
| --- | --- |
| **Chunk** | Addressable payload unit with an index row (text, image, cite, …). |
| **Chunk id** | Stable `u64` within one file; reference writer uses 1-based ids. |
| **Chunk type** | `text`, `image`, `link`, `cite`, `slide`, `page` — see [layout_v0](layout_v0.md#chunk-types). |
| **Text chunk** | UTF-8 body + JSON header (`role`, `level`, …). |
| **Reading order** | Chunks with `chunk_flags & 1`, sorted by `chunk_id`. |
| **Catalog** | UTF-8 JSON blob: title, `doc_id`, tags, timestamps. |
| **`doc_kind`** | Document mode: `note`, `document`, `hub`, `research`, etc. |
| **`doc_id`** | UUID string identifying one document across a vault. |

---

## Graph and vault

| Term | Definition |
| --- | --- |
| **Vault** | Folder of `.tes` files; optional `vault.tes` index. |
| **Link** | Pointer from source chunk to `target_doc_id` (+ optional chunk). |
| **Link table** | `TLNK` region listing outbound links for backlink index. |
| **Backlink** | Inverse of a link — computed from link tables across vault. |
| **Hub doc** | `doc_kind = hub`; ordered links acting as map-of-content. |
| **Cite / citation** | Quote + pointer to source doc/chunk/byte range; cite chunk or link kind 2. |

---

## Presentation

| Term | Definition |
| --- | --- |
| **Structure** | Blocks, headings, slide order — stored in chunks. |
| **Theme** | CSS (or design tokens → CSS) applied at **export**, not canonical. |
| **Template** | Allowed block types + default theme + export targets. |
| **Export / view** | Decoded projection: `--raw`, `--ai-text`, `--html`, etc. |
| **Parse once** | Import or save compiles markup into chunks; reads do not re-parse Markdown/HTML. |

---

## Technical (Tetration-aligned)

| Term | Definition |
| --- | --- |
| **`TIDX`** | Chunk index region magic. |
| **`TLNK`** | Link table magic. |
| **`THST`** | Optional history footer magic at EOF. |
| **Codec `0` / `1`** | Raw payload vs zstd-compressed. |
| **mmap** | Memory-map file for zero-copy payload slices. |
| **Sealed file** | Single writer finished file; safe for concurrent readers. |

---

## Modes (authoring)

| `doc_kind` | Typical use |
| --- | --- |
| `note` | Short-form capture |
| `document` | Long-form essay/report |
| `manuscript` | Fiction, chapters |
| `research` | Papers, lit notes, cites |
| `deck` | Slides |
| `wiki_page` | Standalone wiki article |
| `hub` | Table of contents / MOC |
| `index` | Vault catalog sidecar |

See [README — Writing modes](../README.md#writing-one-format-many-kinds-of-work).

---

## See also

- [engine.md](engine.md) — crate architecture, Tetration boundary
- [format-comparison.md](format-comparison.md) — Tessera vs HTML, PDF, DOCX, Markdown
- [decisions.md](decisions.md) — v0 design choices
- [exports.md](exports.md) — view contracts
