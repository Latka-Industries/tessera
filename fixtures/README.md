# Fixtures

Test inputs, golden outputs, and browse samples for Tessera.

| Path | Purpose |
| --- | --- |
| `v0/` | Byte-exact **`.tes` goldens** — sole source of truth ([layout_v0.md](../docs/layout_v0.md#golden-fixtures-planned)) |
| `conformance/` | Open-format kit: `accept/` (copy of `v0/` + feature file) and `reject/` ([mime.md](../docs/mime.md)) |
| `samples/` | **Browse** `.tes` + Tessprek buffers for nvim / LSP — not golden |
| `vault/` | Sample vault + optional `vault.tes` TOC ([cli.md — tes vault](../docs/cli.md#tes-vault)) |
| `assets/` | Import / bench source documents and media |

## Where to open in Neovim

```bash
cargo build --bin tes --bin tes-lsp
nvim fixtures/samples/tessprek_showcase.tes   # sealed Tessprek element tour
```

Pack `\phrase` expands at format (not stored sealed):

```bash
# plain Tessprek buffer (not a .tes)
nvim fixtures/samples/phrases_demo.tessprek
# or CLI:
cargo run -q --bin tes -- format \
  -i fixtures/samples/phrases_demo.tessprek \
  --template-root templates --template minimal
```

## Roles (do not mix)

| Role | Edit / regenerate | CI |
| --- | --- | --- |
| Goldens | `v0/` via `gen_v0_fixtures` | Byte-exact in `golden_v0` tests; `mise fixtures` copies → `conformance/accept/` |
| Conformance gate | `accept/` (generated) + `reject/` | Deep-verify accept ok / reject fail |
| Browse | `samples/` via `gen_sample_fixtures` | Not byte-asserted |
| Import inputs | `assets/` | Used by import / export / benches |

```bash
mise run fixtures   # goldens + accept sync + rejects + vault + samples + smoke
mise run samples    # browse .tes only
```
