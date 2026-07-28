# `tes` CLI reference

**Status:** design reference for the **`tes`** binary. Flags may change until v0.1; **`tes -h`** is authoritative after implementation.

Related: [layout_v0.md](layout_v0.md),
[structure_v1.md](structure_v1.md), [exports.md](exports.md),
[engine.md](engine.md), [roadmap.md](roadmap.md).

---

## Command summary

| Command | Alias | Role |
| --- | --- | --- |
| [`tes info`](#tes-info) `<path.tes>` | — | Summarize document (catalog + chunk table) |
| [`tes verify`](#tes-verify) `<path.tes>` | — | Layout health check (exit 1 on failure) |
| [`tes export`](#tes-export) `<path.tes>` | — | Decoded views ([exports.md](exports.md)) |
| [`tes import`](#tes-import) `<in> <out.tes>` | — | Foreign format → `.tes` (staged rollout) |
| [`tes link`](#tes-link) | — | Resolve / inspect links across a vault |
| [`tes serve`](#tes-serve) `<path.tes>` | — | Local themed browser preview |
| `tes meta <get\|set>` | — | Catalog JSON/YAML/TOML round-trip (planned) |
| [`tes edit-read`](#mutation-protocol) / `edit-write` | — | Tessera Markdown virtual editor protocol |
| [`tes apply`](#mutation-protocol) | — | Verified Tessera Markdown / typed-op mutation |
| `tes log\|diff\|checkout\|changelog` | — | M10 revision/history tools |

v0 ships **`info`**, **`verify`**, **`export`**, **`import`**, **`link`**, **`serve`**, and the **mutation** commands.

---

## Global flags

| Flag | Effect |
| --- | --- |
| `-h`, `--help` | Command help |
| `-V`, `--version` | Crate version |
| `--color auto\|always\|never` | stderr/stdout color |

**Exit codes:** `0` success, `1` user/verify error, `2` usage/IO error.

---

## `tes info`

Summarize a `.tes` file without full payload decode.

| Flag | Effect |
| --- | --- |
| _(default)_ | Human table: title, `doc_kind`, chunk counts by type, modified |
| `--json` | Full JSON: superblock, catalog, index rows (no bodies), link table |
| `-q`, `--quiet` | One line: `title\tchunks=N\tbytes=M` |
| `--chunks` | Include chunk id / type / byte len table |
| `--links` | Include link table entries |
| `-n`, `--limit N` | Cap chunk rows (default 32; `0` = all) |

**Example:**

```bash
tes info notes/standup.tes
tes info vault.tes --json | jq '.catalog.doc_id'
```

---

## `tes verify`

Validate on-disk layout per [layout_v0.md](layout_v0.md).

| Flag | Effect |
| --- | --- |
| _(default)_ | Human-readable checklist + summary |
| `--deep` | Decode every payload (zstd + UTF-8 validate) |
| `--json` | Machine-readable report |
| `-q`, `--quiet` | One line: `status=ok` or `status=failed` |

Exit code **1** when verification fails (CI-friendly).

**Example:**

```bash
tes verify manuscript.tes --json
```

Future: `tes repair` (Tetration parity) — **not v0**.

---

## `tes export`

Write a **decoded view** to stdout or `-o PATH`. See [exports.md](exports.md) for contracts.

```bash
tes export note.tes --raw
tes export note.tes --ai-text -o context.txt
tes export paper.tes --chunks-jsonl -o chunks.jsonl
tes export doc.tes --markdown -o doc.md
tes export paper.tes --pdf -o paper.pdf --theme-id print
tes export paper.tes --bibliography --bib-format bibtex -o refs.bib
tes import --bibtex fixtures/assets/citations/sample.bib refs.tes
tes export doc.tes --ai --format markdown
tes export doc.tes --meta toml
```

`--pdf` uses the same semantic HTML + template theme path as `tes serve`.
Requires a Chromium/Chrome binary (`TES_CHROME` or auto-detect). PDF is a
lossy print sink, not an editable source.

| Flag | Effect |
| --- | --- |
| `--raw` | UTF-8 text bodies ([exports — raw](exports.md#--raw)) |
| `--linear` | Reading-order text with light headings |
| `--ai-text` | LLM-oriented plain text |
| `--chunks-jsonl` | One JSON object per chunk line |
| `--markdown` | Lossy Markdown |
| `--html` | HTML fragment (+ `--theme`, `--standalone`) |
| `--pdf` | Print-theme PDF via headless Chromium (requires `-o`) |
| `--bibliography` | BibTeX / CSL-JSON from cite chunks (`--bib-format`) |
| `--bib-format` | `bibtex` (default) or `csl-json` with `--bibliography` |
| `--template ID` | Pack id for `--pdf` (default: catalog or `minimal`) |
| `--template-root DIR` | Pack root for `--pdf` (env: `TES_TEMPLATE_ROOT`) |
| `--theme-id ID` | Pack theme for `--pdf` (default: `print`) |
| `--ai --format markdown\|html` | AI-safe structured profile (planned v1) |
| `--meta json\|yaml\|toml` | Catalog metadata projection (planned v1) |
| `--chunk ID` | Single chunk (where applicable) |
| `-o`, `--output PATH` | Write file instead of stdout |
| `--annotate` | Include chunk ids in `--ai-text` |

**Default when no view flag:** error — require explicit view (avoid accidental huge stdout).

---

## `tes import`

Build a `.tes` from foreign formats. **Parse once** into chunks.

```bash
tes import --markdown article.md article.tes
tes import --html page.html page.tes
tes import --bibtex refs.bib refs.tes
tes import --pdf scan.pdf scan.tes --page-rasters
```

| Flag | Effect |
| --- | --- |
| `--markdown` | CommonMark subset ([decisions](decisions.md#markdown-import--export)) |
| `--html` | Semantic block import |
| `--bibtex` / `--csl-json` | Bibliography → research cite chunks |
| `--pdf` | Text extract + optional `--page-rasters` (not yet) |
| `--doc-kind KIND` | Override superblock `doc_kind` |
| `--title TEXT` | Catalog title |
| `--doc-id UUID` | Stable id (generate if omitted) |

v0 target: **`--markdown`** first.

DOCX import: **`--docx`** Phase 4+ (not v0 CLI).

---

## `tes link`

Vault-level link operations (requires `--vault DIR`).

| Subcommand | Effect |
| --- | --- |
| `resolve UUID[/chunk]` | Print target title + chunk preview |
| `backlinks UUID` | List docs linking to target |
| `check` | Validate all link table targets exist in vault |

**Phase:** implemented in Phase 5 ([roadmap](roadmap.md)).

---

## `tes serve`

Loopback browser preview via the same semantic HTML export used by
`tes export --html`, styled by an external template pack.

```bash
# From the repo root (ships templates/minimal):
tes serve fixtures/v0/note_one_chunk.tes --theme draft
tes serve paper.tes --theme print --watch
```

| Flag | Effect |
| --- | --- |
| `--template ID` | Pack id under `--template-root` (default: catalog `template_id` or `minimal`) |
| `--template-root DIR` | Pack search root (env: `TES_TEMPLATE_ROOT`, default `templates`) |
| `--theme ID` | Pack theme (`draft` / `print`, or catalog `theme_id`) |
| `--host` | Loopback only: `127.0.0.1`, `localhost`, or `::1` |
| `--port` | Bind port (default `7878`; `0` = ephemeral) |
| `--watch` | Inject HTML meta-refresh (no theme JavaScript) |
| `--watch-secs N` | Refresh interval when `--watch` is set |
| `--allow-theme-js` | Opt in for packs that declare `requires_theme_js` (still CSS-served) |

Routes: `/` (standalone HTML), `/theme.css` (selected theme), `/media/{chunk_id}`
(image bytes), `/healthz`. Each request re-opens the `.tes` file. CSP is CSS-only
by default. See [security.md](security.md).

---

## Mutation protocol

```bash
# Editor-neutral virtual buffer protocol.
tes edit-read paper.tes --format tessprek
tes edit-write paper.tes --format tessprek --source-hash HASH --stdin
tes edit-write paper.tes --source-hash HASH -i buffer.tessprek --dry-run

# Agent-safe structural mutation.
tes apply paper.tes --ops ops.json --source-hash HASH --dry-run
tes apply paper.tes --patch change.tessprek --source-hash HASH
```

`edit-read` prints Tessera Markdown (Tessprek) to stdout and the SHA-256
`source-hash=…` on stderr. Directives look like:

```text
<!-- tessera: format=tessprek version=1 source-hash=… -->

<!-- tes chunk=1 role=heading level=1 class="lead" -->
# Title

<!-- tes chunk=2 type=figure image=3 placement=flow caption="…" -->
![alt](media:chunk-3)
```

`edit-write` and `apply` acquire an advisory per-file lock, re-check the source
hash, compile to a sibling temporary file, deep-verify, and atomically replace.
`--dry-run` stops before replace and prints a line diff. Vim/Neovim integrations
are thin adapters over these commands.

Typed ops (`--ops`) are a JSON array of closed `TesOp` variants: `set_title`,
`set_text`, `append_paragraph`, `delete_chunk`.

---

## History commands (M10)

```bash
tes save paper.tes --draft outline -m "first cut"
tes log paper.tes
tes diff paper.tes rev-a rev-b
tes changelog paper.tes --between rev-a rev-b
```

`tes save` appends a content-addressed revision (exact-hash payload store +
chunk manifests) into the optional `THST` footer without bumping
`layout_version`. Drafts are named pointers into that revision list.
Structural `tes diff` / `tes changelog` compare chunk ids and hashes, then
show text line diffs for changed text payloads.

Every exported revision materialization (`export-revs` / checkout) remains
planned; blame, git textconv, and merge drivers are deferred.

---

## Environment variables

| Variable | Effect |
| --- | --- |
| `TES_VAULT` | Default vault directory for `tes link` |
| `TES_THEME` | Default CSS path for `--html` export |
| `TES_TEMPLATE_ROOT` | Default template pack root for `tes serve` / `--pdf` |
| `TES_CHROME` | Chromium/Chrome binary for `tes export --pdf` |
| `TES_CHROME_NO_SANDBOX` | Force `--no-sandbox` for headless print (also auto on Linux / `CI`) |
| `RUST_LOG` | `trace`/`debug` for library logging |

---

## CI usage

```yaml
- run: cargo build --release
- run: tes verify fixtures/v0/*.tes --quiet
- run: tes export fixtures/v0/note_one_chunk.tes --raw | diff -u golden.txt -
```

---

## Library parity

Every CLI command maps to a library entry point. The binary only calls
`tessera_doc::cli::run`:

| CLI | Library |
| --- | --- |
| `tes info` | `tessera_doc::catalog::read_summary_v0` |
| `tes verify` | `tessera_doc::verify::verify_tes_file` |
| `tes export` | `tessera_doc::io::export::export_view` (also `--pdf` → `render::pdf`, `--bibliography` → `io::bib`) |
| `tes import` | `tessera_doc::io::import::*` / `io::bib::import_bibliography` |
| `tes link` | `tessera_doc::vault::*` |
| `tes save` / `log` / `diff` / `changelog` | `tessera_doc::history::*` |
| `tes serve` | `tessera_doc::render::preview::serve_preview` |
| `tes edit-read` | `tessera_doc::edit::edit_read` |
| `tes edit-write` | `tessera_doc::edit::edit_write` |
| `tes apply` | `tessera_doc::edit::apply_ops` / `apply_patch` |

Embedders use the library directly; CLI is a thin wrapper.
