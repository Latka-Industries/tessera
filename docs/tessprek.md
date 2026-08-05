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
| Figure, cite (block), slide, attachment | `\figure{…}` (attrs-only) / `\cite{…}`+quote / `\slide{…}` / `\attach{…}` |
| Text attrs that can't live in Markdown (`class` / `lang` / `align`) | Optional `\text{…}` immediately before the Markdown block |
| Document header | `\tessera{format=tessprek version=2 source-hash=… [doc meta…]}` |
| Reading order | `\ids{1,2,3,6,7}` (flat list, regenerated on every encode) |
| Media payloads | `\media{…}` multiline (id / media_type / sha256 / width / height) |

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
\media{
  id=5
  media_type=image/png
  sha256=9f2c…
  width=1600
  height=900
}

# Meeting notes

- Ship Tessprek v2
- Update docs

\figure{
  image=5
  placement=flow
  alt="Whiteboard photo"
  caption="Whiteboard"
}

\cite{label=Smith2024 target_chunk=2 target_byte_start=0 target_byte_end=42}
> The whiteboard sketch matches the locked design.
```

Chunk 5 (the `Image` payload the figure references) does not appear in
`\ids{}` — only the five projected reading-order chunk types
(text/figure/cite/slide/attachment) get an id slot. It is listed in
`\media{…}` with type/hash/dimensions so `media:5` is inspectable without
embedding bytes.

## Grammar

### Header

```text
\tessera{format=tessprek version=2 [source-hash=HEX] [doc_id=UUID]
         [doc_kind=note] [title="…"] [language=en] [cite_style_id=…]
         [theme_id=…] [template_id=…] [slug=…]}
\ids{ID[,ID…]}
[\media{ id=ID [media_type=…] [sha256=…] [width=N] [height=N] }]
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
between). Optional `\media{…}` may follow `\ids{}` when the doc has figures.
`tes format` / `normalize_tessprek` is lenient — it accepts free Markdown with
no header at all and synthesizes one (preserving known `\tessera{…}` identity
keys when they were already present).

### Chunk = Markdown block, not line

Everything **between** brace commands (and the initial free run before the
first one, if any) is a contiguous Markdown region. It is parsed with the same
CommonMark-subset + GFM-tables parser `tes import --markdown` uses
(`parse_markdown_blocks`): a heading, a paragraph, each list item, a
blockquote, a fenced code block, a display-math block, and each GFM table
all become **one chunk each**, in document order. There is no way to declare
the "wrong" role — it's always inferred straight from the Markdown shape.

### `\ids{…}`

A flat, comma-separated, **reading-order** list of chunk ids for every body
block (text, figure *refs*, cites, slides, attachments) — one per block. Not
every chunk in the file: image *payloads* are omitted here. Freshly regenerated
on encode; decode errors if the count doesn't match the number of parsed blocks.

### `\media{…}`

Metadata for **image payload** chunks referenced by figures (not reading-order;
never in `\ids{}`). Same multiline shape as `\figure` / `\attach`: one attr per
line; a blank line between payloads when there are several:

```text
\media{
  id=7
  media_type=image/png
  sha256=…
  width=1
  height=1

  id=12
  media_type=image/jpeg
  sha256=…
  width=800
  height=600
}
```

| Key | Notes |
| --- | --- |
| `id` | Image chunk id — target of `media:N` / `\figure{image=N}` |
| `media_type` | IANA type from the sealed payload |
| `sha256` | Hex digest of the image bytes (bytes themselves stay in `.tes`) |
| `width` / `height` | Intrinsic pixels when known |

Emitted when the document has figures; ignored on decode (regenerated from the
`.tes` on `edit-read`). `tes format` preserves declared attrs when open as a
buffer only. Legacy `\media{7}` / packed one-line entries still skip cleanly.

### `\text{title="…" caption="…" class="…" lang=… align=…}`

Optional, immediately before a Markdown block, for the few `TextHeader`
attributes Markdown can't express. Prefer the multiline form (same shape as
`\tessera{…}`):

````text
\text{
  title="Listing 1"
  caption="Prints hello"
}
```rust
fn main() {}
```
````

- `title` renders **above** the block
- `caption` renders **below** the block
- Both are valid only on `table`, `math`, and `code_block` (mermaid = fenced
  code with language `mermaid` plus this directive)

Also accepted: `class`, `lang`, `align`. Everything else — role, heading level,
list kind/depth, fence language, table structure — comes from Markdown. When a
`\text{}` precedes a multi-block Markdown run, attrs apply to **at least the
first** resulting block.

### `\figure{image=N placement=… alt="…" [region="…"] [title="…"] [caption="…"]}`

Attrs-only (same shape as `\attach{…}`): `image` points at a `\media{…}` /
`media:N` payload; `alt` is required. `title` renders above the image; `caption`
below (as `figcaption`). Multiline brace form is preferred on encode.

Legacy buffers with a following `![alt](media:N)` body are still accepted; the
Markdown alt/id win if present. New encodes do not emit that line.

### `\cite{[label=…] [target_doc=UUID] [target_chunk=N] [target_byte_start=N] [target_byte_end=N] [page=N]}`

Followed by the quoted text rendered blockquote-style (`> …`, one `>` per
line) — a `CitePayload` chunk. Block cites are lossless, including optional
`target_byte_start` / `target_byte_end` (half-open UTF-8 byte range on the
target chunk body). Same-doc cites may omit `target_doc`.

**Inline** `\cite{key}` spans (citation markers inside a paragraph, as
opposed to a standalone quote chunk) are **best-effort and not implemented in
this pass**: `InlineKind::Citation` spans still render as their plain
underlying text (matching v1). Deferred to cite/biblio follow-ups
([THI-321](https://linear.app/thicclatka/issue/THI-321)); does not block
ranged block cites ([THI-320](https://linear.app/thicclatka/issue/THI-320)).

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
3. Collect figure image payloads → `\media{…}` rows with type/hash/size (omit if
   none).
4. Per chunk: text → optional `\text{…}` + Markdown via
   `OrderedListNumbering` + `TextHeader::render_markdown_with_links_indexed`
   (contiguous ordered items become `1.` / `2.` / …; nested depths restart;
   consecutive list items stay tight — one `\n`, not a blank line);
   figure/cite/slide/attachment → brace command + body as above (no `chunk=`
   attr on the command itself — the id lives only in `\ids{}`).

**Decode** (`decode_tessprek`, strict):

1. Require `\tessera{…version=2…}` as the first non-blank header block
   (one line or multiline; extra catalog keys accepted for display; not
   applied to the sealed catalog).
2. Parse `\ids{…}`; skip optional `\media{…}` (informational; regenerated
   on encode).
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
  above) — deferred under biblio / inline-cite follow-ups (THI-321+).
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
