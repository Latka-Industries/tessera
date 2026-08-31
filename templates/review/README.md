# Review pack (`review`)

Turns on weave `[body].line_numbers` (per-column gutter). Not acmart `review`
mode and not code-fence numbers.

```bash
tes export fixtures/samples/article_bands.tes --pdf --backend native \
  --template-root templates --template review -o tmp/review.pdf
```
