# Front-matter pack (`frontmatter`)

Thesis/book **chrome**, not a KOMA/mimosis clone. Tessprek stays headings +
`\toc` — there is no `\frontmatter` command.

* Title page: centered H1 / subtitle / degree line as ordinary headings (or
  `\layout` place). Manuscript profile already page-breaks on H1.
* Unnumbered front chapters: heading text without a pack counter.
* Roman page labels: `[page.numbers] style = "roman"` (`{page_roman}` always
  roman even when style is arabic).
* Twoside running heads: `align_even` / `format_even` on header/footer.

Out: glossaries, KOMA chapter numbers, publisher thesis classes.

Smoke with [`fixtures/samples/manuscript_chapters.tes`](../../fixtures/samples/):

```bash
tes export fixtures/samples/manuscript_chapters.tes --pdf --backend native \
  --template-root templates --template frontmatter -o tmp/frontmatter.pdf
```
