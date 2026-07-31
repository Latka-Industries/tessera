# Glossary

Terms used consistently across Tessera docs, issues, and code.

---

## Format and file

| Term | Definition |
| --- | --- |
| **Tessera** | Open document format and reference Rust crate; `.tes` files. |
| **`.tes`** | Tessera document file; one primary document per file in v0. |
| **`tes`** | CLI binary: info, verify, export, import, link, vault, serve, edit/apply, history. |
| **GUI (unnamed)** | Future GUI/knowledge workspace built on Tessera — product name TBD; not part of v0 wire format. |
| **Wire format** | On-disk bytes: superblock, catalog, index, payloads. See [layout_v0.md](layout_v0.md). |
| **Layout version** | `u32` after magic; **`0`** in this spec. |
| **Superblock** | Fixed 64-byte header at file offset 0. |
| **Canonical** | Stored in `.tes`; not Markdown/HTML/PDF source. |
| **Tessera Markdown / Tessprek** | Markdown plus narrow Tessera attributes used as a lossless edit projection; not canonical storage. |

---

## Document structure

| Term | Definition |
| --- | --- |
| **Chunk** | Addressable payload unit with an index row (text, image, cite, …). |
| **Chunk id** | Stable `u64` within one file; reference writer uses 1-based ids. |
| **Chunk type** | `text`, `image`, `link`, `cite`, `slide`, `page` — see [layout_v0](layout_v0.md#chunk-types). |
| **Text chunk** | UTF-8 body + JSON header (`role`, `level`, …). |
| **Inline span** | Validated UTF-8 byte range plus an enum meaning such as strong, underline, math, link, or citation. |
| **Figure ref** | One contextual use of an image chunk, with alt text, caption, placement, and reading-order position. |
| **Attachment** | Inert generic file bytes plus media type and safe filename. |
| **Reading order** | Chunks with `chunk_flags & 1`, sorted by `chunk_id`. |
| **Catalog** | UTF-8 JSON blob: title, `doc_id`, tags, category, section, aliases, slug, timestamps. |
| **`doc_kind`** | Document mode: `note`, `document`, `hub`, `research`, etc. |
| **`doc_id`** | UUID string identifying one document across a vault. |
| **`slug`** | Optional vault-unique human handle (often from Obsidian `id:`). |
| **`category`** | Optional single primary bucket (e.g. top-level folder); tags remain cross-cutting. |
| **`section`** | Optional ordered path under `category` (e.g. `Books/Authors`); nested folders, not tags. |
| **`aliases`** | Alternate display / wikilink names. |

---

## Graph and vault

| Term | Definition |
| --- | --- |
| **Vault** | Folder of `.tes` files plus optional registered external files/roots; optional `vault.tes` index/manifest. |
| **Link** | Typed pointer to an internal doc/chunk, external URI, or attachment. |
| **Link table** | `TLNK` region listing outbound links for backlink index. |
| **Backlink** | Inverse of a link — computed from link tables across vault. |
| **Hub doc** | `doc_kind = hub`; ordered links acting as map-of-content. |
| **Cite / citation** | Quote + pointer to source doc/chunk/byte range; cite chunk or link kind 2. |

---

## Presentation

| Term | Definition |
| --- | --- |
| **Structure** | Blocks, headings, slide order — stored in chunks. |
| **Theme pack** | External versioned manifest + CSS/assets applied at export; referenced by id/hash, not canonical content. |
| **Template** | Theme pack plus allowed blocks, `doc_kind` defaults, cite style, slide regions, export targets, and optional starter Tessera Markdown. |
| **Export / view** | Decoded projection: `--raw`, `--ai-text`, `--html`, etc. |
| **Parse once** | Import or save compiles markup into chunks; reads do not re-parse Markdown/HTML. |
| **Render** | Generated HTML, PDF, deck, or AI view; never stored as canonical layout. |

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
| **Must-understand feature** | Feature an older reader must reject if unknown; unknown optional features may be skipped with a warning. |
| **Feature flags** | Catalog `features.optional` / `features.required` lists; see [layout_v0](layout_v0.md#catalog-features-forward-compatibility). |

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
- [structure_v1.md](structure_v1.md) — accepted next semantic model
- [security.md](security.md) — untrusted file, theme, attachment, and write safety
- [format-comparison.md](format-comparison.md) — Tessera vs HTML, PDF, DOCX, Markdown
- [decisions.md](decisions.md) — v0 design choices
- [exports.md](exports.md) — view contracts
