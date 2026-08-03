# Tessprek language server (`tes-lsp`)

**Status:** server MVP is in-tree. Neovim packaging lives under
[`contrib/nvim/`](../contrib/nvim/README.md).

`tes-lsp` is a thin Language Server Protocol server over **stdio** (tokio + tower-lsp).
Logs go to **stderr** only — stdout is the LSP wire.

## Document model

| Concept | Detail |
| --- | --- |
| Language id | `tessprek` (client-side; server keys by URI) |
| On-disk file | `.tes` |
| Editor buffer | Tessprek v2 (hybrid Markdown + LaTeX-lite brace commands; [docs/tessprek.md](tessprek.md)) |
| Open | `edit_read` → buffer = Tessprek; stash `source_hash` |
| Change | `didChange` updates the in-memory Tessprek string only |
| Write-back | `tessera.write` / `willSave` → `edit_write` with stored hash |
| Success | Refresh stored hash from `EditWriteReport` |
| Hash conflict | `source-hash` diagnostic; **never** silent overwrite |
| Parse error | Ranged `edit-parse` on the offending Tessprek line (buffer) |

## Capabilities

| Capability | Status |
| --- | --- |
| `initialize` / `shutdown` | Yes |
| `textDocument/didOpen` / `didClose` | `.tes` only; open runs `edit_read` |
| `textDocument/didChange` | Full (and incremental) apply to in-memory Tessprek |
| `textDocument/publishDiagnostics` | Buffer `decode_tessprek` (`edit-parse`, ranged) + on-disk `verify_*` + source-hash |
| `textDocument/willSave` | Triggers write-back |
| `workspace/executeCommand` | `tessera.write` |
| `textDocument/hover` | Tessprek `\tessera{}` / `\ids{}` / brace-command lines only (not prose). Neovim plugin binds `K`. |

### `tessera.write`

Arguments (first element):

- URI string: `"file:///…/doc.tes"`
- or object: `{"uri":"file:///…/doc.tes"}`

Result JSON includes `ok`, and on success `source_hash` / `path`. On hash
conflict: `ok: false`, `code: "source-hash"`.

### Hover

Hovering a Tessprek brace-command marker shows its parsed attrs:

- `\tessera{…}` — format / version / truncated source-hash
- `\ids{…}` — the reading-order id list
- `\text{…}` / `\figure{…}` / `\cite{…}` / `\slide{…}` / `\attach{…}` — parsed attrs

## Launch

```bash
cargo build --bin tes-lsp        # → target/debug/tes-lsp
cargo run --bin tes-lsp          # stdio server
# optional wrappers: mise run tes-lsp / mise run lsp-smoke
mise run lsp-smoke               # JSON-RPC smokes (init / open / change / write / hover)
# mise check  → fmt + clippy + test + lsp-smoke
```

Point any LSP client at `target/debug/tes-lsp` (or release) with **stdio** transport.
Open the **`.tes` path** (not a detached `.md`); the server projects Tessprek into the buffer.

## Minimal Neovim client

Use the in-repo plugin under [`contrib/nvim/`](../contrib/nvim/README.md).
Opens `.tes` as Tessprek, attaches `tes-lsp`, and saves via `tessera.write`.

```lua
-- lazy.nvim
{
  dir = vim.fn.expand("~/Code/LatkaIndustries/tessera/contrib/nvim"),
  name = "tessera.nvim",
  lazy = false,
  opts = {},
}
```

Build the server first: `cargo build --bin tes-lsp`.

## See also

- [cli.md — Tessprek LSP](cli.md#tessprek-lsp-tes-lsp) (short pointer)
- [cli.md](cli.md) — `tes edit-read` / `edit-write` / Tessprek markers
- [engine.md](engine.md) — module map (`lsp/`)
