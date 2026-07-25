---
title: "Specimen Document — Import Stress Test"
author: "Tessera Fixtures"
created: "2026-06-05T12:00:00Z"
tags: [fixture, markdown, import-test]
---

# Specimen Document — Import Stress Test

**Lorem ipsum** dolor sit amet, *consectetur adipiscing elit*. Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat. See also [[hub-vault-index]] and an [external reference](https://example.com/docs/specimen).

> Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla pariatur. Excepteur sint occaecat cupidatat non proident.

## Image gallery

Local files live in `fixtures/assets/images/` (paths below are relative to this `.md` file).

Landscape JPEG:

![Forest trail through morning light](../images/landscape.jpg "Landscape fixture")

Portrait JPEG:

![Vertical composition](../images/portrait.jpg)

Square JPEG:

![Square frame](../images/square.jpg)

PNG with alpha (transparency):

![RGBA transparency demo](../images/transparency.png)

Reference-style link (same square image):

![alt text via reference][square-ref]

[square-ref]: ../images/square.jpg "Reference definition"

## Typography and inline markup

This paragraph mixes **bold**, *italic*, ***bold italic***, ~~strikethrough~~, `inline code`, and a footnote marker.[^1]

[^1]: Footnote body: lorem ipsum footnote text for parser coverage.

### Heading level three

Subscript H~2~O and superscript E=mc^2^ are uncommon but appear in notes. Line break with two spaces at end of line  
continues on the next line without a paragraph break.

#### Task list (GFM)

- [x] Parse headings through level six
- [x] Embed local images
- [ ] Round-trip footnotes (future)
- [ ] Import PDF without a source fixture (convert-only)

##### Ordered and unordered lists

1. First ordered item with **emphasis**
2. Second item
   1. Nested ordered
   2. Another nested
3. Third item

- Bullet A
- Bullet B
  - Nested bullet
  - Another nested
- Bullet C

###### Heading level six

Small caps are rare; `monospace` blocks inline references like `TesWriterSession`.

---

## Code samples

Inline `let x: u64 = 0xDEADBEEF;` then a fenced Rust block:

```rust
use tessera_doc::layout::Superblock;

fn main() {
    let sb = Superblock::default_v0();
    assert_eq!(&sb.magic, b"TESS");
}
```

Python with line numbers implied by content:

```python
def chunk_count(entries: list[dict]) -> int:
    """Return reading-order text chunks."""
    return sum(1 for e in entries if e.get("role") != "metadata")
```

Shell one-liner:

```bash
tes info fixtures/v0/note_one_chunk.tes --json | jq '.chunk_count'
```

## Table

| Chunk role | Markdown source | Expected export |
| --- | --- | --- |
| `heading` | `# Title` | `# Title` in `--linear` |
| `paragraph` | Plain paragraph | Body only in `--ai-text` |
| `list_item` | `- item` | `- item` prefix |
| `table` | GFM pipe table | TSV in linear view |
| `code_block` | Fenced block | Preserved or tagged (TBD) |

## Blockquote nesting

> Level one quote begins here.
>
> > Level two nested quote with *emphasis*.
>
> Back to level one. Final sentence.

## HTML inside Markdown

<details>
<summary>Collapsible section (HTML)</summary>

<p>This block is raw HTML inside Markdown. Importers should either preserve as an opaque chunk or flatten to text — behavior is explicitly tested.</p>

</details>

## Mixed scripts and symbols

- Emoji: 📎 🦀 ✅
- Latin extended: naïve, façade, Zürich
- Greek: α β γ Δ
- CJK sample: 文档导入测试
- Arrows: → ← ↔ and math: ∑ integrals

## Link variants

- Absolute: <https://github.com/Latka-Industries/tessera>
- Relative doc: [exports spec](../../docs/exports.md)
- Autolink: https://example.com/autolink
- Wiki-style (Obsidian): [[research-notes-2026]]

## Horizontal rule above

---

Closing paragraph. *Curabitur* pretium tincidunt lacus. Nulla gravida orci a odio. Nullam varius, turpis et commodo pharetra, est eros bibendum elit, nec lorem tincidunt nisl.

**End of specimen.**
