# Format comparison — HTML, PDF, DOCX, Markdown, and Tessera

**Status:** design reference. Tessera is not implemented yet; this doc explains *why* the format exists and *where* it competes, with **HTML as the primary point of comparison**.

The README states the problem in one line: we overload formats built for **printing** (PDF, DOCX) or **plain-text authoring** (Markdown) when we need **structured, linkable, partially readable documents** that humans edit, machines index, and models consume without parsing markup.

HTML sits in the middle — and that is why it is the main antagonist. Tessera borrows HTML’s best idea (structure + theme) while rejecting HTML as the canonical on-disk representation.

---

## At a glance

| | **HTML** | **PDF** | **DOCX** | **Markdown** | **Tessera (`.tes`)** |
| --- | --- | --- | --- | --- | --- |
| **Primary job** | Render pages in a browser | Fixed visual page | Editable office document | Human-readable source text | Structured document **artifact** + exports |
| **Canonical unit** | One file or site (DOM tree) | Page(s) with drawing ops | OOXML ZIP (XML + media) | One `.md` file per note | One binary file, **chunk index** |
| **Structure** | Tags in source (`<h1>`, `<p>`) | Often implicit after layout | Styles + paragraphs in XML | `#` headings, `**bold**` in source | **Fields on chunks** (heading, emphasis, cite) |
| **Presentation** | CSS (often inline or linked) | Baked into page stream | Styles/themes in package | Themes at render time only | **CSS themes at export**; not in canonical layer |
| **Partial read** | Parse whole file or stream; DOM build | Page-level at best; text order fragile | Unzip + traverse XML | Read whole file or scan lines | **mmap + chunk offset** from index |
| **Links** | `<a href>` URLs | Annotations (optional) | Relationships in XML | `[[wikilinks]]` by title string | **`target_doc` + `target_chunk`** in index |
| **Citations** | Ad hoc footnotes | Hard to extract reliably | Fields, endnotes | `\cite{}`, pandoc extras | **cite chunks** → byte range + doc id |
| **AI / RAG path** | Strip tags; lose tables; boilerplate | OCR / heuristics; hyphenation noise | XML soup or lossy export | Markdown syntax in embedding | **`export_ai_text()`** — UTF-8 chunks, no escapes |
| **Git / diff** | Noisy (formatting churn) | Binary | Binary ZIP | Excellent | Export to Markdown/text for diff |
| **Print fidelity** | Browser-dependent | Excellent | Good | Weak unless pipeline | **Themed paginated export** (PDF target) |
| **Edit model** | Source or WYSIWYG in browser | Not really editable | Word GUI | Text editor | GUI + optional light markup → chunks |

---

## HTML — the closest cousin, and the main antagonist

If Tessera were described in one sentence to a web developer: **“Like HTML’s separation of structure and CSS, but the file is a chunked binary with stable IDs — and HTML is an export, not the source of truth.”**

### What HTML gets right

HTML is the format Tessera most resembles in *architecture*:

1. **Structure vs presentation** — semantic elements (`article`, `section`, `figure`) vs CSS. Tessera’s README explicitly adopts this split: blocks + semantic hints in `.tes`, **theme as injectable CSS** at export time.
2. **Multiple render targets** — same DOM → screen, print stylesheet, accessibility tree. Tessera: same chunks → HTML preview, print PDF, slide deck, AI text.
3. **Ecosystem** — every language has an HTML parser; browsers are universal viewers. Tessera does not fight this: **HTML is a first-class export**, not a competitor to eliminate.
4. **Links as first-class** — hypertext is native. Tessera wants that at the **corpus** level (cross-file, cross-chunk), not only within one page.

### Where HTML breaks down as the canonical format

These are the gaps Tessera is designed to attack **within scope**:

| HTML pain | Why it hurts | Tessera response (in scope) |
| --- | --- | --- |
| **Markup in the canonical layer** | Source *is* tags; every read parses angle brackets; AI pipelines strip HTML or embed noisy tokens | **Text chunks** hold reading-order UTF-8; bold/heading are **fields**, not `<strong>` in storage |
| **No stable addressability** | Fragments (`#id`) are optional, duplicated, or regenerated on export; no standard cross-file chunk pointer | **Chunk index**: `doc_id`, `chunk_id`, byte range — links and cites are index records |
| **Whole-file or DOM parse** | Large notes = large parse; partial edit still loads surrounding context | **mmap + catalog**: jump to chunk N without lexing the file |
| **Vault = folder of `.html`** | Wikis, backlinks, rename propagation — all ad hoc (static site generator, CMS, or custom) | **Link table on save**; hub docs; optional vault catalog |
| **Ambiguous “source”** | Is canonical the `.html`, the JSX, the CMS DB, the Markdown that generated it? | **One sealed `.tes` per document**; imports parse once |
| **Git-unfriendly** | Prettier, attribute order, wrapper divs → diff noise | Binary canonical; **`tes export --raw`** / Markdown for version control when needed |
| **Citation semantics** | `<cite>`, footnotes, sidenotes — no standard machine-readable quote→source graph | **cite chunks** with resolved `doc + chunk + range` |
| **Slides** | Reveal.js, bespoke HTML decks — not the same object model as long-form | **slide chunks** in the same container as text, images, cites |

### What Tessera deliberately does *not* try to beat HTML at

- **Universal zero-install viewing** — browsers win; Tessera needs an app or export step.
- **Live web publishing** — hosting, SSR, hydration, CSP, JS bundles are out of scope for the **format** (a future Aleph-style workspace might export *to* HTML sites).
- **Full DOM/CSS feature parity** — v1 targets flow layout, print themes, slide regions — not every CSS edge case or JavaScript-driven layout.

**Framing:** HTML is the **rendering lingua franca**. Tessera is the **authoring and storage lingua franca** for a personal or team corpus, with HTML as one of several projections — like saying LLVM IR isn’t “against” assembly, but you don’t hand-edit assembly as your source tree.

---

## PDF — the print antagonist (secondary)

PDF excels at **“this is exactly what it looks like on paper.”** That is also its curse for everything Tessera cares about.

| PDF strength | Tessera stance |
| --- | --- |
| Pixel-stable print | **Export target** via structure + print theme — not editable canonical form |
| Universal “send this” | `tes export --pdf` when needed; import PDF → text chunks + optional page rasters |
| Archival | Store imported PDFs as chunks or sidecar; don’t pretend vectors are prose |

| PDF weakness | Tessera attack (in scope) |
| --- | --- |
| Text extraction order ≠ reading order | Canonical **text chunks** written at import or authoring time |
| Tables, footnotes, columns → heuristic mess | Structure captured once at import; re-export from chunks |
| No real partial edit | Don’t edit PDF; edit `.tes`, re-export |
| Links/citations opaque to tools | **cite** and **link** records in index |
| Bad RAG input | **`export_ai_text()`**, chunk JSONL — never “raw PDF bytes to the model” |

**Out of scope (v1):** lossless PDF round-trip, perfect layout reconstruction from arbitrary PDFs, replacing LaTeX for math-heavy publishing.

---

## DOCX — the office antagonist

DOCX is what people already have — and what Tessera must **import from**, not emulate byte-for-byte.

| DOCX reality | Tessera stance |
| --- | --- |
| OOXML ZIP with styles, revisions, embedded objects | **`tes import`** → text chunks + images; styles → theme hints, not canonical |
| Track changes, comments, collaboration | Later revision log; not v1 Word clone |
| “Everyone uses Word” | Interchange **in**, not **through** |

| DOCX weakness | Tessera attack (in scope) |
| --- | --- |
| Mixed content + presentation in one package | Same split as HTML: structure in chunks, look in CSS theme |
| Heavy parse for “just the words” | Parse **once** on import; thereafter index + mmap |
| Weak cross-document graph | Native link/cite index across vault |
| AI tools choke on XML or lossy paste | Clean chunk exports |

**Out of scope (v1):** feature parity with Word (styles, mail merge, macros), flawless round-trip to DOCX.

---

## Markdown — the wiki antagonist

Markdown is Tessera’s most loved competitor for **notes and git-backed wikis**. The README already contrasts Obsidian-style vaults with a Tessera vault.

| Markdown strength | Tessera stance |
| --- | --- |
| Plain text, any editor | **Raw preview** = slice of text chunk; `export --raw` → `.txt` |
| Git diff and merge | Export Markdown for diff; canonical stays binary |
| Huge tooling ecosystem | Markdown remains an **export**; pandoc compatibility is a goal, not storage |

| Markdown weakness | Tessera attack (in scope) |
| --- | --- |
| `[[links]]` by title — rename breaks graph | Stable **doc/chunk IDs**; backlinks in index |
| Structure = syntax (`#`, `**`) in source | Structure = **chunk metadata**; no re-parse on read |
| One file per note, scan for backlinks | **Link table** updated on save |
| Citations via plugins / pandoc conventions | **cite chunks** native to format |
| No single fast binary artifact | One `.tes` with catalog; optional vault catalog |
| Tables, slides, print — bolted on | Same container: text, slide, image, cite chunks |

**Out of scope (v1):** replacing Markdown for every static site or README on GitHub; forcing binary for one-line notes (short-form `.tes` should still *feel* like a text file in the editor).

---

## Layer model — how Tessera maps onto the HTML mental model

Tessera makes the HTML split explicit and moves rendering **out** of the canonical file:

```text
┌─────────────────────────────────────────────────────────────┐
│  .tes (canonical)                                           │
│  ├── catalog + chunk index                                  │
│  ├── text / slide / image / link / cite chunks              │
│  └── metadata (doc_kind, tags, template id)                 │
└─────────────────────────────────────────────────────────────┘
          │ export + theme (CSS)
          ▼
┌──────────┬──────────┬──────────┬──────────┬──────────────┐
│   HTML   │   PDF    │ Markdown │  slides  │  AI views    │
│ preview  │  print   │  git     │  deck    │ linear_text  │
│          │          │          │          │ chunks_jsonl │
└──────────┴──────────┴──────────┴──────────┴──────────────┘
```

HTML appears **only** in the bottom row. The antagonist is not “HTML bad” — it is **“HTML (or Markdown, or OOXML) as the thing you mmap, link, and cite against.”**

---

## Where to attack — scoped priorities

Suggested order aligned with the README roadmap and the comparisons above:

| Priority | Target | Rationale |
| --- | --- | --- |
| **1** | **HTML-like structure, not HTML syntax** | Chunk types + semantic fields; themes at export — direct answer to “why not just HTML files?” |
| **2** | **Partial I/O + vault graph** | Beats HTML/MD folder models for large corpora |
| **3** | **AI exports** | Beats PDF/DOCX ingest and MD markup noise |
| **4** | **Import HTML/MD/DOCX/PDF once** | Lower switching cost; parse into chunks, never look back |
| **5** | **HTML + print PDF export** | Reuse CSS ecosystem for preview and “send a PDF” |
| **6** | **Research cites** | None of the four formats do this natively at corpus scale |

### Explicit non-goals (don’t fight these formats where they win)

- HTML in the browser for anonymous read-only sharing
- PDF as archival pixel-perfect snapshot of a third-party doc (import + raster is enough)
- DOCX as collaboration transport with legal track-changes workflow
- Markdown as the default for open-source READMEs and static blogs

---

## One-line positioning

| Format | One line |
| --- | --- |
| **HTML** | Great **output**; awkward **canonical store** for a linked, chunked, AI-ready corpus. |
| **PDF** | Great **print snapshot**; terrible **editable structure**. |
| **DOCX** | Great **office interchange**; terrible **machine-native document model**. |
| **Markdown** | Great **authoring syntax**; fragile **graph and binary performance** at vault scale. |
| **Tessera** | **Canonical chunked binary** — structure in the file, presentation in themes, **HTML/Markdown/PDF as exports**. |

---

## Open questions (format strategy)

- **HTML import:** preserve class names as theme hints vs strip to semantic blocks only?
- **Single-file `.html` vs site:** import one page per `.tes` or multi-page via chunk sections?
- **Markdown compatibility:** which CommonMark/GFM features compile into v1 chunk types?
- **PDF import:** text-only first vs page rasters + text for research mode?
- **When to export HTML vs Markdown** for human readers vs git — document per `doc_kind` defaults?

See also the main [README](../README.md) for writing modes, vault layout, and implementation order.

**Documentation index**

| Doc | Role |
| --- | --- |
| [layout_v0.md](layout_v0.md) | Wire format (bytes on disk) |
| [exports.md](exports.md) | Decoded view contracts |
| [cli.md](cli.md) | `tes` command surface |
| [decisions.md](decisions.md) | v0 design choices |
| [roadmap.md](roadmap.md) | Phases and suggested issues |
| [engine.md](engine.md) | Rust crate architecture and module map |
| [glossary.md](glossary.md) | Term definitions |
