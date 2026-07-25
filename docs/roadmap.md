# Roadmap and phases

**Status:** M0–M6 implemented; Phase 7 (print/PDF export) is next. This is an
implementation plan, not a release schedule.

Use this doc to create **GitHub milestones and issues**. Each phase lists acceptance criteria and doc links.

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
| Library API | `tessera::export::*` documented in rustdoc |

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

## Phase 7 — Print PDF export

**Goal:** “Send a PDF” from structure + print theme.

| Issue theme | Acceptance |
| --- | --- |
| Print theme CSS | `@page`, margins, page breaks |
| `tes export --pdf` | Print-ready output from long-form fixture |
| Draft vs print | Two theme ids documented |

**Depends on:** Phase 6 (HTML render path). Non-goal: PDF as editable source.

---

## Phase 8 — Research mode

**Goal:** Citations across corpus.

| Issue theme | Acceptance |
| --- | --- |
| Cite chunks + link kind 2 | Wire + writer |
| `tes import --pdf` | Text + optional page rasters |
| `--ai-text` cite expansion | Resolved quotes in prose |
| `--bibliography` | BibTeX or CSL JSON export |

**Depends on:** Phase 5, Phase 7 optional.

---

## Phase 9 — Presentations

**Goal:** Slide chunks in same container.

| Issue theme | Acceptance |
| --- | --- |
| Slide payload v1 wire | Extend layout doc |
| Deck export | HTML or PDF slide mode |
| Reuse prose/images | Import chunks from research doc into deck |

**Depends on:** Phase 6–7. Decision: [slides](decisions.md#slide-model).

---

## Phase 10 — Fiction + manuscript exports

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
| Revision log / sync | Post–v0 collab |
| Page tensors | Vision `[H,W,C]` export |
| Aleph GUI | Workspace on top of `.tes`; separate product |
| Shared wire crate with Tetration | Revisit after v0 stable |

---

## Suggested first GitHub issues (batch)

Copy into issues after Phase 0 review:

1. **layout:** Implement superblock + chunk index types (`layout_v0.md`)
2. **writer:** Session writer + `note_one_chunk.tes` fixture
3. **reader:** mmap catalog + index reader
4. **cli:** `tes verify` with golden corrupt fixtures
5. **cli:** `tes info` default + `--json`
6. **export:** `--raw` and `--ai-text` + golden tests
7. **export:** `--chunks-jsonl`
8. **import:** Markdown → `.tes` (CommonMark subset)
9. **vault:** Link table write + `hub_links.tes` fixture
10. **ci:** Wire `tes verify` on fixtures in GitHub Actions

**Milestone map:** M0 = issues 1–2, M1 = 3–5, M2 = 6–7, M3 = 8, M4 = 9–10.

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
