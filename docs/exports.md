# Export views — contracts

**Status:** specification for decoded **views** of a `.tes` file. Canonical storage is [layout_v0.md](layout_v0.md); exports are **projections**, not round-trip source.

Related: [layout_v0.md](layout_v0.md) (wire format), [engine.md](engine.md) (module map), [cli.md](cli.md), [decisions](decisions.md), [roadmap](roadmap.md).

---

## Principles

1. **Models read views, not wire format** — pipelines request Markdown,
   semantic HTML, plain AI text, JSONL, or typed multimodal parts.
2. **Markup is profile-specific** — `--ai-text` remains markup-free;
   Markdown is the compact default for general LLM prompts; AI HTML is a
   sanitized semantic fragment.
3. **HTML is an export** — generated from chunks + theme; see [format-comparison.md](format-comparison.md).
4. **Lossy is explicit** — Markdown/HTML/PDF exports document what they drop.

---

## View catalog

| View | CLI flag | Primary consumer |
| --- | --- | --- |
| Raw text | `--raw` | Editors, diff, “show me bytes” |
| Linear text | `--linear` | Reading order, TTS, simple ingest |
| AI text | `--ai-text` | LLM context windows |
| Chunks JSONL | `--chunks-jsonl` | RAG indexing |
| Markdown | `--markdown` | Git, Obsidian migration, pandoc |
| HTML | `--html` | Browser preview, static site |
| AI Markdown/HTML | `--ai --format markdown\|html` | LLM prompts |
| Metadata | `--meta json\|yaml\|toml` | Agents, static-site tools |
| Bibliography | `--bibliography` | Research mode |
| Attachment bytes | `--attachment` | Explicit download of inert attachment chunk |
| PDF | `--pdf` | Print / send (themed) |

v0 implementation target: **`--raw`**, **`--linear`**, **`--ai-text`**, **`--chunks-jsonl`**,
plus Markdown/HTML/PDF/bibliography/attachment as shipped.

---

## `--raw`

**One chunk or whole file UTF-8** without structural decoration.

| Flag | Behavior |
| --- | --- |
| _(default)_ | Concatenate all text chunk **bodies** in reading order, `\n\n` between chunks |
| `--chunk ID` | Single text chunk body only |
| `--chapter N` | Text bodies in the Nth H1-bounded chapter only (1-based) |
| `--include-headers` | Prefix each chunk with `[chunk_id=N role=heading level=2]\n` (debug) |

**Guarantees:**

- Output is valid UTF-8 (invalid surrogate pairs rejected at write time).
- No HTML, Markdown, or LaTeX syntax unless literally present in authored body.

---

## `--linear`

Reading-order prose with **light structure markers** for human scan (not for models).

**Format:** plain text with optional prefix lines:

```text
# Methods

We measured temperature at 15 stations.

## Results

The mean was 12.4°C.
```

| Source | Rule |
| --- | --- |
| `role: heading` | Prefix `#` repeated `level` times + space + body |
| `role: paragraph` | Body only |
| `role: list_item` | `- ` or `1. ` from `list_kind` |
| `role: table` | v0 TSV; v1 structured cells in reading order |
| `attachment` | `[attachment filename=… media_type=… sha256=…]` (+ optional caption line); never embeds bytes |
| Links | Inline `[display](doc:UUID/chunk)` form |

**Guarantees:**

- Deterministic given same `.tes` bytes.
- De-hyphenated soft breaks: **not applied in v0** (import normalizes).

---

## `--ai-text`

Optimized for **LLM context** — no markup, minimal noise.

**Rules:**

| Include | Exclude |
| --- | --- |
| Text chunk bodies in reading order | Heading `#` markers (structure via separate JSONL fields if needed) |
| Resolved cite quotes as plain sentences | `\cite{}`, footnote markers |
| Table cell text row-by-row | HTML/MD syntax |
| Single `\n\n` between chunks | Chunk ids in prose (unless `--annotate`) |

**Cite handling:**

```text
Smith (2024) reported that we measured temperature at 15 stations.
```

Resolved from cite chunks + link table; unresolved cites → `[citation unresolved: label]`.

| Flag | Behavior |
| --- | --- |
| `--annotate` | Prefix each chunk with `<!-- chunk:12 -->` (machine-oriented) |
| `--no-cites` | Omit cite chunk expansion |

**Guarantee:** Output MUST NOT contain HTML tags or Markdown sigils introduced by the exporter.

---

## `--ai --format markdown|html` (layout v1 profile)

General LLM prompting defaults to Markdown because headings, lists, emphasis,
links, citations, math, and image positions are compact and familiar to
models. Semantic HTML is available when explicit element boundaries matter.

| Format | Contract |
| --- | --- |
| `markdown` | Reconstruct structure and ranged spans; no presentation wrappers |
| `html` | Semantic fragment only; no CSS, scripts, navigation, or theme wrappers |

Both are projections of the same typed blocks/spans. Neither is canonical.
`--ai-text` remains the choice for pure-text embeddings.

Images in either textual view include an anchor, alt text, and caption.
Pixels are delivered by a library-level typed multimodal export
(`Text | Image | Text`), not as base64 inside prose.

---

## `--chunks-jsonl`

One JSON object per line — **one row per index entry** with `chunk_flags & 1` by default.

**Schema (v0):**

```json
{
  "doc_id": "550e8400-e29b-41d4-a716-446655440000",
  "doc_title": "Meeting notes",
  "chunk_id": 3,
  "chunk_type": "text",
  "role": "paragraph",
  "level": null,
  "byte_len": 142,
  "text": "We agreed to ship v0 in June."
}
```

| Field | Notes |
| --- | --- |
| `doc_id`, `doc_title` | From catalog |
| `chunk_id`, `chunk_type` | From index |
| `role`, `level`, `list_kind` | From text header JSON |
| `spans` | Layout v1 ranged inline semantics |
| `text` | Body UTF-8 only |

**Cite rows** (`chunk_type: cite`):

```json
{
  "chunk_id": 8,
  "chunk_type": "cite",
  "quote": "We measured …",
  "target_doc_id": "...",
  "target_chunk_id": 12,
  "resolved_text": "We measured temperature …"
}
```

| Flag | Behavior |
| --- | --- |
| `--all-types` | Include image/slide stubs with `"text": null` |
| `--max-bytes N` | Split long text bodies (future; see [decisions](decisions.md#rag-chunking-policy)) |

**Guarantee:** Each line is valid JSON; file is UTF-8.

---

## `--markdown`

**Lossy** export for git and human editing.

| Tessera | Markdown |
| --- | --- |
| `heading` | ATX `#` |
| `list_item` | `-` or `1.` |
| `code_block` | Fenced code with optional language |
| `blockquote` | `>` |
| `table` | v0 TSV fallback; v1 structured GFM-compatible table |
| Internal link | `[title](<doc_id>.tes#chunk-12)` (vault-relative convention) |
| Cite | Pandoc-style `[@label]` plus a generated `## References` list (numeric) |

**Non-goals v0:** lossless round-trip from exported Markdown back to identical
chunks. Layout v1 adds a lossless editor profile, **Tessera Markdown**
(working nickname Tessprek), with attributes/directives for ids, spans,
placement, citations, and other enum-backed fields.

---

## `--html`

Semantic HTML5 projection of reading-order content. List-item chunks with the
same `list_kind` / `list_depth` are coalesced into a single `<ul>` / `<ol>`
(so ordered lists number `1. 2. 3.` instead of restarting). Display and inline
math render to **MathML** via KaTeX at export time (LaTeX remains the stored
source; fallback is escaped TeX in `<code class="math-fallback">`).

Generated **DOM-like** HTML5 fragment + linked theme CSS.

```html
<article data-doc-id="550e8400-e29b-41d4-a716-446655440000">
  <h2 id="chunk-2">Methods</h2>
  <p data-chunk-id="3">We measured …</p>
</article>
```

| Flag | Behavior |
| --- | --- |
| `--theme PATH` | Inject `<link rel="stylesheet">` |
| `--standalone` | Full `<!DOCTYPE html>` wrapper |
| `--embed-css` | Inline theme for single-file share |

**Mapping:** chunk `role` → element name; `class` from header JSON → `class` attribute. Presentation from theme only — **no inline styles** from exporter.

**Phase:** implemented in Phase 6.

Website/preview HTML and AI HTML are distinct profiles. The former may link a
trusted external theme and standalone scaffolding; the latter is always a
sanitized fragment.

---

## `--bibliography`

Export **BibTeX** or **CSL JSON** from cite-chunk `source` metadata (fallback:
`label` / `quote` / `page`). Import via `tes import --bibtex` / `--csl-json`.

In-text rendering uses catalog/template `cite_style_id` (v0 ships `numeric`:
`[1]`, `[2]`, …). Display style is never stored inside cite payloads.

---

## `--attachment`

Write the **opaque bytes** of one attachment chunk. This is an explicit
download path — text/HTML/AI views only list metadata and never embed or
execute attachment payloads.

| Flag | Behavior |
| --- | --- |
| `--chunk ID` | **Required** attachment chunk id |
| `-o PATH` | **Required** output path for raw bytes |

`tes serve` exposes the same bytes at `/attachment/{id}` with
`Content-Disposition: attachment` and `X-Content-Type-Options: nosniff`.

---

## `--pdf`

Paginated PDF. **Direction (D21):** native layout via the [print IR](print_ir.md)
and the **`ariadnes-weave`** crate (deterministic profiles such as `print` /
`manuscript`). Until that backend ships and becomes default, Tessera still
exports PDF by embedding print-theme CSS + image data URIs and printing through
headless Chromium/Chrome (`TES_CHROME` or auto-detect). On Linux and in CI,
Tessera passes `--no-sandbox` when needed (`TES_CHROME_NO_SANDBOX`).

| Flag | Behavior |
| --- | --- |
| `-o PATH` | **Required** output PDF path |
| `--theme-id ID` | Pack theme for **Chromium** HTML-print (default `print`; `manuscript` for `doc_kind = manuscript`) |
| `--template ID` / `--template-root DIR` | Template pack selection (Chromium path) |
| `--chapter N` | Restrict body to the Nth H1-bounded chapter (1-based; same flag on all export views) |
| `--backend native\|chromium` | Planned: select `ariadnes-weave` vs HTML-print (default flips when native is ready) |

PDF is a lossy print sink — never an editable canonical source. Browser
preview (`tes serve`) stays on semantic HTML + CSS. Native PDF and HTML preview
share **structure** (`.tes` chunks), not a single CSS pagination engine.
Manuscript / beta-reader **print profile** `manuscript` encodes Courier-like /
double-spaced policy in `ariadnes-weave`; the pack theme `manuscript` remains
for HTML/Chromium until cutover.

---

## `--meta json|yaml|toml` (layout v1 profile)

Export catalog-only metadata for agents and static-site generators. The reverse
operation, `tes meta set PATH --from meta.toml` (and YAML/JSON equivalents),
validates and updates the catalog without replacing body chunks.

---

## Import symmetry

| Import command | Builds |
| --- | --- |
| `tes import --markdown` | text chunks + optional link table |
| `tes import --html` | semantic blocks per [decisions](decisions.md#html-import) |
| `tes import --pdf` | text + optional page chunks (**Future** — not shipped) |

Import **parses once**; subsequent exports read chunks — re-exported Markdown/HTML will not match source byte-for-byte.

---

## Testing exports

Acceptance tests (fixture-driven):

1. `note_one_chunk.tes` → `--raw` equals golden `.txt`
2. Same → `--ai-text` equals golden (no tags)
3. `--chunks-jsonl` line count = reading-order text chunk count
4. `--markdown` round-trip **structure** preserved (not bytes)

Fixtures: [layout_v0 — golden fixtures](layout_v0.md#golden-fixtures-planned).
