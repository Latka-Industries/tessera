# THI-397 — jimis-article reseal gaps

Witness (read-only, gitignored): `tmp/latex-goldens/jimis-article/main.pdf`
(1 page, A4, two-column French article). Corpus source:
`tmp/latex-corpus/extra_latex_templates/jimis-article/main.tex`.

Tessera reseal: [`fixtures/samples/jimis_article.tes`](../fixtures/samples/jimis_article.tes)
via pack `article`. No new print primitives in this ticket.

```bash
cargo run --example gen_sample_fixtures
mkdir -p tmp/thi-397-smoke
cargo run -q --bin tes --features native-pdf -- export \
  fixtures/samples/jimis_article.tes --pdf --backend native \
  --template-root templates --template article \
  -o tmp/thi-397-smoke/jimis-native.pdf
cargo run -q --bin tes -- export \
  fixtures/samples/jimis_article.tes --pdf --backend chromium \
  --template-root templates --template article \
  -o tmp/thi-397-smoke/jimis-chromium.pdf
```

Open side-by-side: golden `main.pdf` · native · Chromium.

## Good enough (present)

| jimis shape | Tessera |
| --- | --- |
| Title + two authors | H1 + `kind=author` bands |
| Corresponding-author `\thanks` | footnote on Camille (THI-396) |
| Abstract + mots-clés | `\abstract` title **Résumé** / `kind=keywords` title **Mots-clés** |
| `definition[Trace minimale]` | `\theorem{kind=definition}` — same Callout paint (THI-414) |
| Weighted-mean display `\bar` / `\frac` | `TextRole::Math` (weave 0.2.13 / THI-385) |
| Two-column body | `\columns{n=2}` after the title block |
| Numbered bibliography | H2 Références + two paragraphs (no in-text `\cite` in the source) |
| Running page | pack `{heading}` / `{page}` |

## Measured this reseal

| PDF | Pages | Size |
| --- | --- | --- |
| LaTeX golden | **1** | A4 |
| Native (`--backend native`, weave 0.2.13) | **3** | A4 |
| Chromium (`--backend chromium`) | **2** | **Letter** (pack HTML/CSS; not A4) |

Native running header on page 1 is the last H2 on the page (`Protocole`), not a journal short title. Author names are titled bands, not `\maketitle` centered names. Display math paints; **inline `$x_i$` / `$m_i$` stay literal dollars** in both backends.

## Gaps — accept for 0.3.0 (already routed)

| Gap | Where it lives |
| --- | --- |
| 3 native pages vs 1 TeX page (type size, margins, titled-band height, no column balance) | densify/geometry; **not** a clone target |
| Native 2-col **spans** the table full measure; jimis `table[t]` sits in one column | [THI-391](https://linear.app/thicclatka/issue/THI-391) — locked |
| No last-page column balancing (`balance.sty`) | THI-391 out |
| Tight jimis geometry (2.2 cm / 1.7 cm, 10 pt, `columnsep=0.8 cm`) vs pack defaults | pack knobs; not a new ticket |
| Chromium page is Letter, not A4 | HTML print CSS; not grown here |
| Journal “JIMIS — volume à compléter” is a body paragraph, not `\date` / running chrome | static chrome in `weave.toml` is allowed; article pack keeps `{heading}`/`{page}` |
| Theorem paint label is English **Definition (Trace minimale).** not **Définition 1** | Tessera-owned `callout_band_title`; not an amsthm clone |
| Inline `$…$` in prose is not TeX | display `Math` role only; do not invent inline math in 397 |
| `\ref{tab:trace}` dropped (prose says “le tableau suivant”) | `\ref` is a cite-family pointer, not a float counter |
| Bibliography is heading + paragraphs, not `thebibliography`. Cite chunks would dump English **References** after `\endcolumns` | cite dump copy is English-only |
| French babel hyphenation / T1 fonts vs weave ASCII hyphen + Liberation | accept |
| Hyperref colored links | out |

## Chromium vs native

Chromium: 2 Letter pages, CSS columns, HTML `<aside class="tes-callout">`. Native: 3 A4 pages, weave Callout + column flow. Neither matches the 1-page golden; they are not required to match each other. Files: `tmp/thi-397-smoke/jimis-native.pdf`, `tmp/thi-397-smoke/jimis-chromium.pdf`.

## Out of this ticket

Homework callouts, thesis/book front matter, ACM class port, TikZ, publisher chrome. Optional 1-col witness `compositionality` was not resealed.
