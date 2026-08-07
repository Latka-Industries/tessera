# Browse packs (figure / weave knobs)

Sparse packs for native PDF smoke — not product templates. Each declares a stub
`print` theme (Chromium unused here) plus a `weave.toml` overlay.

Regenerate with `cargo run --example gen_sample_fixtures` (same as samples).

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
