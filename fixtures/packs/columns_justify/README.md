# Multi-column body smoke packs (THI-391)

Pair with [`../../samples/article_columns.tes`](../../samples/article_columns.tes)
(2-col then 3-col lorem; mid heading spans). Paragraph align is pack-global —
export both packs to compare flush-left vs justified column bands.

| Pack | Shows |
| --- | --- |
| `columns_left` | page chrome + `[paragraph] text_align = left` |
| `columns_justify` | page chrome + `[paragraph] text_align = justify` |

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
