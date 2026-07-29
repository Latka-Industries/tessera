# Sample vault

Small tagged vault for `tes vault` demos (THI-216).

| File | Role |
| --- | --- |
| `alpha.tes` | Note tagged `notes`, `demo` |
| `beta.tes` | Note tagged `research`, `citations` |
| `gamma.tes` | Note tagged `media`, `figures` |
| `vault.tes` | Optional TOC index (`doc_kind = index`) |

```bash
cargo run --example gen_vault_fixtures
cargo run --bin tes -- vault --vault fixtures/vault list
cargo run --bin tes -- vault --vault fixtures/vault list --tag research
```

`vault.tes` embeds member mtimes. If list reports a stale index after checkout,
re-run the generator (or `tes vault --vault fixtures/vault rebuild`).
