# Browse samples

Hand-built multi-role `.tes` files for exploring Tessprek in Neovim / CLI.
**Not** byte-golden — regenerate anytime; CI does not assert these bytes.

| File | What you see |
| --- | --- |
| **`tessprek_showcase.tes`** | **Start here** — umbrella tour including live `\phrase{…}` and multi-script `\font{…}{…}` pins in one paragraph (plus spans, lists, cite family, figure, slide, …) |
| **`phrases_demo.tessprek`** | Standalone Tessprek buffer for CLI `tes format` phrase smoke |
| `text_roles.tes` | Focused text-role matrix (heading / list depth / blockquote / captioned blocks) |
| `field_notes.tes` | Longer research note (quote-style cite, scorecard) |
| `studio_brief.tes` | Deck with slides + figure + attachment + links |
| `block_captions.tes` | Caption surface matrix |
| `manuscript_chapters.tes` | Fiction draft for `--chapter N` + `manuscript` theme |

```bash
cargo run --example gen_sample_fixtures
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
