# Tessera

**An open document format anyone can use to write, take notes, build wikis, and make presentations — fast on disk, friendly to AI.**

Tessera is a separate project from [Tetration](https://github.com/thicclatka/tetration) (`.tet` numeric tensors). Same _engineering pattern_ — mmap-friendly binary, catalog + chunk index, partial I/O, verify on save, CLI + library — applied to **human-authored content**, not scientific arrays.

The goal is not “yet another PDF.” It is a **native format** (proposed `.tes`) that any app can read or write: a writer GUI, a notes app, a slide tool, import from Word/PDF, export to Markdown — with **built-in views** so models get clean text without parsing Office formats.

---

## Problem

Today we overload the wrong formats:

- **Word / PDF** — great for printing, painful for partial read, linking, and AI (order, tables, figures).
- **Markdown folders** — great for git and wikis, fragile links (`[[titles]]`), weak citations, no single fast binary artifact.
- **Presentation tools** — slides locked in vendor files; hard to reuse prose, images, or citations across doc ↔ deck.

We want **one format** that authors, apps, and models can share: open spec, chunked binary for speed, exports when you need plain text or markdown.

## Approach

1. **Canonical binary (fast path)** — one file (or small family of files) with a catalog, chunk index, and payloads. Computers mmap and touch only the spans they need (pages, text chunks, image blobs).

2. **Built-in exports (AI path)** — models and pipelines do not eat the wire format. The engine exposes views such as:
   - linear reading-order text (UTF-8, de-hyphenated, tables sane)
   - chunk JSONL for RAG
   - per-page raster or tensor for vision
   - resolved citations (`doc`, chunk, byte range → quote + page)

3. **CLI + library (human + agent path)** — same operations for scripts and automation: `info`, `verify`, `export`, `link`, `cite`, etc.

4. **Authoring apps (writer path)** — GUI(s) for everyday use: long documents, quick notes, slide decks. Users never edit chunk indices; save writes valid `.tes`. Markdown/PPTX/PDF are **exports or imports**, not the source of truth.

## Writing: one format, many kinds of work

Tessera is a **writer’s format first** — not only wikis and slides. The same `.tes` container and chunk types support different **writing modes** via `doc_kind`, metadata, default theme, and how you slice text into chunks. You pick a mode when you create a file (or the app infers it); the engine does not fork the wire format.

| Mode                 | What you’re doing                                        | Typical shape                                                                          | Export you care about                                   |
| -------------------- | -------------------------------------------------------- | -------------------------------------------------------------------------------------- | ------------------------------------------------------- |
| **Short-form**       | Daily notes, meeting captures, ideas, ZK cards           | One file, few text chunks, light metadata                                              | Raw text, Markdown, paste into chat                     |
| **Long-form**        | Essays, reports, manuals, book chapters                  | Many sections/chapters as ordered text chunks; optional hub for TOC                    | Markdown, HTML, print PDF                               |
| **Fiction**          | Novels, stories, scripts                                 | Chapters or scenes as chunks; characters/places as stable link targets                 | Manuscript PDF, ePub-style flow, plain text for editors |
| **Research**         | Papers, lit reviews, reading notes tied to sources       | Text + **cite** chunks; links to PDFs, other `.tes`, or external DOIs                  | Academic PDF, BibTeX-friendly export, clean text for AI |
| **Print / PDF-like** | Documents that must **look** finished on paper or screen | Same structure as long-form; **paginated** theme (margins, running heads, page breaks) | Print-ready PDF (primary); HTML preview                 |

**Short-form** optimizes for speed: open → mmap one text chunk → write like a `.txt` note. Backlinks and tags still work when you link out to longer work. No slide chunks, no chapter machinery unless you promote the note to a longer doc.

**Long-form** optimizes for **structure at scale**: headings map to chunk boundaries (or section records) so you can jump to “Chapter 3” without loading the whole manuscript. A **hub doc** can list parts; cross-links between chapters stay stable IDs, not renamed filenames. Revision history can attach per chunk later without rewriting a megabyte file.

**Fiction** reuses long-form mechanics but different **conventions**: scene/chapter chunking, optional character/location registries (metadata or hub pages), comments and version notes without polluting the reading-order export. Export for beta readers is **themed PDF or clean prose** — not `.docx` with tracked changes unless you import that way.

**Research** adds **first-class citations**: quotes point at `doc` + chunk + byte range (and optional page on imported PDFs). Reading notes in short-form link to paper `.tes` or imported PDF chunks; the graph is native, not a Zotero plugin scanning Markdown. `export_ai_text()` gives models methods/results without `\cite{}` noise; `export_bibliography()` (or similar) generates the publishing layer when you need it.

**Print / PDF-like** is not a separate binary — it is **long-form (or research) + a print theme**. Word and PDF today mix _content_ with _layout_; Tessera keeps structure in chunks and pushes margins, fonts, headers, and page breaks into **CSS + paginated export**. You write in flow; preview toggles “draft” vs “print.” Import from an existing PDF still yields text chunks (+ optional page rasters); re-export aims at **replacement for “send a PDF”** without making PDF the editable source.

Presentations (**decks**) sit beside these: reuse images, citations, and prose chunks from research or long-form docs on slides — same vault, same link IDs.

### What you can store in one file

Same container; **`doc_kind`** (or similar) distinguishes use cases — `note`, `document`, `manuscript`, `research`, `deck`, `wiki_page`, `hub`:

| Chunk / layer       | Documents & notes                   | Presentations                                                        |
| ------------------- | ----------------------------------- | -------------------------------------------------------------------- |
| **text**            | Paragraphs, headings, lists, tables | Speaker notes, titles                                                |
| **slide**           | —                                   | Layout + ordered blocks per slide (background, bullets, image slots) |
| **image**           | Figures, photos                     | Slide media                                                          |
| **page** (optional) | PDF page raster for import          | Slide thumbnail / fixed layout raster                                |
| **link / cite**     | Wiki graph, footnotes, AI quotes    | Link to source doc chunks                                            |
| **metadata**        | Title, tags, created, template      |

**Notes** (`note` / short-form) are the same format with a small footprint. **Manuscripts** and **research** docs use the same text and cite chunks with richer metadata and print-oriented themes when needed. **Decks** add slide chunks but still share text, images, and links — e.g. cite a paper from slide 4, backlink from your research notes.

Optional **page-as-tensor**: render a page or slide to a fixed `[H,W,C]` grid for vision models; **prose for LLMs** still comes from **text chunks**, not pixels alone.

### Corpus: many files, hub docs, no re-parsing

You still have **many documents** (one `.tes` per note, article, deck, etc.). The difference from an Obsidian-style folder of `.md` files:

|             | Markdown vault                   | Tessera vault                                                           |
| ----------- | -------------------------------- | ----------------------------------------------------------------------- |
| Link        | Parse `[[title]]` strings        | Read `target_doc` + `target_chunk` from index                           |
| Backlinks   | Scan all files or plugin index   | Updated on save in link table                                           |
| TOC / MOC   | Hand-written wikilinks in a note | **Hub doc** — a first-class file whose chunks are an ordered link index |
| Rename note | Find/replace link text           | Rename updates one ID; backlinks follow                                 |
| AI text     | Strip markup from source         | Read **text chunks** (UTF-8), no `\frac`, `**`, etc.                    |

Inside the vault, **nothing is interpreted as markup on read**. Text is parsed once at **import** (Word/PDF/Markdown) or composed on **save** from the GUI. After that, tools do **binary decode + index lookup**, not “lex this file again.”

**Hub / index documents** (`doc_kind = hub` or `index`): a small `.tes` that is only navigation — sections, blurbs, and stable links to other files. Open the hub → mmap a few KB → jump anywhere. Same role as a Map of Content in ZK, but native to the format.

Optional **vault catalog** (sidecar or special root file): `doc_id → title, tags, modified` for search and graph view without opening every note.

### As fast as plain text (when you want raw)

People like `.md` because the editor shows the file bytes directly and I/O is simple. Tessera can match that in practice:

- **Text chunks** store **uncompressed UTF-8** by default (optional zstd for images and history).
- **Open note** = mmap + jump to text chunk by offset (catalog in header), no lexer on every keystroke.
- **Preview / “raw” pane** = zero-copy slice of the text chunk payload — what you see is what the AI gets.
- **`tes export --raw`** (or per-chunk dump) → a `.txt` file anytime.

Tradeoffs: Finder won’t show prose without an app; git diff wants `tes export` or a chunk-aware diff. In return: **linking, backlinks, hubs, and AI export** without scanning 10k markdown files.

---

## Structure vs presentation (themes, not LaTeX in the file)

Layout is the hard part. Tessera avoids “Word coordinates in binary” and “full Typst in one box” by splitting:

| Layer         | Stored in `.tes`                                                                   | Who edits it                           |
| ------------- | ---------------------------------------------------------------------------------- | -------------------------------------- |
| **Structure** | Blocks, slide order, links, semantic hints (`heading`, `emphasis`, `slide_layout`) | GUI and/or light markup mode           |
| **Theme**     | CSS (or design tokens → CSS), global or per-file                                   | Power users; gallery for everyone else |
| **Render**    | _Not stored_ — HTML, PDF, slides via export                                        | Engine applies structure + theme       |

**Figma-like model:** content is tiles; **injectable CSS** (or template packs) controls look. A template is a manifest: allowed block types + `theme.css` + export targets (pdf, html, deck).

- **Notes (short-form)** — minimal template, typography-only CSS.
- **Long-form & fiction** — flow layout; chapter breaks; optional drop caps / scene breaks in theme only.
- **Research** — citation blocks, figure/table numbering, two-column optional in print theme.
- **Print / PDF-like** — `@page`, margins, running heads, page-break rules; separate “screen draft” theme.
- **Decks** — slide masters via CSS grid / regions (`title`, `body`, `media`).

Power users can attach **`local.css`** or classes on blocks (`pull-quote` → `.pull-quote { … }`). Everyone else picks **“Academic”** or **“Dark notes”** from built-in themes. Markdown and Typst remain **export targets**, not the canonical representation.

---

## Clean text for AI (no escape hell)

Canonical chunks hold **reading-order UTF-8** and structure — not publishing syntax.

```text
chunk 12 → "Methods\nWe measured …"
```

not LaTeX/Markdown source with `\section{}`, `**`, `\cite{}`, or PDF extraction noise.

Bold, headings, and citations are **fields on the chunk** (or parallel link records). Export to Markdown/LaTeX **generates** escapes; `export_ai_text()` does not strip them. Optional markup authoring compiles **into** chunks, like writing to an IR.

---

## Why Markdown wikis feel scattered (what we fix)

Obsidian is popular because plain files are portable — but the **vault is a convention on top of text**, not one system:

- One note per file → corpus feels like a tree of files, not one space.
- `[[links]]` by display title → renames and duplicates break mental models.
- Graph, backlinks, and properties come from **plugins and rescans**, not the format.
- “Interactive” still means typing source; preview is a skin.

Tessera targets **one coherent knowledge space**: native links, hub docs, GUI + CLI on the same binary, Markdown only when you **export for git**. Closer to the _feel_ of an integrated app, without a proprietary cloud silo — if the spec is open and multiple GUIs can implement it.

---

## Relation to Tetration

|                | **Tetration**                                 | **Tessera**                                    |
| -------------- | --------------------------------------------- | ---------------------------------------------- |
| Domain         | HDF5 / NetCDF / Zarr-like **numeric tensors** | DOCX / PDF / authored **documents**            |
| File           | `.tet`                                        | `.tes` (proposed)                              |
| CLI            | `tet`                                         | `tes` (proposed)                               |
| Typical query  | `sum`, `mean`, slice, materialize             | `export-text`, `export-chunks`, `resolve-cite` |
| Default ingest | `tet convert` from scientific formats         | `tes import` from DOCX / PDF / GUI save        |

Reuse the **pattern** (superblock, catalog, chunk index, footer/history, verify/repair, parallel read). Do **not** reuse the tensor dtype / reduction stack as-is.

---

## Design principles

- **Open format, many apps** — spec + reference implementation; anyone can build readers, writers, or plugins.
- **Binary for the machine** — predictable chunk addressing; verify on save; optional compression on heavy payloads only.
- **Feels like plain text in the editor** — mmap UTF-8 text chunks; raw preview; `export --raw` when you want a `.txt`.
- **Structure in the file, presentation in themes** — CSS/templates for print and slides; not glyph-level layout in v1.
- **Exports for humans and models** — `to_markdown()`, `to_ai_text()`, slide outline JSON, PDF — decoded views, not internal blobs.
- **Stable IDs for the graph** — links, citations, and hub docs; wiki `[[links]]` are a projection.
- **Parse once** — import or GUI save builds chunks; reads are index + mmap, not markup re-parse across the vault.

## Non-goals (initial)

- Be the only app — Tessera is the **format**; GUIs and importers are products on top.
- Pixel-perfect clone of PowerPoint or Word feature-for-feature on day one.
- Storing only raw OOXML/PDF bytes without a canonical text/structure layer.
- Full real-time collaboration (v1); revision log / sync later.
- Lossless round-trip to every Markdown or PPTX variant (exports may be lossy).

---

## Status

**Not started** — product sketch and naming only. Implementation order (suggested):

1. Layout v0 spec (chunk types, index entry, magic/footer).
2. Ingest prototype (PDF or DOCX → text chunks + page rasters).
3. Library: mmap open, export `linear_text`, `chunks_jsonl`.
4. CLI: `tes info`, `tes export`, `tes verify`.
5. GUI: short-form notes + long-form editor (sections, links, images).
6. Research mode: cite chunks, import PDF, resolve quotes across corpus.
7. Print export: paginated PDF from structure + theme (PDF-like output).
8. GUI or module: presentations (slides reusing same chunk types).
9. Fiction-friendly exports (manuscript PDF, chapter-scoped chunks).

---

## Name

**Tessera** — a tile in a mosaic; documents as addressable chunks. Sibling metaphor to Tetration without sharing the `tet` CLI or `.tet` format.

---

## Open questions

- Vault layout: folder of `.tes` + `vault.tes` catalog vs single multi-doc archive file.
- Hub doc wire format: ordered link list vs nested sections only.
- Slide model: freeform blocks vs template regions only.
- Chunking policy for RAG (size, overlap, never split tables mid-row).
- CRDT vs revision log for multi-device edit.
- Page tensors in v1 or text + images + slides first.
- Shared Rust wire crate with Tetration vs standalone copy.
