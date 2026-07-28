# Golden `.tes` fixtures (v0 + additive layout-v1)

Byte-exact containers for layout / writer / verify tests.

| File | Description |
| --- | --- |
| `empty.tes` | Superblock only (64 bytes) |
| `note_one_chunk.tes` | Single paragraph note + catalog + `TIDX` |
| `note_three_chunks.tes` | Heading + paragraph + list item |
| `hub_links.tes` | Hub doc linking to `note_one_chunk.tes` through `TLNK` |
| `external_links.tes` | `TLNK` v1: https + mailto URI heap + mixed internal edge |
| `layout_v1_text.tes` | Spans, math, `code_lang`, structured table, catalog `language` |
| `slide_deck.tes` | Deck with `title_body` slide regions |
| `research_cite.tes` | Research doc with cite chunk + citation `TLNK` |
| `figure_sample.tes` | 1×1 PNG image + figure ref |
| `attachment_sample.tes` | Inert PDF attachment (`notes.pdf`) |

Regenerate after writer changes:

```bash
cargo run --example gen_v0_fixtures
cp fixtures/v0/*.tes fixtures/conformance/accept/
```

Golden CI: `src/tests/golden_v0.rs` asserts on-disk bytes match `TesWriterSession::encode_file()`.
