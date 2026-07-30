# Fixtures

Test inputs and golden outputs for Tessera.

| Path | Purpose |
| --- | --- |
| `assets/` | Source documents and media for **import** pipeline tests (Markdown, HTML, images, edge cases) |
| `v0/` | Byte-exact **`.tes`** golden files ([layout_v0.md](../docs/layout_v0.md#golden-fixtures-planned)) |
| `samples/` | Multi-role **browse** `.tes` files (not golden) — start here for Tessprek / nvim exploration |
| `vault/` | Sample vault + optional `vault.tes` TOC ([cli.md — tes vault](../docs/cli.md#tes-vault)) |
| `conformance/` | Must-accept / must-reject open-format kit ([docs/mime.md](../docs/mime.md)) |

Import tests should read from `assets/`; container tests should read from `v0/`;
human browsing / Neovim smoke from `samples/`; vault demos from `vault/`;
compatibility claims should exercise `conformance/`.
