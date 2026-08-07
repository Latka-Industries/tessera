# Tessprek language server (`tes-lsp`)

**Status:** server MVP is in-tree (pack-aware completions in **0.2.7** / THI-369).
Neovim packaging lives under [`contrib/nvim/`](../contrib/nvim/README.md).

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
| Success | Refresh stored hash **and** Tessprek from `edit_read` (compile remaps media ids; pack expand rewrites prose); result includes `tessprek` for clients to replace the buffer |
| Hash conflict | `source-hash` diagnostic; **never** silent overwrite |
| Unknown `\tessera{…}` key | `tessera-unknown-key` error; **write refused** until removed |
| Parse error | Ranged `edit-parse` on the offending Tessprek line (buffer) |

## Capabilities

| Capability | Status |
| --- | --- |
| `initialize` / `shutdown` | Yes |
| `textDocument/didOpen` / `didClose` | `.tes` only; open runs `edit_read` |
| `textDocument/didChange` | Full (and incremental) apply to in-memory Tessprek |
| `textDocument/publishDiagnostics` | Buffer `decode_tessprek` (`edit-parse`, ranged) + unknown `\tessera{…}` key warnings + on-disk `verify_*` + source-hash |
| `textDocument/willSave` | Triggers write-back |
| `workspace/executeCommand` | `tessera.write` |
| `textDocument/hover` | Tessprek brace-command markers **and** body lines (chunk id / role / cite fields). Header hover shows projected catalog fields (`doc_id`, `title`, …). Neovim binds `K` like other LSP clients. |
| `textDocument/completion` | Brace commands (`\figure` …) + attribute keys inside `{…}`; triggers on `\`, `{`, space |

### `tessera.write`

Arguments (first element):

- URI string: `"file:///…/doc.tes"`
- or object: `{"uri":"file:///…/doc.tes"}`

Result JSON includes `ok`, and on success `source_hash` / `path` / `tessprek`
(post-seal projection — clients should replace the editor buffer). On hash
conflict: `ok: false`, `code: "source-hash"`.

### Hover

Hover uses the standard LSP `textDocument/hover` method (Neovim: `K`, same as
other language servers; VS Code / other clients: editor hover).

- Brace commands (`\tessera{…}` / `\ids{…}` / `\media{…}` / `\block{…}` /
  `\figure{…}` / …) — parsed attrs on any line of a multiline block
  - `\tessera{…}` — catalog fields (`doc_id`, `title`, …)
  - `\ids{…}` — reading-order list (image payloads are in `\media`, not here)
  - `\media{…}` — per-payload `media:N` metadata (type / sha256 / size)
  - `\block{…}` / `\figure{…}` — title/caption and other directive attrs
- Body lines (prose, quote, …) — **chunk id**, role/type, title/caption when
  present; figures are attrs-only (`alt=` / `media:N` via `image=`)

### Completion

Typing `\` offers Tessprek brace commands (`\figure`, `\cite`, …). Inside an
open `{…}` attribute list, attribute keys are offered (`image=`, `placement=`, …).

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

For a rich Tessprek buffer, open
[`fixtures/samples/tessprek_showcase.tes`](../fixtures/samples/tessprek_showcase.tes)
(see [fixtures/samples/README.md](../fixtures/samples/README.md)). Pack `\phrase`
lives in [`phrases_demo.tessprek`](../fixtures/samples/phrases_demo.tessprek).
Completions include `\font` / `\phrase` command snippets, plus **pack-aware**
ids inside `\font{…}` / `\phrase{…}` (and pack aliases after a typed prefix)
resolved from the document `template_id` pack — sparse overlays or master
`tessera.toml` (THI-369 / THI-367). Pack root: `TES_TEMPLATE_ROOT` or
`templates/`.

## See also

- [cli.md — Tessprek LSP](cli.md#tessprek-lsp-tes-lsp) (short pointer)
- [cli.md](cli.md) — `tes edit-read` / `edit-write` / Tessprek markers
- [engine.md](engine.md) — module map (`lsp/`)
