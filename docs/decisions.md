# Design decisions

**Status:** v0 defaults plus accepted layout v1 direction. A v1 decision
explicitly supersedes a conflicting v0 decision; shipped v0 readers remain
unchanged until migration code lands.

Related: [layout_v0.md](layout_v0.md),
[structure_v1.md](structure_v1.md), [security](security.md),
[format-comparison.md](format-comparison.md), [README](../README.md).

---

## Vault layout

**Decision:** A vault is a **folder of `.tes` files** plus an optional **`vault.tes`** sidecar with `doc_kind = index`.

| Choice | Rationale |
| --- | --- |
| One file per document | Matches note/wiki mental model; mmap one note without loading corpus |
| Optional `vault.tes` | `doc_id → title, tags, modified` for search/graph without opening every file |
| No multi-doc archive v0 | Simpler writer; object-store bundling deferred |

**Rejected for v0:** single tarball archive containing many docs (may revisit for sync/backup).

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

**v0 decision:** one text chunk with `"role": "table"` and a UTF-8 TSV body.

**v1 decision (supersedes D4):** a structured table payload contains rows and
cells with text, spans, alignment, header status, and row/column spans. One
table remains one reading-order unit. HTML/Markdown tables compile into that
structure.

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

| Shipped / near-term | Later |
| --- | --- |
| Source-hash + advisory write lock; optional `THST` suffix | M10 content-addressed revisions, drafts, diff, and review |

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
| `h1`–`h6`, `p`, `ul/ol/li`, `blockquote`, `pre/code`, `table`, `a` display text | `script`, `style`, inline `style=""` |
| Internal document edges → `TLNK` | — |

**Export:** HTML is generated from chunks + theme CSS — not round-tripped from imported HTML source.

Typed internal, external-URI (`http`/`https`/`mailto`), and attachment link
targets ship in `TLNK` (v0 all-internal; v1 + URI heap when needed). Markdown
import persists allowed external `href`s; HTML import still flattens anchors to
display text until wired the same way.

See [format-comparison.md — HTML](format-comparison.md#html--the-closest-cousin-and-the-main-antagonist).

---

## Markdown import / export

**Decision:** **CommonMark subset** for v0 import:

| Supported | Deferred |
| --- | --- |
| ATX headings, paragraphs, lists, fenced code, blockquotes | Footnotes and raw HTML blocks |
| Link display text; `[text](https://…)` / UUID destinations → `TLNK` + `InlineKind::Link` | Footnote link kinds; vault wikilink resolver |

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

## Page tensors (vision models)

**Decision:** **Out of v0**. Text + images + slides first; page-as-tensor (`[H,W,C]`) is Phase 9+ research.

---

## Naming: Tessera vs Aleph

**Decision:** **Tessera** = format (`.tes`, spec, `tes` CLI). **Aleph** reserved for a future GUI/workspace product on top. No Aleph references in v0 wire format.

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
   pagination, and computed numbering are renderer concerns.
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
| D20 | Content-addressed drafts/review in `THST` | M10 direction |
