# Roadmap and phases

**Status:** M0–M9 plus M10 (`save`/`log`/`diff`/`changelog`/`export-revs`/
`checkout`/`textconv`), layout-v1 **text wire** (spans/math/tables/lang),
**inert attachments**, and **typed TLNK targets** (external URI heap v1) are
shipped as additive wire on `layout_version = 0`. Next code is history residual
(blame/merge/redline) and light `vault.tes` polish. This is an implementation
plan, not a release schedule.

Linear is the canonical tracker. Each phase lists acceptance criteria and doc
links.

## Capability snapshot

| Area | Shipped now | Frozen next | Later |
| --- | --- | --- | --- |
| Binary foundation | superblock, catalog, `TIDX`, `TLNK`, writer, mmap reader, verify | optional/required feature policy | conformance kit grows continuously |
| Prose | typed text roles, Markdown/HTML import/export, Tessprek edit, ranged spans, math, code/block lang, align | — | further editor adapters |
| Tables | structured header table + TSV fallback | — | rich import/export |
| Links | internal UUID/chunk graph + typed external/attachment targets (TLNK v0/v1) | — | light `vault.tes` catalog |
| Media | image payload + `FigureRef`; inert attachments | — | — |
| Human render | template packs, `tes serve`, PDF, slides | — | theme polish |
| AI | raw/linear/AI text/JSONL, multimodal parts, cites/BibTeX | Markdown or semantic HTML profiles (already distinct) | — |
| History | `THST` v1, drafts, structural diff/changelog, export-revs/checkout/textconv | — | blame, merge driver, redline |

Full decisions and non-goals: [structure_v1.md](structure_v1.md).

---

## Phase 0 — Spec freeze (done)

**Goal:** Decisions and wire format written down before Rust modules multiply.

| Deliverable | Doc |
| --- | --- |
| Layout v0 | [layout_v0.md](layout_v0.md) |
| Export contracts | [exports.md](exports.md) |
| CLI surface | [cli.md](cli.md) |
| Decisions | [decisions.md](decisions.md) |
| Format strategy | [format-comparison.md](format-comparison.md) |
| Engine architecture | [engine.md](engine.md) |
| CI skeleton | `.github/workflows/ci.yml` |

**Exit criteria:** Team agrees v0 scope; first issues filed with doc links.

---

## Phase 1 — Write path

**Goal:** Create valid minimal `.tes` files on disk.

| Issue theme | Acceptance |
| --- | --- |
| `layout` types | Rust structs match [superblock](layout_v0.md#superblock-v0-64-bytes) + index row |
| Session writer | Write `empty.tes`, `note_one_chunk.tes` |
| Golden fixtures | Byte-exact fixtures under `fixtures/v0/` |
| Unit tests | Round-trip superblock + index parse |

**Depends on:** Phase 0.

---

## Phase 2 — Read path

**Goal:** mmap open, catalog + index read, no export yet.

| Issue theme | Acceptance |
| --- | --- |
| Mmap reader | Open fixture files read-only |
| `tes info` | Default table + `--json` |
| `tes verify` | Fails on truncated/corrupt fixtures; exit 1 |

**Depends on:** Phase 1.

---

## Phase 3 — Core exports

**Goal:** AI-friendly views without import pipeline.

| Issue theme | Acceptance |
| --- | --- |
| `--raw`, `--linear` | Golden diff tests |
| `--ai-text` | No HTML/MD sigils in output |
| `--chunks-jsonl` | Line count = reading-order chunks |
| Library API | `tessera_doc::io::export::*` documented in rustdoc |

**Depends on:** Phase 2. Spec: [exports.md](exports.md).

---

## Phase 4 — Markdown import

**Goal:** `tes import --markdown` → `.tes`.

| Issue theme | Acceptance |
| --- | --- |
| CommonMark subset | Headings, lists, paragraphs, code, quotes |
| Link table | External `href` optional v0 |
| Import + export | Structure preserved through MD → tes → MD |

**Depends on:** Phase 1 writer, Phase 3 export. Decisions: [Markdown](decisions.md#markdown-import--export).

---

## Phase 5 — Vault graph

**Goal:** Cross-document links and hub docs.

| Issue theme | Acceptance |
| --- | --- |
| Link table write path | GUI-less: CLI or test helper |
| `tes link resolve/backlinks` | Works on fixture vault dir |
| Hub doc fixture | [hub_links.tes](layout_v0.md#golden-fixtures-planned) |
| Optional `vault.tes` index | Search by title without opening all files |

**Depends on:** Phase 4. Decisions: [vault](decisions.md#vault-layout), [hub](decisions.md#hub-documents).

---

## Phase 6 — HTML import + export

**Goal:** Answer the “why not HTML files?” comparison with working paths.

| Issue theme | Acceptance |
| --- | --- |
| `tes import --html` | Semantic blocks per [decisions](decisions.md#html-import) |
| `tes export --html` | Theme injection; valid HTML5 fragment |
| Preview story | Document `--standalone` + `--theme` |

**Depends on:** Phase 3, Phase 5 (internal links). See [format-comparison.md](format-comparison.md).

---

## Structure-freeze checkpoint

**Goal:** write down the layout v1 semantic contract before later modes build
incompatible one-off models.

| Issue theme | Acceptance |
| --- | --- |
| Inline structure | Enum-backed ranged spans; math uses LaTeX source only |
| Rich blocks | Structured tables; code/document language fields |
| Graph/media | Typed link targets; image payload vs figure use; inert attachments |
| Templates | Pack manifest for theme, cite style, export defaults, starter Tessera Markdown |
| Safety/evolution | [Security](security.md), optional-vs-required feature policy |
| Tracker | Tessera Linear summary current; M7–M10 populated |

This is a short specification pass, not a rewrite of M0–M6 and not a code
milestone. See [structure_v1.md](structure_v1.md).

---

## Phase 7 — Preview, themes, and print/PDF

**Goal:** “Send a PDF” from structure + print theme.

| Issue theme | Acceptance |
| --- | --- |
| Template/theme pack | Manifest, ids/hashes, draft + print CSS |
| `tes serve` | Loopback live preview; file watch; CSS-only by default |
| Images | Reusable image payload + `FigureRef`; alt/caption/placement |
| Print theme CSS | `@page`, margins, page breaks, accessible HTML input |
| `tes export --pdf` | Print-ready output from long-form fixture |
| One render path | Browser preview and PDF use the same semantic HTML + theme |

**Depends on:** Phase 6 and the structure-freeze checkpoint. Does not depend
on citations or slides. Non-goals: PDF as editable source; pixel coordinates
in `.tes`.

---

## Phase 8 — Research mode

**Goal:** Citations across corpus.

| Issue theme | Acceptance |
| --- | --- |
| Cite chunks + link kind 2 | Writer + ranged cite spans + graph mirror |
| Cite styles | Template-selected APA/MLA/Chicago/numeric-style projection |
| `tes import --pdf` | Text + optional page rasters |
| AI cite expansion | Markdown/HTML/plain projections resolve cite data |
| Bibliography interchange | BibTeX and CSL JSON import/export |

**Depends on:** Phase 5 and the structure freeze. Phase 7 is optional except
for producing an academic PDF.

---

## Phase 9 — Presentations

**Goal:** Slide chunks in same container.

| Issue theme | Acceptance |
| --- | --- |
| Slide payload v1 wire | `layout_id` + named region refs |
| Deck export | HTML or PDF slide mode |
| Reuse prose/images | Import chunks from research doc into deck |

**Depends on:** Phase 6–7. Freeform coordinates are not canonical. Decision:
[slides](decisions.md#slide-model).

---

## Phase 10 — History + review

**Goal:** full logical drafts with compact shared storage, readable diffs, and
asynchronous track changes.

| Issue theme | Acceptance |
| --- | --- |
| `THST` v1 | Content-addressed payload store + revision manifests |
| Drafts | Save/checkout/export named full revisions |
| Diff | Tessera Markdown textconv; structural `tes diff` / changelog |
| Review | Authored pending ops; redline; accept/reject; comments |
| Git interop | `.gitattributes` textconv; optional verified merge driver |

**Depends on:** chunk hashes and stable ids specified during the structure
freeze. CRDT/live cursors are not part of M10.

---

## Phase 11 — Fiction + manuscript exports

**Goal:** Chapter-scoped exports, beta-reader PDF.

| Issue theme | Acceptance |
| --- | --- |
| `doc_kind = manuscript` conventions | Scene/chapter chunking |
| Chapter-scoped export flags | `--chapter N` |
| Manuscript PDF theme | Separate from academic print |

**Depends on:** Phase 7.

---

## Future (unscheduled)

| Theme | Notes |
| --- | --- |
| DOCX import | Office interchange, not canonical |
| `tes repair` | Tetration parity |
| CRDT / live collaboration | Post-M10; async review first |
| Page tensors | Vision `[H,W,C]` export |
| Aleph GUI | Workspace on top of `.tes`; separate product |
| Native full-text index | Projected text / external sidecar first |
| Shared wire crate with Tetration | Revisit after v0 stable |

---

## Next Linear issue batch

1. **history residual:** blame, verified merge driver, pending-ops redline.
2. **open format:** MIME/magic entries and v1 conformance cases.
3. **benchmark:** measure the specific mmap/link/export claims in the README.
4. **vault polish:** light `vault.tes` catalog (optional).

---

## Relation to Tetration

| Tetration phase analog | Tessera phase |
| --- | --- |
| Layout v1 + writer | Phase 1–2 |
| `tet verify` | Phase 2 |
| `tet convert` | Phase 4+ import |
| Query / export views | Phase 3 exports |
| FFI / Python | Not planned v0 |

See [README — Relation to Tetration](../README.md#relation-to-tetration).
