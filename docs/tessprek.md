# Tessprek (Tessera Markdown) v2

**Status:** locked sketch (THI-318), implemented. `.tes` stays the canonical,
sealed container; Tessprek is a **lossy editor-buffer projection** only —
`edit_write` / `apply` always recompile back through `.tes` verification, they
never treat a Tessprek buffer as a source of truth by itself.

Related: [cli.md — Mutation protocol](cli.md#mutation-protocol),
[lsp.md](lsp.md), [decisions.md](decisions.md).

---

## Design

Tessprek v2 is a **hybrid**: plain CommonMark/GFM for anything Markdown
already expresses well, and small LaTeX-lite **brace commands** (not fenced
`tessera` blocks, not HTML comments) for the handful of structured chunk types
Markdown has no syntax for.

| Content | Wire form |
| --- | --- |
| Heading, paragraph, list, blockquote, table, math, fenced code | Plain Markdown |
| Figure, cite (block), slide, attachment | `\figure{…}` / `\cite{…}` / `\slide{…}` / `\attach{…}` + body |
| Text attrs that can't live in Markdown (`class` / `lang` / `align`) | Optional `\text{…}` immediately before the Markdown block |
| Document header | `\tessera{format=tessprek version=2 source-hash=… [doc meta…]}` |
| Reading order | `\ids{1,2,3,6,7}` (flat list, regenerated on every encode) |

There is **no** per-block `\id{N}`, **no** HTML comments, and **no** YAML
front matter. v1 (`<!-- tessera: … -->` / `<!-- tes chunk=N … -->`) is
retired — `decode_tessprek` rejects anything that isn't
`\tessera{… version=2 …}` (no dual-read).

### Example

```text
\tessera{
  format=tessprek
  version=2
  source-hash=9f2c…
  doc_kind=note
  title="Meeting notes"
}
\ids{1,2,3,4}

# Meeting notes

- Ship Tessprek v2
- Update docs

\figure{image=5 placement=flow caption="Whiteboard"}
![Whiteboard photo](media:chunk-5)

\cite{label=Smith2024 target_chunk=2}
> The whiteboard sketch matches the locked design.
```

Chunk 5 (the `Image` payload the figure references) does not itself appear in
`\ids{}` — only the five projected reading-order chunk types
(text/figure/cite/slide/attachment) get an id slot; `\ids{}` mirrors
`file.reading_order_chunks()` filtered to those types.

## Grammar

### Header

```text
\tessera{format=tessprek version=2 [source-hash=HEX] [doc_id=UUID]
         [doc_kind=note] [title="…"] [language=en] [cite_style_id=…]
         [theme_id=…] [template_id=…] [slug=…]}
\ids{ID[,ID…]}
```

`format` + `version=2` are required. Everything else is optional:

| Key | Source | Notes |
| --- | --- | --- |
| `source-hash` | on-disk `.tes` SHA-256 | mutation gate (`edit_write`) |
| `doc_id` | catalog | UUID string |
| `doc_kind` | catalog | e.g. `note` |
| `title` | catalog | quoted when it contains spaces |
| `language` | catalog | BCP-47 |
| `cite_style_id` | catalog | display/export hint |
| `theme_id` / `template_id` | catalog | export/GUI hints |
| `slug` | catalog | vault-unique handle |

Encode (`edit_read` / `encode_tessprek`) projects these from the catalog when
present. Decode accepts known keys for editors/LSP (hover shows the fields) but
does **not** silently write them back into the `.tes` catalog — the sealed
catalog remains canonical. **Unknown keys** are an error in tes-lsp and
**refuse write** (so they are not silently dropped on round-trip). The header
may be **one line** or **multiline** (encode prefers multiline, one
`key=value` per indented line):

```text
\tessera{
  format=tessprek
  version=2
  source-hash=…
  doc_id=…
  title="…"
}
```

No YAML front matter. `tags` / `aliases` / `section` stay out of `\tessera{…}`.

Both `\tessera{…}` and `\ids{…}` are **required** for `decode_tessprek`
(strict): the tessera header must be the first non-blank content (one line or
a multiline block), and `\ids{}` must immediately follow (blank lines allowed
between). `tes format` / `normalize_tessprek` is lenient — it accepts free
Markdown with no header at all and synthesizes one (preserving known
`\tessera{…}` identity keys when they were already present).

### Chunk = Markdown block, not line

Everything **between** brace commands (and the initial free run before the
first one, if any) is a contiguous Markdown region. It is parsed with the same
CommonMark-subset + GFM-tables parser `tes import --markdown` uses
(`parse_markdown_blocks`): a heading, a paragraph, each list item, a
blockquote, a fenced code block, a display-math block, and each GFM table
all become **one chunk each**, in document order. There is no way to declare
the "wrong" role — it's always inferred straight from the Markdown shape.

### `\ids{…}`

A flat, comma-separated, reading-order list of the **actual** `.tes` chunk
ids for every block in the document, one per block. It is always **freshly
regenerated** on encode (never hand-preserved) and validated strictly on
decode: `decode_tessprek` errors if the count doesn't match the number of
parsed blocks.

### `\text{class="…" lang=… align=…}`

Optional, immediately before a Markdown block, for the few `TextHeader`
attributes Markdown can't express (CSS-ish theme classes, a BCP-47 language
override, or semantic alignment). Everything else — role, heading level, list
kind/depth, code fence language, table structure — comes straight from the
Markdown syntax itself, so `\text{}` is rare in practice. When a `\text{}`
precedes a Markdown run that expands into multiple blocks (e.g. `# Title`
followed by list items with no blank line), the attrs apply to **at least the
first** resulting block.

### `\figure{image=N placement=… [region="…"] [caption="…"]}`

Followed by a single Markdown image line: `![alt](media:chunk-N)`. The image
id in the Markdown link wins over the `image=` attr if they disagree (matches
`tes import --html` figure handling).

### `\cite{[label=…] [target_doc=UUID] [target_chunk=N] [page=N]}`

Followed by the quoted text rendered blockquote-style (`> …`, one `>` per
line) — a `CitePayload` chunk. Block cites are lossless.

**Inline** `\cite{key}` spans (citation markers inside a paragraph, as
opposed to a standalone quote chunk) are **best-effort and not implemented in
this pass**: `InlineKind::Citation` spans still render as their plain
underlying text (matching v1). This is an intentionally deferred piece — see
THI-318 notes — and does not block v2 landing.

### `\slide{layout=ID regions="name:chunk_id[,name:chunk_id…]"}`

No body (metadata only, mirrors `SlidePayload`).

### `\attach{filename="…" media_type=… sha256=HEX [caption="…"]}`

No body; attachment bytes are never projected into Tessprek (inert — see
`docs/security.md`).

## Encode / decode flow

**Encode** (`encode_tessprek` / `encode_content_blocks`):

1. Write a multiline `\tessera{…}` header from [`TessprekDocMeta`] / catalog
   (`format`, `version`, `source-hash`, `doc_id`, `title`, …). Single-line
   headers remain accepted on decode.
2. Collect the reading-order chunk ids → `\ids{…}`.
3. Per chunk: text → optional `\text{…}` + Markdown via
   `OrderedListNumbering` + `TextHeader::render_markdown_with_links_indexed`
   (contiguous ordered items become `1.` / `2.` / …; nested depths restart;
   consecutive list items stay tight — one `\n`, not a blank line);
   figure/cite/slide/attachment → brace command + body as above (no `chunk=`
   attr on the command itself — the id lives only in `\ids{}`).

**Decode** (`decode_tessprek`, strict):

1. Require `\tessera{…version=2…}` as the first non-blank header block
   (one line or multiline; extra catalog keys accepted for display; not
   applied to the sealed catalog).
2. Parse `\ids{…}`.
3. Scan the rest: brace commands vs. free Markdown runs (up to the next
   `\cmd{`).
4. Free Markdown → `parse_markdown_blocks` (plus a small pipe-table splitter
   for back-to-back tables with no blank line between, matching
   `tes import --markdown`).
5. Zip the resulting blocks with `\ids{}` in order; **error** on a count
   mismatch (message points at `:TesseraFormat` / format-on-save /
   `tes format` to refresh `\ids{}`).
6. `\text{…}` attrs apply to the first block of the following free run.

`normalize_tessprek` (`tes format`) reuses the same scanner but is lenient
about the header/`\ids{}` and reallocates ids positionally (reusing declared
ids where the block count lines up, else allocating fresh ones beyond the max
declared id) — see `src/edit/tessprek/format.rs`.

## Known limitations (locked-sketch scope)

- **Inline `\cite{}` spans** are not encoded/decoded specially yet (see
  above) — deferred, tracked under THI-318 follow-up.
- **Legacy TSV-bodied tables** (`role = table` with no structured `TableData`,
  a v0 read-compat path) round-trip through v2 as a `tsv`-tagged code fence,
  not a table — extremely rare in practice since the writer always populates
  structured `TableData` for new content.
- `normalize_tessprek` id reuse is purely **positional** against the flat
  `\ids{}` list; it does not try to track "this specific directive" through a
  reformat the way v1's inline per-block ids could. This only matters for
  hand-edited/malformed buffers fed to `tes format`, not the `edit_read` →
  `apply_ops`/`edit_write` round trip (which always uses real `.tes` chunk
  ids via strict `decode_tessprek`).
