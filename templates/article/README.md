# Article pack (`article@0`)

Journal-shaped **print profile**, not a publisher class. ACM / rho / jimis /
arxiv starters are witnesses of a recurring shape — do not clone them.

Pair with [`fixtures/samples/article_bands.tes`](../../fixtures/samples/)
(authors, abstract, keywords, definition, proof, note/Q&A, then `\columns`).

```bash
tes export fixtures/samples/article_bands.tes --pdf --backend native \
  --template-root templates --template article -o tmp/article.pdf
```

THI-397 dogfood: same sample vs a **local** gitignored golden (`tmp/latex-goldens/`, not in this repo). Gap notes: [`docs/thi-397-jimis-gaps.md`](../../docs/thi-397-jimis-gaps.md). Do not copy corpus `.tex` into `fixtures/`.

## Journal field → Tessera

| Recurring class field | Tessera |
| --- | --- |
| Title | catalog `title` / H1 |
| Author + affiliation + email | `\box{kind=author title="Name"}` + body; corresponding-author thanks → `\footnote` (THI-396) |
| Abstract | `\box{kind=abstract}` + following paragraph |
| Keywords | `\box{kind=keywords title="Keywords"}` + body |
| Theorem / definition / proof | `\box{kind=definition title="…"}` / `\box{kind=proof}` (THI-414) |
| Note / info / Q&A | `\box{kind=note\|question\|answer}` — **same** labeled box (THI-412) |
| Two-column body | `\columns{n=2}` after full-width bands (or wrap from page 1) |
| Running head / page | `weave.toml` `{heading}` / `{page}` |
| Vol / issue / DOI / received | static chrome `format` string in `weave.toml` (no `{doi}` token) |
| Author-year vs numeric cites | `\tessera{cite_style_id=…}` |
| Review line numbers | pack `review` (`[body].line_numbers`) — THI-415, not this pack |

Out: last-page column balancing, in-column floats, lettrine, listings themes,
publisher rights/CCS/ORCID, camera-ready class clones.
