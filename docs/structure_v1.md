# Structure freeze for layout v1

**Status:** accepted design direction; implementation is milestone-tracked.
Layout v0 remains the only shipped wire format. This document freezes the
semantic model that must be specified before layout v1 code lands.

Related: [layout v0](layout_v0.md), [decisions](decisions.md),
[exports](exports.md), [roadmap](roadmap.md), [security](security.md).

---

## Boundary: structure, theme, render

Tessera has three conceptual layers:

1. **Structure** is canonical in `.tes`: typed blocks, inline spans, links,
   citations, media references, stable ids, and document metadata.
2. **Theme/template** is an external, versioned pack referenced by id and hash.
   It supplies CSS, export defaults, slide regions, cite style, and optional
   trusted presentation code.
3. **Render** is generated: Tessera Markdown, semantic HTML, a website, PDF,
   slides, or AI inputs.

Store author intent that must survive renderers. Do not store soft wrapping,
pagination, computed figure numbers, or pixel coordinates.

---

## Capability inventory

| Capability | v0 engine | v1 structure decision | Status |
| --- | --- | --- | --- |
| Text blocks | Implemented | Typed roles remain canonical | shipped |
| Inline formatting | Ranged `InlineSpan` + `InlineKind` | Ranged `InlineSpan` + `InlineKind` | shipped (additive header) |
| Math | `TextRole::Math` + inline math spans | LaTeX source for math only | shipped (additive header) |
| Tables | Structured `table` on text header (TSV fallback) | Structured rows and cells | shipped (additive header) |
| Code language | Optional `code_lang` on code blocks | Optional `lang` on code blocks | shipped (additive header) |
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
| History (first slice) | `save` / `log` / `diff` / `changelog` | Content-addressed revisions + drafts | shipped (M10) |
| History (checkout / textconv / merge) | Materialize revisions; git Tessprek + verified merge | `export-revs` / `checkout` / `textconv` / `merge-file` | shipped (M10) |
| History (redline) | Footer `pending` reserved | Authored ops + accept/reject | shipped (M10) |
| Vault graph | Implemented | Light `vault.tes` catalog (`tes vault`) | shipped / later |
| Full-text search | Scan only | External index or projected-text search | later |
| Embeddings | Missing | External to `.tes` | out of wire |

---

## Typed Rust model

Closed semantic vocabularies are Rust enums with stable serde names. JSON is a
wire encoding, not an untyped application API.

```rust
pub enum InlineKind {
    Emphasis,
    Strong,
    Code,
    Term,
    Quote,
    Math { tex: String },
    Link { link_id: u64 },
    Citation { cite_chunk_id: u64 },
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

Soft wrap, hyphenation, columns, widows/orphans, and pagination belong to the
renderer. Intentional hard breaks remain content.

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
- CSS and optional fonts/assets;
- allowed block types and `doc_kind` defaults;
- export targets and starter Tessera Markdown;
- named slide regions;
- citation style id or cite-style pack.

The catalog stores `template_id`, `theme_id`, `cite_style_id`, and optional
pack hashes. Pack bytes are external by default; standalone exports may embed
CSS/assets.

`tes serve` projects `.tes` to HTML and applies a draft or print theme.
`tes export --pdf` uses the same HTML + print-theme path with a headless print
engine. Browser preview and PDF are two sinks of one pipeline.

Slides store `layout_id` plus named region slots (`title`, `body`, `media`,
etc.) referencing text/image/cite chunks. CSS grid/flex gives the renderer
wide latitude. Freeform `x/y/w/h` coordinates are not part of layout v1.
Animation is theme-layer presentation, not chunk data.

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

Follow-on materializes any revision as a self-contained `.tes` (`export-revs`)
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
must-understand features. Layout/feature flags distinguish the two; silently
dropping required structure is forbidden.

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

MIME/magic/conformance work may proceed in parallel. Aleph GUI, native
full-text search, in-file embeddings, freeform slide geometry, and CRDTs are
not next.
