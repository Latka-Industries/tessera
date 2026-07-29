# tessera.nvim (scaffold)

Thin Neovim client for **`tes-lsp`** (scaffold PR; Tessprek buffer projection follows).

Attaches the in-repo Tessprek language server over stdio when you open a
`.tes` buffer. Diagnostics and hover come from the server; this plugin does
**not** reimplement `edit-read` / `edit-write`.

> **Buffer caveat (follow-up):** opening a `.tes` file still shows raw binary bytes
> in the buffer. The server already projects Tessprek on `didOpen`; a follow-up
> will replace the buffer text with that Tessprek view and wire save →
> `tessera.write`. Until then, use `mise run lsp-smoke` for write-back checks,
> or set the buffer to Tessprek manually after `tes edit-read`.

## Requirements

- Neovim **0.10+** (`vim.lsp.start`)
- `tes-lsp` on `PATH`, or a built binary under the Tessera repo
  (`cargo build --bin tes-lsp` → `target/debug/tes-lsp`)

## Install (lazy.nvim)

Point lazy at this directory (local checkout):

```lua
{
  dir = vim.fn.expand("~/Code/LatkaIndustries/tessera/contrib/nvim"),
  name = "tessera.nvim",
  lazy = false,
  opts = {
    -- cmd = { "/abs/path/to/tes-lsp" },  -- optional override
  },
}
```

Or, if this tree is on `runtimepath` another way:

```lua
require("tessera").setup()
```

## What this PR enables

| Feature | Status |
| --- | --- |
| `*.tes` → filetype `tes` | Yes |
| Attach `tes-lsp` (stdio) | Yes |
| Diagnostics / hover from server | Yes (once buffer text is Tessprek) |
| Tessprek buffer projection + save write-back | Follow-up |

## Commands

- `:TesseraLspRestart` — stop and re-attach `tes-lsp` for the current buffer

## Manual smoke (repo root)

```bash
cargo build --bin tes-lsp
nvim --clean -u NONE \
  -c "set rtp+=$PWD/contrib/nvim" \
  -c "lua require('tessera').setup()" \
  fixtures/v0/note_one_chunk.tes
```

Inside Neovim:

```vim
:lua =vim.lsp.get_clients({ name = "tes-lsp" })
```

Expect a non-empty client list. The buffer is still binary until Tessprek
projection lands — that is expected for this scaffold.

## See also

- [docs/lsp.md](../../docs/lsp.md) — server capabilities and document model
