# Export views — contracts

**Status:** specification for decoded **views** of a `.tes` file. Canonical storage is [layout_v0.md](layout_v0.md); exports are **projections**, not round-trip source.

Related: [layout_v0.md](layout_v0.md) (wire format), [engine.md](engine.md) (module map), [cli.md](cli.md), [decisions](decisions.md), [roadmap](roadmap.md).

---

## Principles

1. **Models read views, not wire format** — RAG pipelines call `export_ai_text()` / `chunks_jsonl`, not hex dumps.
2. **No markup escapes in AI views** — reading-order UTF-8; structure is metadata, not `**` or `<p>`.
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
| Bibliography | `--bibliography` | Research mode (later) |
| PDF | `--pdf` | Print / send (later, themed) |

v0 implementation target: **`--raw`**, **`--linear`**, **`--ai-text`**, **`--chunks-jsonl`**.

---

## `--raw`

**One chunk or whole file UTF-8** without structural decoration.

| Flag | Behavior |
| --- | --- |
| _(default)_ | Concatenate all text chunk **bodies** in reading order, `\n\n` between chunks |
| `--chunk ID` | Single text chunk body only |
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
| `role: table` | TSV body unchanged |
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
| `code_block` | Fenced ``` |
| `blockquote` | `>` |
| `table` | GFM table when detectable from TSV; else code block |
| Internal link | `[title](<doc_id>.tes#chunk-12)` (vault-relative convention) |
| Cite | Pandoc-style `[@label]` or footnote (TBD in research phase) |

**Non-goals v0:** lossless round-trip from exported Markdown back to identical chunks.

---

## `--html`

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

---

## `--bibliography` (research, later)

Emit **BibTeX** or **CSL JSON** from cite chunks + catalog metadata. Not v0.

---

## `--pdf` (print, later)

Paginated PDF via structure + print theme (`@page`, margins). Requires HTML/layout engine. Phase 7 per [roadmap](roadmap.md).

---

## Import symmetry

| Import command | Builds |
| --- | --- |
| `tes import --markdown` | text chunks + optional link table |
| `tes import --html` | semantic blocks per [decisions](decisions.md#html-import) |
| `tes import --pdf` | text + optional page chunks |

Import **parses once**; subsequent exports read chunks — re-exported Markdown/HTML will not match source byte-for-byte.

---

## Testing exports

Acceptance tests (fixture-driven):

1. `note_one_chunk.tes` → `--raw` equals golden `.txt`
2. Same → `--ai-text` equals golden (no tags)
3. `--chunks-jsonl` line count = reading-order text chunk count
4. `--markdown` round-trip **structure** preserved (not bytes)

Fixtures: [layout_v0 — golden fixtures](layout_v0.md#golden-fixtures-planned).
