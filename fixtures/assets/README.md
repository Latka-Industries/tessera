# Import test assets

Synthetic and downloaded media for the import → chunk pipeline and benches.

## Layout

| Path | Contents |
| --- | --- |
| `markdown/rich_document.md` | Full GFM-style specimen (headings, lists, tables, code, images, links) |
| `markdown/lorem_long.md` | **~893 KiB** length stress (50 sections × 40 paragraphs) |
| `markdown/minimal.md` | Single paragraph smoke |
| `markdown/layout_v1_sample.md` | Math + fenced rust for layout-v1 import smoke |
| `html/rich_document.html` | Semantic HTML mirror of the Markdown specimen |
| `images/` | Shared JPEG/PNG referenced by `rich_document.md` / `.html` |
| `citations/sample.bib` | BibTeX stub for research / cite-chunk tests |

## Image licenses

| File | Source | License |
| --- | --- | --- |
| `landscape.jpg`, `portrait.jpg`, `square.jpg` | [Lorem Picsum](https://picsum.photos/) | Free for testing; photos from Unsplash contributors |
| `transparency.png` | [Wikimedia Commons](https://commons.wikimedia.org/wiki/File:PNG_transparency_demonstration_1.png) | Public domain |

Do not use these assets as product branding; they exist only to test parsers and image chunk embedding.
