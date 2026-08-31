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
| **Template** | Theme pack plus allowed blocks, `doc_kind` defaults, cite style, slide regions, export targets, and optional starter Tessera Markdown. Optional D23 overlays: sparse `weave.toml` / `typography.toml` / `aliases.toml` / `phrases.toml` / `fonts.toml`, or master `tessera.toml` (THI-367). |
| **Pack-pinned font** | Pack `fonts.toml` id → TTF/OTF bytes loaded into weave `EmitOptions::pinned_faces`; Tessprek `\font{id}{…}` seals to `InlineKind::Font`. Category defaults (`[text|heading|quote|cite].font` in `weave.toml`) use the same pin ids when `TextRun.face` is unset. |
| **Export / view** | Decoded projection: `--raw`, `--ai-text`, `--html`, etc. |
| **Parse once** | Import or save compiles markup into chunks; reads do not re-parse Markdown/HTML. |
| **Render** | Generated HTML, PDF, deck, or AI view; never stored as canonical layout. |
| **Print IR** | Pagination-ready tree Tessera builds from `.tes` for native PDF ([print_ir.md](print_ir.md)). |
| **Per-chunk align** | Tessprek `\block{align=…}` / `\columns{align=…}` → `TextHeader.align` → weave `text_align` (THI-398). |
| **Running heading** | Weave page-chrome `{heading}`: last H1/H2 on or before the page (THI-409). |
| **Footnote / endnote** | Tessprek `\footnote{…}` / `\endnote{…}` → `InlineKind::Note`; native print uses weave `PrintBlock::Note` (THI-396 / 410). |
| **Layout block** | Sealed `place` / `vspace` / `rule` chunk (D24). See [decisions.md — D24](decisions.md). |
| **`ariadnes-weave`** | Separate crate that lays out print IR → deterministic PDF bytes. |
| **PDF backend** | `chromium` (HTML-print, CLI default) or `native` (print IR + weave). |

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
| `hub` | Vault map-of-content (ordered links); **not** in-document `\toc` |
| `index` | Vault catalog sidecar |
| **In-document TOC** | Tessprek `\toc` → sealed `TextRole::Toc`; expands at print/HTML from headings (THI-390). Distinct from hub / Tesscriptor TOC panes. |
| **LOF / LOT** | Tessprek `\lof` / `\lot` → sealed `TextRole::Lof` / `Lot`; expands from float titles by default (`source=title` or `caption`; THI-395). |
| **PDF outline** | Native PDF sidebar bookmarks from heading `dest_id`s (THI-393). Same heading walk as `\toc`, different surface — not body content and not vault hub. |
| **Body columns** | Tessprek `\columns`…`\endcolumns` → `TextRole::Columns` / `ColumnsEnd`; print → weave `PrintBlock::Columns` (THI-391). Optional `\columns{align=…}` region default (THI-398). Not `\row` meta panes. |
| **Titled band** | Tessprek `\theorem` / `\proof` / `\callout` / `\abstract` → `TextRole::Callout`; print → one weave `PrintBlock::Callout` (THI-414 / 412). Kind is IR-only; Tessera owns the visible label. |

See [README — Writing modes](../README.md#writing-one-format-many-kinds-of-work).

---

## See also

- [engine.md](engine.md) — crate architecture, Tetration boundary
- [structure_v1.md](structure_v1.md) — accepted next semantic model
- [security.md](security.md) — untrusted file, theme, attachment, and write safety
- [format-comparison.md](format-comparison.md) — Tessera vs HTML, PDF, DOCX, Markdown
- [decisions.md](decisions.md) — v0 design choices
- [exports.md](exports.md) — view contracts
