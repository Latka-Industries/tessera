# Design decisions

**Status:** v0 defaults plus accepted layout v1 direction. A v1 decision
explicitly supersedes a conflicting v0 decision; shipped v0 readers remain
unchanged until migration code lands.

Related: [layout_v0.md](layout_v0.md),
[structure_v1.md](structure_v1.md), [security](security.md),
[print_ir.md](print_ir.md), [format-comparison.md](format-comparison.md),
[README](../README.md).

---

## Vault layout

**Decision:** A vault is a **folder of `.tes` files** plus an optional **`vault.tes`** sidecar with `doc_kind = index`. Membership is the in-tree scan **union** any registered external `.tes` files or extra roots stored in `vault.tes` (`members`, index version ≥ 2).

| Choice | Rationale |
| --- | --- |
| One file per document | Matches note/wiki mental model; mmap one note without loading corpus |
| Optional `vault.tes` | TOC-style `doc_id → title, tags, category, section, aliases, slug, modified, path` (`tes vault`); list/search without opening every file; also membership manifest for out-of-tree paths |
| External members | Absolute (or vault-relative when under root) paths so Obsidian-style sprawl can join without moving files |
| Shared path set | `tes vault` rebuild/list and `tes link` use the same membership so TOC and graph do not diverge |
| No multi-doc archive v0 | Simpler writer; object-store bundling deferred |

**Rejected for v0:** single tarball archive containing many docs (may revisit for sync/backup). Auto-discovery of arbitrary trees outside registered roots.

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

**Decision:** v0 spec defines **slide chunk type `5`** but the reference writer
does not emit slides until M9. Slides use **template regions**.

| v0 | v1+ |
| --- | --- |
| Wire type stubbed | `layout_id` + refs mapped to CSS grid regions (`title`, `body`, `media`) |
| No freeform pixel layout | CSS grid/flex controls geometry |

**Rejected for v1:** canonical `x/y/w/h`, PowerPoint-compatible masters, and
animation bytecode. Theme CSS supplies visual latitude; trusted external theme
code may add presentation motion.

---

## Tables

**v0 decision:** one text chunk with `"role": "table"` and a UTF-8 TSV body
(still accepted on read).

**v1 decision (supersedes D4):** a structured table payload contains rows and
cells with text, spans, alignment, header status, and row/column spans. One
table remains one reading-order unit. GFM pipe tables and HTML `<table>`
import compile into that structure (`TableData` on the text header; empty body).

Tables and other structured/spanned blocks are never auto-split.

---

## RAG chunking policy

**Decision:** Default export chunk boundaries follow **Tessera text chunks** (authoring boundaries), not arbitrary token windows.

| Parameter | v0 default |
| --- | --- |
| Primary unit | One index row = one RAG row in `chunks_jsonl` |
| Overlap | **None** in v0 |
| Max size | Soft target **8 KiB** for plain, unspanned prose only |
| Tables | Whole table chunk = one RAG row |

Optional **`tes export --chunks-jsonl --max-bytes N --overlap N`** added in a later issue; not v0.

Writers never split inside an inline span or structured payload. Export-time
RAG windows may split a projection without changing canonical chunk ids.

---

## Collaboration / revision

**Decision:** v0/v1 files are **single-writer, sealed files**. No CRDT or live
multi-user edit.

| Shipped | Later |
| --- | --- |
| Source-hash + advisory write lock; `THST` v1 revisions, drafts, diff, review (`tes pending`) | CRDT / live multi-user edit |

M10 represents full logical revisions with shared content-addressed payloads.
Pending authored operations power redline and accept/reject. CRDT/live cursors
remain rejected for v1.

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
| `h1`–`h6`, `p`, `ul/ol/li`, `blockquote`, `pre/code`, `a` display text | `script`, `style`, inline `style=""` |
| `<table>` → `TextRole::Table` + `TableData` (`th`/`td`, `align`, rowspan/colspan) | Nested-table edge cases beyond nearest-ancestor filter |
| Internal document edges → `TLNK` | — |

**Export:** HTML is generated from chunks + theme CSS — not round-tripped from imported HTML source.

Typed internal, external-URI (`http`/`https`/`mailto`), and attachment link
targets ship in `TLNK` (v0 all-internal; v1 + URI heap when needed). Markdown
import persists allowed external `href`s; HTML import still flattens anchors to
display text until wired the same way.

See [format-comparison.md — HTML](format-comparison.md#html--the-closest-cousin-and-the-main-antagonist).

---

## Markdown import / export

**Decision:** **CommonMark + GFM tables** for v0 import:

| Supported | Deferred |
| --- | --- |
| ATX headings, paragraphs, lists, fenced code, blockquotes | Footnotes and raw HTML blocks |
| GFM pipe tables → `TextRole::Table` + `TableData` (header / body / column align) | |
| Link display text; `[text](https://…)` / UUID destinations → `TLNK` + `InlineKind::Link`; vault import resolves `[[wikilinks]]` via title → slug → aliases | Footnote link kinds |

**Export:** `tes export --markdown` generates GFM-ish Markdown from chunks; **lossy** for cite/slide richness.

Layout v1 uses Tessera Markdown (working nickname: Tessprek): Markdown plus
narrow attributes/directives that losslessly preserve ids, spans, placement,
citations, and other enum-backed fields for editor round trips.

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

## Manuscript conventions (`doc_kind = manuscript`)

Fiction / chaptered long-form. Conventions are **authoring and export
conventions**, not a separate wire type:

| Convention | Rule |
| --- | --- |
| Chapter | Text heading **level 1** (`#` / `<h1>`) opens a chapter |
| Scene | Heading **level 2+** stays inside the current chapter |
| Front matter | Chunks before the first H1 are not part of `--chapter N` |
| Chapter index | `--chapter N` is **1-based** (first H1 = chapter 1) |
| Beta-reader PDF | Pack theme id `manuscript` (Courier, double-spaced); auto-selected for PDF when `doc_kind = manuscript` unless `--theme-id` / catalog overrides |

Import stays generic: `tes import --markdown --doc-kind manuscript`.
Chapter-scoped export: `tes export draft.tes --markdown --chapter 2` or
`tes export draft.tes --pdf -o ch2.pdf --chapter 2`.

Native PDF (`ariadnes-weave`) uses print profile `manuscript` for the same
conventions (H1 → always new page); Chromium HTML-print may still use pack
theme CSS until the native backend is default.

---

## Print IR and PDF source of truth

**Decision (D21):** Native PDF layout is owned by a **print IR** consumed by
the **`ariadnes-weave`** crate. Semantic HTML + template CSS remain the
**browser preview / HTML export** path (`tes serve`, `--html`). They are **not**
the source of truth for native pagination.

| Path | Role |
| --- | --- |
| `.tes` → print IR → `ariadnes-weave` → PDF | Deterministic print (target default) |
| `.tes` → HTML + theme CSS → browser / Chromium print | Preview; optional `--backend chromium` |

**Rationale:** Markdown/HTML→PDF toolchains disagree on page breaks. Tessera
should guarantee **replayable unfolding** (especially manuscripts): same file +
same print profile version → same pagination. That requires Tessera-owned
layout, not a CSS engine.

**Rejected as endgame:** WeasyPrint / pure-Rust HTML/CSS clones; Typst or LaTeX
as canonical authoring; storing page breaks in the `.tes` wire; editable PDF.

Normative sketch: [print_ir.md](print_ir.md). Tracker: THI-256 / THI-288.

---

## Page tensors (vision models)

**Decision:** **Out of v0**. Text + images + slides first; page-as-tensor (`[H,W,C]`) is Phase 9+ research.

---

## Naming: Tessera vs GUI

**Decision:** **Tessera** = format (`.tes`, spec, `tes` CLI). A future
GUI/workspace product sits on top (name TBD; do not hard-wire a product name
into the v0 wire format). Docs may say “future GUI” until a name is locked.

---

## Layout v1 structure freeze

The detailed contract is [structure_v1.md](structure_v1.md). Locked decisions:

1. **Inline structure:** pure UTF-8 bodies plus validated ranged
   `InlineKind` enums. Markdown/HTML are projections.
2. **Math:** LaTeX source for math only (`TextRole::Math` and inline math);
   not a whole-document language.
3. **Language:** optional BCP-47 document language plus block override; code
   blocks retain an optional programming language.
4. **Layout intent:** enum-backed alignment and image placement; soft wrap,
   pagination, and computed numbering are renderer concerns. **Native PDF**
   pagination is owned by the print IR / `ariadnes-weave` ([print_ir.md](print_ir.md),
   D21); HTML+CSS themes own browser preview only.
5. **Media:** image bytes are reusable; each `FigureRef` owns contextual alt,
   caption, placement, and reading-order position. Generic attachments are
   inert.
6. **References:** stable semantic anchors; numbers generated on export.
   Citation data is structured; BibTeX/CSL are interchange; cite style comes
   from a template.
7. **Templates:** external versioned packs referenced by id/hash supply CSS,
   defaults, cite style, slide regions, and starter Tessera Markdown.
8. **AI:** Markdown and sanitized semantic HTML are first-class text
   projections; pixels travel as typed multimodal parts. Embeddings stay
   external.
9. **Mutation:** editors and agents use Tessera Markdown or typed operations,
   then compile, verify, and atomically replace under source-hash + advisory
   lock checks.
10. **Evolution:** unknown optional features are skippable with a warning;
    unknown must-understand features fail.
11. **Security/accessibility:** no document macros; theme code is external and
    disabled by default; semantic/a11y verification is first-class. See
    [security.md](security.md).
12. **Search:** native graph + light vault catalog; full-text and embeddings
    are external indexes keyed by stable ids/hashes.

---

## Decision log index

| ID | Topic | Status |
| --- | --- | --- |
| D1 | Vault = folder + optional index file | Accepted |
| D2 | Hub = text chunks + link table | Accepted |
| D3 | Slides use template regions; no canonical freeform geometry | Accepted |
| D4 | Tables = single TSV text chunk | Superseded by D13 for v1 |
| D5 | RAG unit = text chunk, no overlap v0 | Accepted |
| D6 | No CRDT v0 | Accepted |
| D7 | Standalone wire crate | Accepted |
| D8 | HTML import → semantic blocks | Accepted |
| D9 | CommonMark subset import | Accepted |
| D10 | PDF = text + optional rasters | Accepted |
| D11 | Page tensors deferred | Accepted |
| D12 | Ranged enum-backed inline spans | Accepted for v1 |
| D13 | Structured table payload | Accepted for v1 |
| D14 | LaTeX source for math only | Accepted for v1 |
| D15 | Reusable images + contextual figure refs | Accepted for v1 |
| D16 | Typed external/internal/attachment links | Accepted for v1 |
| D17 | External template/theme/cite-style packs | Accepted for v1 |
| D18 | Optional-vs-required forward compatibility | Accepted for v1 |
| D19 | Tessera Markdown virtual editing + typed AI ops | Accepted direction |
| D20 | Content-addressed drafts/review in `THST` | Accepted (M10 shipped) |
| D21 | Print IR + `ariadnes-weave` own native PDF; HTML is preview | Accepted direction |
