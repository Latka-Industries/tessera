# Golden `.tes` fixtures (v0)

Byte-exact containers for layout / writer / verify tests.

| File | Description |
| --- | --- |
| `empty.tes` | Superblock only (64 bytes) |
| `note_one_chunk.tes` | Single paragraph note + catalog + `TIDX` |
| `note_three_chunks.tes` | *(planned)* Heading + two paragraphs |
| `hub_links.tes` | *(planned)* Hub doc + link table |

Regenerate after writer changes:

```bash
cargo run --example gen_v0_fixtures
```

Golden CI: `src/tests/golden_v0.rs` asserts on-disk bytes match `TesWriterSession::encode_file()`.
