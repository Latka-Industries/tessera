# Browse samples

Hand-built multi-role `.tes` files for exploring Tessprek in Neovim / CLI.
**Not** byte-golden — regenerate anytime; CI does not assert these bytes.

| File | What you see |
| --- | --- |
| **`tessprek_showcase.tes`** | **Start here** — sealed results (expanded phrase, font-pinned scripts, spans, lists, cite, figure, slide, …) |
| **`phrases_demo.tessprek`** | Tessprek **source** for `tes format` phrase smoke (raw `\phrase` lives here, not in the .tes) |
| `text_roles.tes` | Focused text-role matrix (heading / list depth / blockquote / captioned blocks) |
| `field_notes.tes` | Longer research note (quote-style cite, scorecard) |
| `studio_brief.tes` | Deck with slides + figure + attachment + links |
| `block_captions.tes` | Caption surface matrix |
| `figure_align.tes` | Figure title/caption band tour (240×120 swatch) — pair with [`../packs/`](../packs/) `figure_*` weave overlays |
| `article_columns.tes` | 2- then 3-column newspaper body (THI-391) |
| `article_bands.tes` | Titled bands (author/abstract/definition/Q&A) then `\columns` (THI-411 / 412 / 414) |
| `jimis_article.tes` | THI-397 reseal of the jimis-article witness — see [`docs/thi-397-jimis-gaps.md`](../../docs/thi-397-jimis-gaps.md) |
| `mixed_align.tes` | Per-chunk / columns `align` mix (THI-398): start lead, justify columns, center aside |
| `manuscript_chapters.tes` | Fiction draft for `--chapter N` + `manuscript` theme |

```bash
cargo run --example gen_sample_fixtures   # samples + fixtures/packs/figure_*
# or: mise run samples

cargo build --bin tes --bin tes-lsp
# copy before write-testing — saves rewrite the sealed container
cp fixtures/samples/tessprek_showcase.tes /tmp/tessprek_showcase.tes
nvim --clean -u NONE \
  -c "set rtp+=$PWD/contrib/nvim" \
  -c "lua require('tessera').setup()" \
  /tmp/tessprek_showcase.tes
```

Phrase expand (CLI):

```bash
cargo run -q --bin tes -- format \
  -i fixtures/samples/phrases_demo.tessprek \
  --template-root templates --template minimal
```

Minimal one-chunk / empty containers stay under [`fixtures/v0/`](../v0/) for golden + conformance tests only.
