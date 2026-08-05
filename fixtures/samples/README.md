# Browse samples

Hand-built multi-role `.tes` files for exploring Tessprek in Neovim / CLI.
**Not** byte-golden — regenerate anytime; CI does not assert these bytes.

| File | What you see |
| --- | --- |
| `text_roles.tes` | heading / paragraph / bullet + ordered `list_item` (incl. `depth`) / blockquote / captioned code+mermaid+math+table |
| `field_notes.tes` | longer research note: lists, quote, captioned scorecard/math/code, cite chunk |
| `studio_brief.tes` | deck with slides + figure + attachment + external/internal links |
| `block_captions.tes` | every caption surface: table / math / code / mermaid / figure / attachment |
| `manuscript_chapters.tes` | fiction draft (`doc_kind = manuscript`): front matter + 3 H1 chapters / H2 scene — for `--chapter N` + `manuscript` theme |

```bash
cargo run --example gen_sample_fixtures
cargo build --bin tes --bin tes-lsp
# copy before write-testing — saves rewrite the sealed container
cp fixtures/samples/field_notes.tes /tmp/field_notes.tes
nvim --clean -u NONE \
  -c "set rtp+=$PWD/contrib/nvim" \
  -c "lua require('tessera').setup()" \
  /tmp/field_notes.tes
```

Or: `tes edit-read fixtures/samples/text_roles.tes`

Minimal one-chunk / empty containers stay under [`fixtures/v0/`](../v0/) for golden + conformance tests only.
