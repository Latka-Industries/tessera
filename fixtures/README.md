# Fixtures

Test inputs and golden outputs for Tessera.

| Path | Purpose |
| --- | --- |
| `assets/` | Source documents and media for **import** pipeline tests (Markdown, HTML, images, edge cases) |
| `v0/` | Byte-exact **`.tes`** golden files once the write path lands ([layout_v0.md](../docs/layout_v0.md#golden-fixtures-planned)) |

Import tests should read from `assets/`; container tests should read from `v0/`.
