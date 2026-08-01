# Print IR (`ariadnes-weave`)

**Status (Tessera 0.2.0):** prose print-tree builder + CLI `--backend native`
shipped (THI-288 / THI-290 / THI-294). Spec + D21 accepted. Layout quality
beyond prose (THI-291+ tables/math/decks/fonts) continues in
**`ariadnes-weave`** under epic THI-256.

Tessera builds the IR from `.tes` (`render::print`) and calls the crate
(`ariadnes_weave::emit_pdf`). Cargo feature `native-pdf` (default) gates the
dependency; optional `weave-cjk` / `weave-emoji` / `weave-icons` pass through.

Related: [decisions — Print IR](decisions.md#print-ir-and-pdf-source-of-truth),
[exports — PDF](exports.md#--pdf), [roadmap](roadmap.md).

---

## Why

HTML + CSS + Chromium print is a fine **preview** path, but pagination is not
Tessera-owned. Markdown→PDF toolchains are worse: same prose, different page
breaks every converter. Native PDF should be a **replayable literary
unfolding**: same `.tes` + same **print profile** version → same pagination.

---

## Layers (updated)

| Layer | Owner | Role |
| --- | --- | --- |
| Structure | `.tes` wire | Canonical chunks, spans, media, catalog |
| Print IR | Tessera → `ariadnes-weave` | Pagination-ready tree (this doc) |
| Print profile | `ariadnes-weave` (+ Tessera ids) | Page size, margins, fonts, break policy (`print`, `manuscript`, later `deck`) |
| PDF bytes | `ariadnes-weave` | Laid-out pages |
| HTML + CSS theme | Tessera template packs | Browser preview / interchange (`tes serve`, `--html`) |

**PDF source of truth for layout** = print IR + profile, not CSS.
**HTML** remains the browser sink; it does not drive native PDF.

Chromium HTML-print remains the **CLI default** (`--backend chromium`); native
is opt-in until promoted. After cutover, Chromium stays as optional fallback.

---

## Non-goals

* CSS layout engine / WeasyPrint clone
* Typst (or LaTeX) as canonical authoring
* Editable / round-trippable PDF
* Pixel parity with Chromium deck CSS Grid
* Storing pagination in the `.tes` wire

---

## Pipeline

```text
.tes  --(tessera-doc)-->  PrintDocument + PrintProfileId
                              |
                              v
                      ariadnes-weave
                              |
                              v
                           PDF bytes
```

`tes serve` / `--html` unchanged:

```text
.tes  -->  semantic HTML + theme CSS  -->  browser
```

---

## Type sketch (v0)

Names are illustrative; the crate may rename modules. serde names stay stable
once published.

```rust
/// Top-level input to ariadnes-weave.
pub struct PrintDocument {
    pub meta: PrintMeta,
    pub profile: PrintProfileId,
    pub blocks: Vec<PrintBlock>,
}

pub struct PrintMeta {
    pub title: String,
    pub doc_kind: String,       // mirror catalog / superblock
    pub language: Option<String>, // BCP-47
    pub source_doc_id: Option<String>,
}

/// Stable id for a versioned profile (not a CSS file).
/// Examples: "print@1", "manuscript@1", "deck@1".
pub struct PrintProfileId {
    pub name: String,
    pub version: u32,
}

pub enum PrintBlock {
    Heading { level: u8, runs: Vec<TextRun>, break_before: BreakHint },
    Paragraph { runs: Vec<TextRun> },
    List { ordered: bool, items: Vec<ListItem> },
    Code { lang: Option<String>, text: String },
    Quote { runs: Vec<TextRun> },
    Table { rows: Vec<TableRow> },
    Figure {
        image: PrintImage,
        alt: String,
        caption: Vec<TextRun>,
        placement: FigurePlacement,
    },
    Math { display: bool, latex: String },
    Slide {
        layout_id: String,
        regions: Vec<SlideRegionContent>,
    },
    /// Explicit author/export break (e.g. chapter boundary).
    Break(BreakHint),
}

pub struct TextRun {
    pub text: String,
    pub style: InlineStyle, // strong/em/code/link/cite flags; no free CSS
}

pub enum BreakHint {
    None,
    /// Prefer new page (soft).
    Page,
    /// Always new page (e.g. manuscript H1).
    PageAlways,
    /// Keep with following block (heading + first lines).
    KeepWithNext,
}

pub enum FigurePlacement {
    Flow,
    FloatNear,
    // extend carefully; no freeform x/y
}

pub struct PrintImage {
    pub bytes: Vec<u8>,
    pub media_type: String, // image/png, image/jpeg, …
    pub width_px: Option<u32>,
    pub height_px: Option<u32>,
}
```

### Profiles (policy, not CSS)

| Profile | Intent | Break policy (v0) |
| --- | --- | --- |
| `print` | Academic / long-form | Soft page breaks; headings `KeepWithNext` |
| `manuscript` | Beta-reader | H1 → `PageAlways`; Courier-like mono; double-spaced metrics |
| `deck` | Slides (later) | One slide block → one page; region slots, not CSS Grid |

Profile **version** bumps when pagination rules change so fixtures can pin
`manuscript@1` vs `manuscript@2`.

---

## Determinism

**Guarantee (target):** for fixed `PrintDocument` bytes (or canonical
serialization) + fixed `PrintProfileId` + fixed crate version + pinned fonts,
PDF output is **byte-identical** (preferred) or **structurally identical**
(same page count, same text per page) if PDF object order must vary.

**Literary unfolding:** manuscript chapter/scene breaks land the same way every
export — the anti-Markdown-PDF property.

Font files used by a profile are **crate-bundled or explicitly hashed** inputs,
not “whatever is on the system,” for CI stability.

---

## Tessera mapping (prose MVP)

| `.tes` | Print IR |
| --- | --- |
| Text `heading` level N | `Heading { level: N, … }`; level 1 + `manuscript` → `PageAlways` |
| `paragraph` / quote / code / list | Matching blocks; inline spans → `TextRun` styles |
| `--chapter N` | Emit only that H1 slice (same rules as export) |
| Structured table | `Table` (IR mapped; layout quality THI-291) |
| Figure + image | `Figure` (IR mapped; float/placement THI-291) |
| Math role / inline math | `Math { latex }` (IR mapped; real math layout THI-291) |
| Slide chunk | `Slide` (IR mapped; richer regions THI-293) |
| THST / pending | Ignored for print body (sealed body only) |

---

## CLI

```bash
tes export doc.tes --pdf -o out.pdf --backend native   # ariadnes-weave
tes export doc.tes --pdf -o out.pdf --backend chromium # HTML print (default)
```

Default stays `chromium` until native is promoted; both backends ship in 0.2.0.

---

## Implementation order

1. This sketch + D21 (THI-288) — done
2. Scaffold `ariadnes-weave` (THI-289) — done
3. Tessera print-tree builder, prose (THI-290) — done (0.2.0)
4. Pagination + CLI wiring (THI-294) — done (0.2.0, `--backend native`)
5. Deterministic fixtures (THI-292) — done in weave; tables/figures/math (THI-291); decks (THI-293); fonts (THI-307/308)
