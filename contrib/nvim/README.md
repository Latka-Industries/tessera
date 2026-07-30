# tessera.nvim

Thin Neovim client for **`tes-lsp`**.

Opens `.tes` files as **Tessprek** (via `tes edit-read`), attaches the language
server for diagnostics / hover, and writes back through `tessera.write` on
save — never dumping Tessprek text onto the binary `.tes` path.

## Requirements

- Neovim **0.10+** (`vim.lsp.start`, `vim.system`)
- `tes-lsp` and `tes` on `PATH`, or built under the Tessera repo:
  ```bash
  cargo build --bin tes --bin tes-lsp
  ```

## Install (lazy.nvim)

```lua
{
  dir = vim.fn.expand("~/Code/LatkaIndustries/tessera/contrib/nvim"),
  name = "tessera.nvim",
  lazy = false,
  opts = {
    -- cmd = { "/abs/path/to/tes-lsp" },
    -- project = true,   -- Tessprek via edit-read (default)
    -- autostart = true,
  },
}
```

Or on `runtimepath`:

```lua
require("tessera").setup()
```

## Behavior

| Action | What happens |
| --- | --- |
| Open `*.tes` | Buffer filled with Tessprek (`tes edit-read`); `tes-lsp` attaches |
| Edit | In-memory Tessprek; `didChange` to the server |
| `:w` / save | `workspace/executeCommand` → `tessera.write` (source-hash safe) |
| Hover | Chunk / header markers via server |

## Commands

- `:TesseraLspRestart` — restart `tes-lsp` on the current buffer
- `:TesseraProject` — re-run `edit-read` (discards unsaved buffer edits)

## Manual smoke (repo root)

```bash
cargo build --bin tes --bin tes-lsp
nvim --clean -u NONE \
  -c "set rtp+=$PWD/contrib/nvim" \
  -c "lua require('tessera').setup()" \
  fixtures/v0/note_one_chunk.tes
```

Expect readable Tessprek (`Hello from Tessera`), not binary garbage.

```vim
:TesseraLspInfo
:lua =vim.lsp.get_clients({ name = "tes-lsp" })
```

If clients is `{}`, check `:TesseraLspInfo` — usually `tes-lsp` is missing (`cargo build --bin tes-lsp`) while `tes` alone is enough for projection.

Edit a word, `:w`, then:

```bash
cargo run -q --bin tes -- edit-read fixtures/v0/note_one_chunk.tes | head
```

(Use a **copy** of the fixture if you do not want to dirty goldens.)

## See also

- [docs/lsp.md](../../docs/lsp.md) — server capabilities and document model
