# Browse packs (figure / weave knobs)

Sparse packs for native PDF smoke — not product templates. Each declares a stub
`print` theme (Chromium unused here) plus a `weave.toml` overlay.

Regenerate with `cargo run --example gen_sample_fixtures` (same as samples).

## Figures

Pair with [`../samples/figure_align.tes`](../samples/figure_align.tes):

```bash
cargo run --example gen_sample_fixtures

for id in figure_left figure_center figure_right figure_caption_justify; do
  cargo run -q --bin tes --features native-pdf -- export \
    fixtures/samples/figure_align.tes \
    --pdf --backend native \
    --template-root fixtures/packs --template "$id" \
    -o "tmp/tessera-349-smoke/${id}.pdf"
done
```

All packs: caption `band = match_figure` (wraps under the image). Left/center/right use
`text_align = follow`. Justify is the same geometry with `text_align = justify`.

| Pack | Knobs to notice |
| --- | --- |
| `figure_left` | `[figure].align = left`, caption/title `follow` |
| `figure_center` | `align = center`, caption/title `follow` |
| `figure_right` | `align = right`, caption/title `follow` |
| `figure_caption_justify` | same as center, caption `text_align = justify` |

## Page chrome (THI-392)

See [`page_chrome/README.md`](page_chrome/README.md). Tokens: `{page}`, `{pages}`, `{title}`.

```bash
mkdir -p tmp/thi-392-smoke
for doc in manuscript_chapters field_notes studio_brief; do
  for pack in page_chrome page_chrome_footer_left page_chrome_footer_center page_chrome_footer_right page_chrome_fmt_slash page_chrome_fmt_of page_chrome_fmt_bare page_chrome_fmt_title_page page_chrome_header_center page_chrome_header_right; do
    cargo run -q --bin tes --features native-pdf -- export \
      "fixtures/samples/${doc}.tes" \
      --pdf --backend native \
      --template-root fixtures/packs --template "$pack" \
      -o "tmp/thi-392-smoke/${doc}__${pack}.pdf"
  done
done
```

## Hyphenation (THI-394)

See [`hyphen_on/README.md`](hyphen_on/README.md). Pair with
[`../samples/hyphen_dense.tes`](../samples/hyphen_dense.tes):

```bash
mkdir -p tmp/thi-394-smoke
for pack in hyphen_on hyphen_off hyphen_widows_3; do
  cargo run -q --bin tes --features native-pdf -- export \
    fixtures/samples/hyphen_dense.tes \
    --pdf --backend native \
    --template-root fixtures/packs --template "$pack" \
    -o "tmp/thi-394-smoke/${pack}.pdf"
done
```

## In-document TOC (THI-390)

[`../samples/manuscript_chapters.tes`](../samples/manuscript_chapters.tes) seals
`\toc{title="Contents" depth=2}` after front matter. Smoke with page chrome:

```bash
mkdir -p tmp/thi-390-smoke
cargo run -q --bin tes --features native-pdf -- export \
  fixtures/samples/manuscript_chapters.tes \
  --pdf --backend native \
  --template-root fixtures/packs --template page_chrome \
  -o tmp/thi-390-smoke/manuscript_chapters__page_chrome.pdf
```

## Multi-column body (THI-391)

See [`columns_justify/README.md`](columns_justify/README.md). Pair with
[`../samples/article_columns.tes`](../samples/article_columns.tes):

```bash
mkdir -p tmp/thi-391-smoke
for pack in columns_left columns_justify; do
  cargo run -q --bin tes --features native-pdf -- export \
    fixtures/samples/article_columns.tes \
    --pdf --backend native \
    --template-root fixtures/packs --template "$pack" \
    -o "tmp/thi-391-smoke/article_columns__${pack}.pdf"
done
```

## List of figures / tables (THI-395)

[`../samples/lists_of_floats.tes`](../samples/lists_of_floats.tes) seals
`\lof` / `\lot` before titled figures and tables. Smoke with page chrome:

```bash
mkdir -p tmp/thi-395-smoke
cargo run -q --bin tes --features native-pdf -- export \
  fixtures/samples/lists_of_floats.tes \
  --pdf --backend native \
  --template-root fixtures/packs --template page_chrome \
  -o tmp/thi-395-smoke/lists_of_floats__page_chrome.pdf
```

