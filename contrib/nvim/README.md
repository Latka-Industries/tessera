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
  When `contrib/nvim` is loaded from a Tessera checkout, repo `target/debug`
  (or `release`) wins over a stale `tes` / `tes-lsp` on `PATH`.

## Install (lazy.nvim)

```lua
{
  dir = vim.fn.expand("~/Code/LatkaIndustries/tessera/contrib/nvim"),
  name = "tessera.nvim",
  lazy = false,
  opts = {
    -- cmd = { "/abs/path/to/tes-lsp" },
    -- project = true,          -- Tessprek via edit-read (default)
    -- format_on_save = true,   -- default: refresh \ids{} before write
    -- conceal_directives = false, -- hide \tessera{}/\ids{}/brace-command lines while editing
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
| `:TesseraFormat` | Normalize directives from Markdown shape (`tes format`) |
| `:w` / save | Optional format-on-save, then `tessera.write`; buffer refreshed from sealed projection |
| Hover | Markers + body-line chunk id / role via server |
| Completion | Brace commands (`\figure{…}`, …) + attr keys |

### Authoring without hand-writing every directive

Tessprek is a hybrid wire: plain Markdown for heading/paragraph/list/quote/
table/math/code, plus LaTeX-lite brace commands (`\figure{}`, `\cite{}`,
`\slide{}`, `\attach{}`) for structured chunks, with a `\tessera{}` / `\ids{}`
header (see [docs/tessprek.md](../../docs/tessprek.md)). While editing you can
write Markdown-shaped bodies (`#`, `-`, fenced code, …) — including pasting
free Markdown with no header at all — then run **`:TesseraFormat`**. That
calls `tes format`, which reuses the same CommonMark inference as
`tes import --markdown` (roles, list depth, fence language) and reuses
`\ids{}` entries when possible.

```vim
:TesseraFormat
```

Optional: `format_on_save = true` (default) so `:w` refreshes `\ids{}` before
write-back. Toggle live with **`:TesseraFormatOnSave`** (or `on` / `off`);
it notifies the new state — useful when testing the write refuse path.
Optional: `conceal_directives = true` to hide directive / header
brace-command lines (`conceallevel=2`).

Hover (`K` — same as other Neovim LSP clients): brace-command markers **and**
body lines (shows chunk id / role; cite fields when present). Completion on
`\` for `\figure{…}` etc.

## Commands

- `:TesseraHover` / `K` — standard LSP hover (marker or body chunk)
- `:TesseraFormat` — normalize Tessprek via `tes format`
- `:TesseraFormatOnSave` `[on|off|toggle]` — toggle format-before-write (notifies)
- `:TesseraLspRestart` — restart `tes-lsp` on the current buffer
- `:TesseraProject` — re-run `edit-read` (discards unsaved buffer edits)
- `:TesseraLspInfo` — show resolved binaries / client status

## Manual smoke (repo root)

Prefer browse samples over goldens:

```bash
cargo build --bin tes --bin tes-lsp
# copy before write-testing — avoids dirtying the sample
cp fixtures/samples/tessprek_showcase.tes /tmp/tessprek_showcase.tes
nvim --clean -u NONE \
  -c "set rtp+=$PWD/contrib/nvim" \
  -c "lua require('tessera').setup()" \
  /tmp/tessprek_showcase.tes
```

Expect readable Tessprek (headings, `\figure`, `\cite` / `\quote` / `\ref`, …), not binary garbage.
For `\phrase` completion + pack expand, open `fixtures/samples/phrases_demo.tessprek`
(or format it with `--template-root templates --template minimal`).

```vim
:TesseraLspInfo
:TesseraFormat
:lua =vim.lsp.get_clients({ name = "tes-lsp" })
```

CLI-only format check (no Neovim):

```bash
cargo run -q --bin tes -- edit-read fixtures/samples/tessprek_showcase.tes 2>/dev/null \
  | cargo run -q --bin tes -- format --stdin \
  | cargo run -q --bin tes -- format --check --stdin
```

If clients is `{}`, check `:TesseraLspInfo` — usually `tes-lsp` is missing (`cargo build --bin tes-lsp`) while `tes` alone is enough for projection and format.

Edit a word, `:w`, then:

```bash
cargo run -q --bin tes -- edit-read /tmp/tessprek_showcase.tes | head
```

## See also

- [docs/lsp.md](../../docs/lsp.md) — server capabilities and document model
- [docs/cli.md](../../docs/cli.md) — `tes format` / edit-read / edit-write
