# Tessprek language server (`tes-lsp`)

**Status:** MVP complete (THI-241–247). Binary lives in this repo; Neovim packaging is [THI-222](https://linear.app/thicclatka/issue/THI-222).

`tes-lsp` is a thin Language Server Protocol server over **stdio** (tokio + tower-lsp).
Logs go to **stderr** only — stdout is the LSP wire.

## Document model

| Concept | Detail |
| --- | --- |
| Language id | `tessprek` (client-side; server keys by URI) |
| On-disk file | `.tes` |
| Editor buffer | Tessprek (HTML-comment directives + Markdown bodies) |
| Open | `edit_read` → buffer = Tessprek; stash `source_hash` |
| Change | `didChange` updates the in-memory Tessprek string only |
| Write-back | `tessera.write` / `willSave` → `edit_write` with stored hash |
| Success | Refresh stored hash from `EditWriteReport` |
| Hash conflict | `source-hash` diagnostic; **never** silent overwrite |

## Capabilities

| Capability | Status |
| --- | --- |
| `initialize` / `shutdown` | Yes |
| `textDocument/didOpen` / `didClose` | `.tes` only; open runs `edit_read` |
| `textDocument/didChange` | Full (and incremental) apply to in-memory Tessprek |
| `textDocument/publishDiagnostics` | `verify_*` findings + source-hash check |
| `textDocument/willSave` | Triggers write-back |
| `workspace/executeCommand` | `tessera.write` |
| `textDocument/hover` | Tessprek header + `<!-- tes chunk=… -->` markers |

### `tessera.write`

Arguments (first element):

- URI string: `"file:///…/doc.tes"`
- or object: `{"uri":"file:///…/doc.tes"}`

Result JSON includes `ok`, and on success `source_hash` / `path`. On hash
conflict: `ok: false`, `code: "source-hash"`.

### Hover

Hovering an HTML comment marker shows parsed attrs:

- `<!-- tessera: … -->` — format / version / truncated source-hash
- `<!-- tes chunk=N role=… -->` (or `type=figure|cite|…`) — chunk id + fields

## Launch

```bash
mise run tes-lsp                 # cargo build --bin tes-lsp
cargo run --bin tes-lsp          # stdio server
mise run lsp-smoke               # JSON-RPC smokes (init / open / change / write / hover)
# mise check  → fmt + clippy + test + lsp-smoke
```

Point any LSP client at `target/debug/tes-lsp` (or release) with **stdio** transport.
Open the **`.tes` path** (not a detached `.md`); the server projects Tessprek into the buffer.

## Minimal Neovim snippet

Full plugin work is tracked as [THI-222](https://linear.app/thicclatka/issue/THI-222).
Until then, a hand-rolled stdio client is enough to try hover / diagnostics:

```lua
-- ~/.config/nvim/after/plugin/tes-lsp.lua  (experimental)
vim.api.nvim_create_autocmd("FileType", {
  pattern = "tes", -- or set filetype manually for *.tes
  callback = function(args)
    vim.lsp.start({
      name = "tes-lsp",
      cmd = { vim.fn.exepath("tes-lsp") ~= "" and "tes-lsp"
        or (vim.fn.getcwd() .. "/target/debug/tes-lsp") },
      root_dir = vim.fs.root(args.buf, { ".git" }) or vim.fn.getcwd(),
    })
  end,
})
```

Notes:

- Build `tes-lsp` first (`mise run tes-lsp`).
- Raw `.tes` in the buffer is still binary until a Tessprek-aware plugin
  replaces the buffer text from `didOpen` / `edit_read` (THI-222). For smoke
  coverage today, use `mise run lsp-smoke` rather than hand-editing binary `.tes`.

## See also

- [cli.md — Tessprek LSP](cli.md#tessprek-lsp-tes-lsp) (short pointer)
- [cli.md](cli.md) — `tes edit-read` / `edit-write` / Tessprek markers
- [engine.md](engine.md) — module map (`lsp/`)
