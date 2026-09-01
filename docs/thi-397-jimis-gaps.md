# THI-397 — article dogfood gaps

**Do not copy corpus LaTeX into git.** Dropbox / `tmp/latex-corpus/` and
`tmp/latex-goldens/` are local witnesses (gitignored). Tracked samples are
original Tessera prose.

Witness (read-only): `tmp/latex-goldens/jimis-article/main.pdf` (1 page A4,
two-column French article). Shape only — not a clone target and not a fixture.

Tracked sample: [`fixtures/samples/article_bands.tes`](../fixtures/samples/article_bands.tes)
+ pack `article`.

```bash
cargo run --example gen_sample_fixtures
mkdir -p tmp/thi-397-smoke
cargo run -q --bin tes --features native-pdf -- export \
  fixtures/samples/article_bands.tes --pdf --backend native \
  --template-root templates --template article \
  -o tmp/thi-397-smoke/article_bands-native.pdf
cargo run -q --bin tes -- export \
  fixtures/samples/article_bands.tes --pdf --backend chromium \
  --template-root templates --template article \
  -o tmp/thi-397-smoke/article_bands-chromium.pdf
```

Open side-by-side: golden (local) · native · Chromium.

## Good enough (present)

| Witness shape | Tessera sample |
| --- | --- |
| Title + authors | H1 + `kind=author` band |
| Abstract + keywords | `\abstract` / `kind=keywords` |
| Named definition | `\box{kind=definition}` — same lined box paint (THI-414) |
| Two-column body | `\columns{n=2}` after full-width bands (paragraphs only in the region) |
| Running page | pack `{heading}` / `{page}` |

## Native columns (why a short region looks one-col)

Weave `PrintBlock::Columns` flows **paragraphs / lists / quotes / code** down
each column. Headings, callouts, figures, tables, and display math **span**
full measure and flush the band (THI-391). A region with one short paragraph
only fills column 0 — that is the packer, not a missing `\columns` emit.

`article_bands.tes` keeps titled bands full-width, then several paragraphs in
`\columns` so the 2-col band can actually fill.

## Gaps — accept for 0.3.0 (already routed)

| Gap | Where it lives |
| --- | --- |
| Native 2-col **spans** tables/math/figures full measure; some witnesses put a table in one column | [THI-391](https://linear.app/thicclatka/issue/THI-391) — locked |
| No last-page column balancing | THI-391 out |
| Witness geometry (tight A4 10 pt two-col from page 1) vs pack defaults | pack knobs; not a new ticket |
| Chromium HTML print may be Letter, not A4 | pack `print.css` |
| Theorem paint label is English (`Definition (…)`.) | Tessera-owned `callout_band_title` |
| Inline `$…$` in prose is not TeX | display `Math` role only |
| Cite chunks dump an English **References** heading | not grown here |

## Chromium vs native

Chromium uses pack CSS columns + `<aside class="tes-callout">`. Native uses
weave Callout + column flow. They are not required to match each other or the
LaTeX golden.

## Out of this ticket

Homework-only callouts, thesis/book front matter, ACM class port, TikZ,
publisher chrome. Do not vendor corpus `.tex` / `.pdf` / starter prose in
`fixtures/`.
