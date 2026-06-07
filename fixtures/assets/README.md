# Import test assets

Synthetic and downloaded media for exercising the import → chunk compiler (M4+).

## Layout

| Path | Contents |
| --- | --- |
| `markdown/rich_document.md` | Full GFM-style specimen (headings, lists, tables, code, images, links) |
| `markdown/lorem_long.md` | **~893 KiB** length stress test (50 sections × 40 paragraphs) |
| `markdown/utf8_edge_cases.md` | Unicode, emoji, smart quotes, mixed scripts |
| `markdown/minimal.md` | Single paragraph smoke test |
| `html/rich_document.html` | Semantic HTML mirror of the Markdown specimen |
| `text/plain_lorem.txt` | Plain text without markup |
| `text/lorem_long.txt` | **~893 KiB** plain-text length stress test (same paragraph count as `lorem_long.md`) |
| `images/` | Shared JPEG/PNG files referenced by `rich_document.md` and `rich_document.html` |
| `citations/sample.bib` | BibTeX stub for future research / cite-chunk tests |

## Image licenses

| File | Source | License |
| --- | --- | --- |
| `landscape.jpg`, `portrait.jpg`, `square.jpg` | [Lorem Picsum](https://picsum.photos/) | Free for testing; photos from Unsplash contributors |
| `transparency.png` | [Wikimedia Commons](https://commons.wikimedia.org/wiki/File:PNG_transparency_demonstration_1.png) | Public domain |

Do not use these assets as product branding; they exist only to test parsers and image chunk embedding.
