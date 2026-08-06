# Golden `.tes` fixtures (v0 + additive layout-v1)

Byte-exact containers for layout / writer / verify tests.

| File | Description |
| --- | --- |
| `empty.tes` | Superblock only (64 bytes) |
| `note_one_chunk.tes` | Tagged note + inline `tes textconv` span |
| `note_three_chunks.tes` | Agenda covering flags / PR preview / vault TOC |
| `hub_links.tes` | Hub doc linking to `note_one_chunk.tes` through `TLNK` |
| `external_links.tes` | `TLNK` v1: https + mailto URI heap + mixed internal edge |
| `layout_v1_text.tes` | Spans (strong/em/code), captioned math / rust / mermaid / feature-id table |
| `slide_deck.tes` | Two `title_body` slides (intro + feature flags) |
| `research_cite.tes` | Research doc with cite chunk + citation `TLNK` |
| `figure_sample.tes` | 1×1 PNG image + figure ref |
| `attachment_sample.tes` | Inert PDF attachment (`notes.pdf`) + prose |

**Sole golden source.** `conformance/accept/` is a generated copy (plus
`unknown_optional_feature.tes`) — edit / regenerate here, not under `accept/`.

Builders: [`src/fixtures/v0.rs`](../../src/fixtures/v0.rs). Regenerate:

```bash
cargo run --example gen_v0_fixtures
cp fixtures/v0/*.tes fixtures/conformance/accept/
# or: mise run fixtures
```

Golden CI: `src/tests/golden_v0.rs` asserts on-disk bytes match the shared encoders.
Deep-verify in CI runs against `conformance/accept/` (includes these files).
