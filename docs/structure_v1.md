# Structure freeze for layout v1

**Status:** accepted design direction; most of the semantic model ships as
**additive headers on `layout_version = 0`**. Layout v0 remains the only
`layout_version` value. This document freezes the model that must stay stable
when a must-understand feature eventually bumps the layout version.

Related: [layout v0](layout_v0.md), [decisions](decisions.md),
[exports](exports.md), [print IR](print_ir.md), [roadmap](roadmap.md),
[security](security.md).

---

## Boundary: structure, theme, render

Tessera has four conceptual layers for human output:

1. **Structure** is canonical in `.tes`: typed blocks, inline spans, links,
   citations, media references, stable ids, and document metadata.
2. **Theme/template** is an external, versioned pack referenced by id and hash.
   It supplies **CSS** for browser preview, export defaults, slide regions, cite
   style, and optional trusted presentation code.
3. **Print IR** is a pagination-ready tree built from structure, plus a versioned
   **print profile** (margins, fonts, break policy). See [print_ir.md](print_ir.md).
4. **Render** is generated: Tessera Markdown, semantic HTML, PDF (via
   `ariadnes-weave` or Chromium fallback), slides, or AI inputs.

Store author intent that must survive renderers. Do not store soft wrapping,
pagination, computed figure numbers, or pixel coordinates in `.tes`.

**PDF vs HTML:** native PDF layout is defined by the print IR + profile, not by
re-printing HTML/CSS. HTML remains the preview/interchange sink (D21).

---

## Capability inventory

| Capability | v0 engine | v1 structure decision | Status |
| --- | --- | --- | --- |
| Text blocks | Implemented | Typed roles remain canonical | shipped |
| Inline formatting | Ranged `InlineSpan` + `InlineKind` | Ranged `InlineSpan` + `InlineKind` | shipped (additive header) |
| Math | `TextRole::Math` + inline math spans | LaTeX source for math only | shipped (additive header) |
| Tables | Structured `table` on text header (TSV fallback) | Structured rows and cells | shipped (additive header) |
| Code language | Optional `code_lang` on code blocks | Optional `lang` on code blocks | shipped (additive header) |
| Block captions | Optional `caption` on table / math / code_block (+ figure/attach) | Caption-sized HTML / print | shipped (additive header) |
| Document language | Catalog `language` + block `lang` | BCP-47 catalog value + block override | shipped (additive header) |
| Internal links | Implemented (`TLNK`) | Typed link targets | shipped |
| External links | Discarded | Variable-length external URI target | shipped (TLNK v1 + URI heap) |
| Images | Implemented | Reusable media + contextual figure refs | shipped |
| Attachments | Inert attachment chunk (type 8) | Generic inert attachment chunk | shipped |
| Citations | Cite wire + BibTeX/CSL | Cite writer, spans, styles, interchange | shipped (M8) |
| HTML | Import/export implemented | AI-safe and themed profiles remain distinct | shipped (M6) |
| Browser preview | `tes serve` + theme packs | Theme packs + safe reload | shipped (M7) |
| PDF | `tes export --pdf` | HTML + print theme pipeline | shipped (M7) |
| Slides | Region-based slide chunks | Named regions, no freeform coordinates | shipped (M9) |
| Layout blocks | `ChunkType::Layout` + `place`/`vspace`/`rule` | Closed ops; weave paints; not pack macros | shipped (D24 / THI-363..364) |
| History (first slice) | `save` / `log` / `diff` / `changelog` | Content-addressed revisions + drafts | shipped (M10) |
| History (checkout / textconv / merge) | Materialize revisions; git Tessprek + verified merge | `export-revs` / `checkout` / `textconv` / `merge-file` | shipped (M10) |
| History (redline) | Footer `pending` reserved | Authored ops + accept/reject | shipped (M10) |
| Vault graph | Implemented | Light `vault.tes` catalog (`tes vault`) + multi-root | shipped |
| Full-text search | Parallel scan + optional Tantivy under `.tessera/fts` | External to wire (sidecar / projected text) | shipped (sidecar) |
| Embeddings | Missing | External to `.tes` | out of wire |

---

## Typed Rust model

Closed semantic vocabularies are Rust enums with stable serde names. JSON is a
wire encoding, not an untyped application API.

```rust
pub enum InlineKind {
    Emphasis,
    Strong,
    Underline,
    Code,
    Term,
    Quote,
    Math { tex: String },
    Link { link_id: u64 },
    Citation { cite_chunk_id: u64 },
    Font { font_id: String }, // pack-pinned TTF (D23 / THI-356)
}

pub struct InlineSpan {
    pub start: u32,
    pub end: u32,
    pub kind: InlineKind,
}
```

`start..end` is a half-open UTF-8 byte range. Writers reject ranges that are
out of bounds, empty, not character boundaries, or crossing. Proper nesting
is allowed. A block containing spans or a structured payload is never
auto-split; the existing 8 KiB target applies only to plain prose.

Block-level author intent is also typed:

- `TextRole` and `ListKind` remain enums.
- code blocks gain optional `lang`;
- blocks gain optional BCP-47 `lang` and `TextAlign`;
- `TextAlign` is `Start | Center | End | Justify`, never physical
  left/right.
- `list_item` gains optional `list_depth` (1 = top-level, 2+ = nested);
  absent means depth 1. Indentation is derived for Markdown/HTML themes.
- Optional `indent` (0..=16) is the print **band** level (orthogonal to
  `list_depth`). Points = `level ×` weave `prose.indent.step` (resume densify
  uses 14pt). Absent / 0 = content margin.

Soft wrap, hyphenation, widows/orphans, and pagination belong to the renderer.
Intentional hard breaks remain content. Authored multi-column body flow is
Tessprek `\columns`…`\endcolumns` → weave `PrintBlock::Columns` (THI-391).

---

## Math and tables

Math uses LaTeX source because authors, LLMs, and publishing tools already
interoperate with it:

- block math: `TextRole::Math`, body is LaTeX;
- inline math: `InlineKind::Math { tex }`;
- Tessera Markdown accepts `$...$`, `$$...$$`, and fenced math;
- HTML/PDF render through MathML/KaTeX or the selected print engine.

LaTeX is not the document authoring language.

The v0 TSV table decision is superseded for v1. A structured table payload
contains rows and cells; each cell may carry UTF-8, inline spans, alignment,
header status, row/column spans, and references. One table remains one
reading-order unit. Markdown/HTML tables compile into this structure.

---

## Links, notes, and references

```rust
pub enum LinkTarget {
    Internal { doc_id: Uuid, chunk_id: Option<u64> },
    External { uri: String },
    Attachment { chunk_id: u64 },
}
```

External URI bytes live in a variable-length URI heap after the fixed `TLNK`
rows (table version **1**), not in the fixed v0 row. Inline spans reference
link records by `link_id` (0-based table index). All-internal tables still
encode as version **0** for golden compatibility.

Footnotes, figures, tables, sections, and citations use stable semantic
anchors. Display numbers are generated by the exporter/theme, so reordering
never leaves stale “Figure 3” text. Comments and review threads anchor to a
chunk id plus an optional span.

Citation chunks hold structured source data and resolved ranges. In-text form
and bibliography style come from `cite_style_id` in the template/catalog.
BibTeX and CSL JSON are import/export formats, not canonical payload syntax.

---

## Media and attachments

Image bytes are separate from each use of the image:

```rust
pub struct ImagePayload {
    pub media_type: String,
    pub width_px: u32,
    pub height_px: u32,
    pub data: Vec<u8>,
}

pub struct FigureRef {
    pub image_chunk_id: u64,
    pub alt_text: String,
    pub caption: Option<String>,
    pub placement: ImagePlacement,
}

pub enum ImagePlacement {
    Flow,
    FullWidth,
    FloatStart,
    FloatEnd,
    Inline,
    Region { name: String },
    Background,
}
```

One image payload can be reused with different captions and placement without
duplicating bytes. Every figure use has a reading-order anchor and alt text.
Actual width, crop, margins, and responsive behavior come from the theme.

A generic attachment chunk stores media type, safe basename filename, optional
caption, SHA-256 integrity hash, and inert bytes (`ChunkType::Attachment` = 8).
Attachments never auto-extract or execute; preview serves them only as
`Content-Disposition: attachment` downloads with `nosniff`.

---

## Templates, preview, PDF, and slides

A Tessera template is a folder/pack with a versioned manifest:

- id, version, compatible structure/features;
- CSS themes (HTML / Chromium only) and optional fonts/assets;
- optional `weave.toml` (or manifest `weave` path) — sparse overlay on
  ariadnes-weave `LayoutKnobs` for `--backend native` (D23); optional
  category `font` pin ids (`[text|heading|quote|cite].font`, THI-360);
- optional `typography.toml` / `aliases.toml` / `phrases.toml` — expand once at
  `tes format` / edit-write (D23 / THI-354 / THI-355); sealed body stores
  results (`\phrase{key}{arg…}` → ordinary Tessprek; `{argN}`/`$N` slots; lossy);
- optional `fonts.toml` + font files — pack-pinned TTFs for `\font{id}{…}` and
  category defaults (D23 / THI-356 / THI-360); seals to `InlineKind::Font`;
  native PDF → `pinned_faces`;
- optional `tessera.toml` (or manifest `pack`) — master form of those overlays
  (THI-367 / Tessera 0.2.7; see [decisions.md — D23](decisions.md));
  `tes-lsp` completes font / phrase / alias ids from the same pack (THI-369);
- allowed block types and `doc_kind` defaults;
- export targets and starter Tessera Markdown;
- named slide regions;
- citation style id or cite-style pack.

The catalog stores `template_id`, `theme_id`, `cite_style_id`, and optional
pack hashes. Pack bytes are external by default; standalone exports may embed
CSS/assets.

`tes serve` projects `.tes` to HTML and applies a draft or print theme.
`tes export --pdf` defaults to HTML + print-theme + Chromium; `--backend native`
uses print IR → `ariadnes-weave` (optional pack `weave.toml` / `fonts.toml`).
Browser preview and PDF are two sinks of shared `.tes` structure, not one CSS
engine.

Slides store `layout_id` plus named region slots (`title`, `body`, `media`,
etc.) referencing text/image/cite chunks. CSS grid/flex gives the renderer
wide latitude. Freeform `x/y/w/h` coordinates are not part of layout v1.
Animation is theme-layer presentation, not chunk data.

### Layout blocks (D24)

Reading-order **layout chunks** (`ChunkType::Layout` = `9`) seal a short
ordered list of closed ops: `place`, `vspace`, `rule`. Packs never invent ops
or Tessprek commands. Optional feature id: `layout`.

Normative rules (units, flush-at-`frac=1`, errors, projections):
[decisions.md — D24](decisions.md). Wire sketch: [print_ir.md](print_ir.md).
Authoring: [tessprek.md](tessprek.md) (`\layout{…}`).

---

## Tessera Markdown and editing

**Tessera Markdown** (working nickname: Tessprek) is Markdown plus narrow
extensions that map directly to typed fields: alignment, classes, image
placement, stable targets, citations, and slide regions.

It is a compile/edit projection, not another canonical file:

```text
Tessera Markdown -> typed blocks/spans -> .tes
.tes -> typed blocks/spans -> Tessera Markdown
```

Vim/Neovim and other editors may expose a virtual Tessera Markdown buffer for
a `.tes` path. `edit-read` decodes; `edit-write` compiles to a temporary file,
deep-verifies, and atomically replaces the original. Lossless edit projection
preserves ids and unsupported metadata. Writes require a source hash and an
advisory per-file lock.

AI reads Markdown or semantic HTML. AI writes Tessera Markdown or a closed
Rust `TesOp` enum through the same compile/verify/replace gate; models never
emit raw `.tes` bytes.

Catalog metadata round-trips separately:

```text
tes export doc.tes --meta toml
tes meta set doc.tes --from meta.toml
```

YAML and JSON are equivalent projections. Metadata import changes the catalog,
not body chunks.

---

## AI projections and images

Markdown is the compact default LLM projection. Semantic HTML is also
first-class for tasks that benefit from explicit element boundaries. AI HTML
is a sanitized fragment: no CSS, scripts, navigation, or presentation wrappers.
`--ai-text` remains the markup-free embedding view; JSONL carries explicit
roles/spans.

Markdown/HTML conveys an image's position, alt text, and caption. Pixels reach
a multimodal model through ordered typed parts such as
`Text | Image { chunk_id, media_type, bytes }`, translated by an API adapter
into the provider's image-input blocks. Text-only models receive the fallback
description, not binary bytes or base64 prose.

Embeddings are intentionally external to `.tes`, keyed by stable chunk id and
content hash. Phrase search likewise uses projected text or an external
SQLite/Tantivy-style sidecar. The optional `vault.tes` catalog accelerates
title/tag/id navigation and the native link graph.

---

## History, drafts, and review

M10 first slice ships `THST` v1 with:

- logical full revisions;
- a physical content-addressed payload store keyed by chunk/catalog hash;
- revision manifests mapping stable chunk ids to payload hashes;
- named drafts and parent hashes;
- structural `tes diff` / `tes changelog`.

M10 also materializes any revision as a self-contained `.tes` (`export-revs`)
or replaces the live sealed body while preserving the current footer
(`checkout`). Git interoperability uses Tessera Markdown `textconv`
(`tes textconv`), `tes blame`, pending-ops redline (`tes pending`), and a
verified git merge driver (`tes merge-file`).

**Limitation:** revision manifests store catalog + chunk payloads only — not
`TLNK` rows — so materialization does not rewrite the link table yet.

Exact payload deduplication is required; near-duplicate deltas are later.
Real-time CRDT collaboration is not part of v1.

---

## Forward compatibility and conformance

Readers skip unknown optional features with a warning and fail on unknown
must-understand features. Catalog `features.optional` / `features.required`
distinguish the two ([layout_v0.md](layout_v0.md#catalog-features-forward-compatibility));
silently dropping required structure is forbidden.

Known optional feature ids in this build (all on `layout_version = 0`):

| Id | Meaning |
| --- | --- |
| `text_spans` | Layout-v1 text header fields (spans, table, math, lang, align, code_lang, caption, list_depth, indent) |
| `attachments` | Inert `ChunkType::Attachment` payloads |
| `external_uris` | TLNK v1 external URI heap |
| `citations` | Cite chunks and/or citation link edges |
| `slides` | Slide chunks |
| `figures` | Image and/or figure-ref chunks |
| `layout` | Layout chunks (`place` / `vspace` / `rule`; D24) |

Bump `layout_version` only when introducing a must-understand feature that
older readers must fail closed on.

The open-format deliverables are:

- a published Tessera MIME type and `.tes` mapping;
- a `file(1)` magic entry for `TESS`;
- must-accept/must-reject conformance fixtures and expected exports;
- an explicit license for the specification text.

The reference implementation also maintains claim-backed benchmarks for
partial chunk reads, backlinks, import, and export against equivalent
Markdown vault operations.

---

## Locked implementation order

Post-checkout/textconv (THI-194), layout-v1 text wire (THI-195), attachments
(THI-196), and typed TLNK targets / external URI heap (THI-197):

1. Optional: bump `layout_version` when must-understand feature flags land.

MIME/magic/conformance work may proceed in parallel. A future GUI, **in-wire**
full-text indexes, in-file embeddings, freeform slide geometry, and CRDTs are
not next. Vault FTS already ships as an **external** Tantivy sidecar.
