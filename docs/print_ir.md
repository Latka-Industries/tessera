# Print IR (`ariadnes-weave`)

**Status (Tessera 0.2.7):** prose print-tree builder + CLI `--backend native`
shipped (THI-288 / THI-290 / THI-294). Spec + D21 accepted. Requires
**`ariadnes-weave` ≥ 0.2.7** (quote italic + aesthetic knobs; `TextRun.face`
pins since 0.2.2; category default fonts via `[text|heading|quote|cite].font`
in pack `weave.toml` — THI-360). Pack `fonts.toml` → `EmitOptions::pinned_faces` and
`weave.toml` → layout knobs (D23). Layout quality beyond prose (THI-291+
tables/math/decks; OS fonts THI-311) continues in
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
        title: Vec<TextRun>,   // above image; empty = none (weave figure title band)
        caption: Vec<TextRun>, // weave `[caption]` knobs own italic/size/band
        placement: FigurePlacement,
    },
    Math { display: bool, latex: String },
    Slide {
        layout_id: String,
        regions: Vec<SlideRegionContent>,
    },
    /// Sealed layout chunk (D24): closed `place` / `vspace` / `rule` ops.
    Layout {
        ops: Vec<LayoutOp>,
    },
    /// Explicit author/export break (e.g. chapter boundary).
    Break(BreakHint),
}

/// One closed layout op inside `PrintBlock::Layout` (D24).
pub enum LayoutOp {
    Place { skip: PlaceSkip, runs: Vec<TextRun> },
    Vspace { amount: VspaceAmount },
    Rule { width: RuleWidth },
}

pub struct TextRun {
    pub text: String,
    pub style: InlineStyle, // strong/em/code/link/cite flags; no free CSS
    /// Optional pin id for `EmitOptions::pinned_faces` (host TTF).
    /// Tessera prose runs leave this `None` (Liberation style mapping).
    pub face: Option<String>,
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

### Host-pinned faces (`ariadnes-weave` ≥ 0.2.2)

`emit_pdf` is `emit_pdf_with(..., EmitOptions::bundled_only())`: sealed
Liberation (plus optional `icons` / future `cjk` / `emoji` packs). Embedders
can pin host TTFs and select them per run:

```rust
let opts = EmitOptions::bundled_only().with_pinned_face("ui", ttf_bytes);
// TextRun::pinned("…", "ui")  or  TextRun { face: Some("ui".into()), … }
let pdf = emit_pdf_with(&doc, &opts)?;
```

Unknown pin ids fail emit (`WeaveError::Font`). Pins are stable inputs (sorted
ids + fixed bytes), so fixtures stay deterministic — this is not OS fontconfig
lookup (that remains later / THI-311).

**Tessera today:** `InlineKind::Font { font_id }` maps to weave `TextRun.face`
(D23 / THI-356). The CLI native path loads pack `fonts.toml` into
weave `EmitOptions::pinned_faces` and pack `weave.toml` into layout knobs when a
template pack is resolvable; otherwise `EmitOptions::bundled_only()`.
Optional category defaults (`[text|heading|quote|cite].font` pin ids) apply when
`TextRun.face` is unset; explicit `\font{id}{…}` / `TextRun.face` still wins
(THI-360).

---

## Tessera mapping (prose MVP)

| `.tes` | Print IR |
| --- | --- |
| Text `heading` level N | `Heading { level: N, … }`; level 1 + `manuscript` → `PageAlways` |
| `paragraph` / quote / code / list | Matching blocks; inline spans → `TextRun` styles |
| Text chunk `title` | `Paragraph` with `style.strong` (label stand-in; no non-figure title IR) |
| Text chunk `caption` | `Paragraph` with `style.emphasis` (stand-in; weave `[caption]` is figure-only) |
| Figure title | `Figure.title` runs (plain; weave title band / `title_text_align`) |
| Figure caption | `Figure.caption` plain runs; weave `[caption]` knobs (italic/size/band) |
| Inline `Quote` | `style.emphasis` |
| Inline `Math` | `style.code` (latex/source visible; true inline math IR still weave gap) |
| Inline `Underline` | `style.underline` (ariadnes-weave ≥ 0.2.8) |
| `--chapter N` | Emit only that H1 slice (same rules as export) |
| Structured table | `Table` (IR mapped; layout quality THI-291) |
| Figure + image | `Figure` (title + caption + placement; float THI-291) |
| Math role / inline math | Display math role → `Math { latex }`. Inline `Math` spans → `style.code` stopgap (THI-349); real inline math runs still open on weave |
| Slide chunk | `Slide` (IR mapped; richer regions THI-293) |
| Layout chunk (`place` / `vspace` / `rule`) | `Layout` (D24 / THI-363) — see [decisions.md — D24](decisions.md) |
| Cite / quote / ref chunks | Mapped (THI-348): `\quote` → `Quote`; biblio stub → numbered `Paragraph`; `\ref` → short `Paragraph`; trailing `References` when biblio stubs exist |
| Inline `Citation` spans | Rewritten to `[n]` / `[@key]` with `style.cite` (same numbering as HTML/Markdown) |
| Attachment chunks | Skipped (not prose) |
| THST / pending | Ignored for print body (sealed body only) |

---

## CLI

```bash
tes export doc.tes --pdf -o out.pdf --backend native   # ariadnes-weave
tes export doc.tes --pdf -o out.pdf --backend chromium # HTML print (default)
```

Default stays `chromium` until native is promoted; both backends ship since 0.2.0
(`ariadnes-weave` **0.2.7+** for category fonts + aesthetic knobs). Native CLI uses
bundled faces plus optional pack `fonts.toml` pins and `weave.toml` knob
overlays when `--template` / `--template-root` resolve a pack. See
[Host-pinned faces](#host-pinned-faces-ariadnes-weave--022) for the library pin
path.

---

## Implementation order

1. This sketch + D21 (THI-288) — done
2. Scaffold `ariadnes-weave` (THI-289) — done
3. Tessera print-tree builder, prose (THI-290) — done (0.2.0)
4. Pagination + CLI wiring (THI-294) — done (0.2.0, `--backend native`)
5. Deterministic fixtures (THI-292) — done in weave; tables/figures/math (THI-291); decks (THI-293); fonts (THI-307/308); host pins via `EmitOptions` (weave 0.2.2 / Tessera 0.2.1); pack `fonts.toml` + `\font` (Tessera 0.2.5 / THI-356); category fonts (Tessera 0.2.6 / THI-360); layout blocks (D24 / THI-362..363)
