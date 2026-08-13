# Hyphenation smoke packs (THI-394)

Narrow indent band (`indent.step = 48`) + dense long words in
[`../../samples/hyphen_dense.tes`](../../samples/hyphen_dense.tes).
Compare `hyphen_on` vs `hyphen_off` side by side.

| Pack | Shows |
| --- | --- |
| `hyphen_on` | `hyphenate = true`, orphan/widow 2, `indent.step = 48` |
| `hyphen_off` | `hyphenate = false`, orphan/widow 2, `indent.step = 48` |
| `hyphen_widows_3` | `hyphenate = true`, `widow_lines = 3` |

```bash
mkdir -p tmp/thi-394-smoke
for pack in hyphen_on hyphen_off hyphen_widows_3; do
  cargo run -q --bin tes -- export fixtures/samples/hyphen_dense.tes \
    --pdf --backend native \
    --template-root fixtures/packs --template "$pack" \
    -o "tmp/thi-394-smoke/${pack}.pdf"
done
```
