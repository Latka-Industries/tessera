# `tes` CLI reference

**Status:** design reference for the **`tes`** binary. Flags may change until v0.1; **`tes -h`** is authoritative after implementation. A second binary, **`tes-lsp`**, speaks LSP over stdio for Tessprek (scaffold — see [Tessprek LSP](#tessprek-lsp-tes-lsp)).

Related: [layout_v0.md](layout_v0.md),
[structure_v1.md](structure_v1.md), [exports.md](exports.md),
[engine.md](engine.md), [roadmap.md](roadmap.md).

---

## Command summary

| Command                                                                                 | Alias | Role                                          |
| --------------------------------------------------------------------------------------- | ----- | --------------------------------------------- |
| [`tes info`](#tes-info) `<path.tes>`                                                    | —     | Summarize document (catalog + chunk table)    |
| [`tes verify`](#tes-verify) `<path.tes>`                                                | —     | Layout health check (exit 1 on failure)       |
| [`tes repair`](#tes-repair) `<path.tes>`                                                | —     | Salvage damaged containers (plan by default)  |
| [`tes export`](#tes-export) `<path.tes>`                                                | —     | Decoded views ([exports.md](exports.md))      |
| [`tes import`](#tes-import) `<in> <out.tes>`                                            | —     | Foreign format → `.tes` (staged rollout)      |
| [`tes link`](#tes-link)                                                                 | —     | Resolve / inspect links across a vault        |
| [`tes vault`](#tes-vault)                                                               | —     | Optional `vault.tes` TOC + FTS search         |
| [`tes serve`](#tes-serve) `<path.tes>`                                                  | —     | Local themed browser preview                  |
| `tes meta <get\|set>`                                                                   | —     | Catalog JSON/YAML/TOML round-trip (planned)   |
| [`tes edit-read`](#mutation-protocol) / `format` / `edit-write`                         | —     | Tessera Markdown virtual editor protocol      |
| [`tes apply`](#mutation-protocol)                                                       | —     | Verified Tessera Markdown / typed-op mutation |
| `tes log\|diff\|changelog\|blame\|pending\|export-revs\|checkout\|textconv\|merge-file` | —     | M10 revision/history tools                    |

v0 ships **`info`**, **`verify`**, **`repair`**, **`export`**, **`import`**, **`link`**,
**`vault`**, **`serve`**, **mutation**, and **history** commands.

---

## Global flags

| Flag            | Effect        |
| --------------- | ------------- |
| `-h`, `--help`  | Command help  |
| `-V`, `--version` | Crate version |

**Exit codes:** `0` success, `1` user/verify error, `2` usage/IO error.

---

## `tes info`

Summarize a `.tes` file without full payload decode.

| Flag            | Effect                                                             |
| --------------- | ------------------------------------------------------------------ |
| _(default)_     | Human table: title, `doc_kind`, chunk counts by type, modified     |
| `--json`        | Full JSON: superblock, catalog, index rows (no bodies), link table |
| `-q`, `--quiet` | One line: `title\tchunks=N\tbytes=M`                               |
| `--copy`        | Read whole file into memory (no mmap; safer on NFS / untrusted)   |

**Example:**

```bash
tes info notes/standup.tes
tes info vault.tes --json | jq '.catalog.doc_id'
```

---

## `tes verify`

Validate on-disk layout per [layout_v0.md](layout_v0.md).

| Flag            | Effect                                       |
| --------------- | -------------------------------------------- |
| _(default)_     | Human-readable checklist + summary           |
| `--deep`        | Decode every payload (zstd + UTF-8 validate) |
| `--copy`        | Read whole file into memory (no mmap)        |
| `--json`        | Machine-readable report                      |
| `-q`, `--quiet` | One line: `status=ok` or `status=failed`     |

Exit code **1** when verification fails (CI-friendly).

**Example:**

```bash
tes verify manuscript.tes --json
```

---

## `tes repair`

Salvage damaged `.tes` containers. Complements [`tes verify`](#tes-verify)
(`--deep`). Verify stays read-only; mutations happen only here. **Never invents
semantic content** — clears bad history flags or drops out-of-bounds chunks and
rewrites a sealed body (THST rebuild is out of scope).

```bash
tes repair damaged.tes                          # plan only
tes repair damaged.tes --apply-all --dry-run
tes repair damaged.tes --apply footer_invalid
tes repair truncated.tes --apply drop_oob_chunks -o fixed.tes
```

| Flag | Effect |
| --- | --- |
| _(default)_ | Plan from verify findings; no writes |
| `--apply CODE` | Apply one or more codes (`footer_invalid`, `drop_oob_chunks`) |
| `--apply-all` | Apply every repairable code from the plan |
| `--dry-run` | Show would-apply messages without writing |
| `-o`, `--output PATH` | Write repaired bytes here (default: replace input) |
| `--json` | Machine-readable report |

| Code | Action |
| --- | --- |
| `footer_invalid` | Clear `HISTORY_FOOTER` / strip broken `THST` trailer |
| `drop_oob_chunks` | Drop index rows whose payloads extend past EOF; rewrite sealed body |

Unrepairable findings (bad magic, corrupt catalog, …) are listed as `manual` /
`unrecoverable` in the report.

---

## `tes export`

Write a **decoded view** to stdout or `-o PATH`. See [exports.md](exports.md) for contracts.

```bash
tes export note.tes --raw
tes export note.tes --ai-text -o context.txt
tes export paper.tes --chunks-jsonl -o chunks.jsonl
tes export doc.tes --markdown -o doc.md
tes export paper.tes --pdf -o paper.pdf --theme-id print
tes export draft.tes --pdf -o ch2.pdf --chapter 2 --theme-id manuscript
tes export note.tes --pdf --backend native -o note.pdf
tes export draft.tes --markdown --chapter 1
tes export paper.tes --bibliography --bib-format bibtex -o refs.bib
tes export note.tes --attachment --chunk 3 -o notes.pdf
tes import --bibtex fixtures/assets/citations/sample.bib refs.tes
# Planned (not in clap yet):
# tes export doc.tes --ai --format markdown
# tes export doc.tes --meta toml
```

`--pdf` defaults to **Chromium**: same semantic HTML + template theme path as
`tes serve` (requires `TES_CHROME` or auto-detect). `--backend native` (0.2.0,
Cargo feature `native-pdf`) uses print IR → [`ariadnes-weave`](print_ir.md)
with no browser. PDF is a lossy print sink, not an editable source. For
`doc_kind = manuscript`, the default pack theme / native profile is
`manuscript` instead of academic `print`. `--chapter N` scopes **any** export
view to the Nth H1-bounded chapter (conflicts with `--chunk`).

| Flag                           | Effect                                                            |
| ------------------------------ | ----------------------------------------------------------------- |
| `--raw`                        | UTF-8 text bodies ([exports — raw](exports.md#--raw))             |
| `--linear`                     | Reading-order text with light headings                            |
| `--ai-text`                    | LLM-oriented plain text                                           |
| `--chunks-jsonl`               | One JSON object per chunk line                                    |
| `--markdown`                   | Lossy Markdown                                                    |
| `--html`                       | HTML fragment (+ `--theme`, `--standalone`, `--embed-css`)        |
| `--pdf`                        | PDF export (requires `-o`; engine via `--backend`)                |
| `--backend chromium\|native`   | PDF engine (default `chromium`; `native` needs Cargo `native-pdf`) |
| `--bibliography`               | BibTeX / CSL-JSON from cite chunks (`--bib-format`)               |
| `--attachment`                 | Write opaque attachment chunk bytes (requires `--chunk` and `-o`) |
| `--bib-format`                 | `bibtex` (default) or `csl-json` with `--bibliography`            |
| `--template ID`                | Pack id for `--pdf` (default: catalog or `minimal`; native uses pack `weave.toml` when present) |
| `--template-root DIR`          | Pack root for `--pdf` (env: `TES_TEMPLATE_ROOT`)                  |
| `--theme-id ID`                | Pack theme / native profile hint (`print`, `manuscript`, …)       |
| `--theme PATH`                 | Stylesheet path/href for `--html`                                 |
| `--standalone`                 | Complete HTML document with `--html`                              |
| `--embed-css`                  | Embed `--theme` CSS into `--html` output                          |
| `--include-headers`            | Prefix each `--raw` chunk with a debug header                     |
| `--all-types`                  | Include non-text rows in `--chunks-jsonl`                         |
| `--no-cites`                   | Omit cite expansion from `--ai-text`                              |
| `--ai --format markdown\|html` | AI-safe structured profile (planned)                              |
| `--meta json\|yaml\|toml`      | Catalog metadata projection (planned)                             |
| `--chunk ID`                   | Single chunk (where applicable; conflicts with `--chapter`)       |
| `--chapter N`                  | Nth chapter by H1 boundaries (1-based; conflicts with `--chunk`)  |
| `-o`, `--output PATH`          | Write file instead of stdout                                      |
| `--annotate`                   | Include chunk ids in `--ai-text`                                  |

**Default when no view flag:** error — require explicit view (avoid accidental huge stdout).

---

## `tes import`

Build a `.tes` from foreign formats. **Parse once** into chunks.

```bash
tes import --markdown article.md article.tes
tes import --html page.html page.tes
tes import --bibtex refs.bib refs.tes
# Planned: tes import --pdf scan.pdf scan.tes --page-rasters
```

| Flag                      | Effect                                                                |
| ------------------------- | --------------------------------------------------------------------- |
| `--markdown`              | CommonMark subset ([decisions](decisions.md#markdown-import--export)) |
| `--html`                  | Semantic block import                                                 |
| `--bibtex` / `--csl-json` | Bibliography → research cite chunks                                   |
| `--pdf`                   | Text extract + optional `--page-rasters` (not yet)                    |
| `--doc-kind KIND`         | Override superblock `doc_kind`                                        |
| `--title TEXT`            | Catalog title                                                         |
| `--doc-id UUID`           | Stable id (generate if omitted)                                       |

Shipped today: **`--markdown`**, **`--html`**, **`--bibtex`** / **`--csl-json`**.

DOCX import: **`--docx`** Future (not v0 CLI).

---

## `tes link`

Vault-level link operations (requires `--vault DIR`).

| Subcommand             | Effect                                         |
| ---------------------- | ---------------------------------------------- |
| `resolve UUID[/chunk]` | Print target title + chunk preview             |
| `backlinks UUID`       | List docs linking to target                    |
| `check`                | Validate all link table targets exist in vault |

**Phase:** implemented in Phase 5 ([roadmap](roadmap.md)).

---

## `tes vault`

Optional vault catalog index — a TOC-style `vault.tes` sidecar
(`doc_kind = index`) listing `doc_id → title, tags, category, section, aliases, slug,
modified, path` so list does not open every note. `tes vault search` defaults
to a **parallel scan** of membership (`--ai-text` + catalog fields). For larger
vaults (≥ 64 docs) or with `--index`, it uses a **Tantivy** BM25 index under
`.tessera/fts/`. Index version ≥ 2 also stores **registered members** (external
`.tes` files or extra roots). Version ≥ 3 rows may include `category` /
`aliases` / `slug`. Version ≥ 4 adds `section` (nested path under `category`).
`tes link` and search use the same membership set.

```bash
tes vault --vault ./notes rebuild
tes vault --vault ./notes list
tes vault --vault ./notes list --tag ml --json
tes vault --vault ./notes list --category Literature
tes vault --vault ./notes list --section Books/Authors
tes vault --vault ./notes list --force-scan
tes vault --vault ./notes search "phrase"
tes vault --vault ./notes search "phrase" --scan
tes vault --vault ./notes search "title:Alpha OR xylophone" --index --json
tes vault --vault ./notes search --rebuild
tes vault --vault ./notes search "phrase" --force-rebuild
tes vault --vault ./notes import /path/to/obsidian-vault
tes vault --vault ./notes add /other/project/note.tes
tes vault --vault ./notes add /other/shared-notes
tes vault --vault ./notes members
tes vault --vault ./notes remove /other/project/note.tes
```

| Subcommand      | Effect                                                                      |
| --------------- | --------------------------------------------------------------------------- |
| `rebuild`       | Scan membership and seal/replace `vault.tes` (preserves registered members) |
| `list`          | List docs from a fresh index, else catalog scan (warn if stale)             |
| `search QUERY`  | Parallel scan by default; Tantivy when ≥ 64 docs or `--index`               |
| `import DIR`    | Import Markdown/Obsidian tree into the vault (skip `.obsidian`)             |
| `add PATH`      | Register a `.tes` file or extra root; rebuilds `vault.tes`                  |
| `remove PATH`   | Unregister a previous `add`; rebuilds `vault.tes`                           |
| `members`       | Show registered externals only (not the automatic in-tree scan)             |

| Flag           | Effect                                                                     |
| -------------- | -------------------------------------------------------------------------- |
| `--tag TAG`    | Keep rows whose catalog tags include `TAG`                                 |
| `--category C` | Keep rows whose catalog category equals `C`                                |
| `--section S`  | Keep rows whose catalog section equals `S` (e.g. `Books/Authors`)          |
| `--force-scan` | Ignore TOC freshness and rescan catalogs (still honors members)            |
| `--scan`       | (`search`) Force parallel membership scan (no sidecar)                     |
| `--index`      | (`search`) Force Tantivy under `.tessera/fts`                              |
| `--rebuild`    | (`search`) Rebuild Tantivy index; with no query, rebuild only              |
| `--force-rebuild` | (`search`) Always rebuild before an indexed query                       |
| `--limit N`    | (`search`) Max hits (default 50)                                           |
| `--json`       | Machine-readable list / search / member report                             |
| `--table`      | Force aligned table (default when stdout is a TTY)                         |
| `--tsv`        | Force tab-separated rows (default when stdout is not a TTY)                |

Interactive terminals get an aligned table
(`KIND TITLE PATH CATEGORY SECTION SLUG` plus a short `DOC_ID` prefix). Pipes
and `--tsv` keep the legacy TSV shape (optional `section=…` extras).

**Import behavior:** preserves relative paths as `.tes`; `doc_id` from Obsidian
`id:` or path via UUIDv5 (keeps existing id on re-import; duplicate `id:` →
path re-seed); top-level folder → `category`, remaining parent path → `section`
(e.g. `Literature/Books/X.md` → category `Literature`, section `Books`); front
matter tags/aliases/slug; `* Index.md` → `hub`; resolves `[[wikilinks]]` via
title → slug → aliases; rebuilds `vault.tes`.

**Stale detection (`vault.tes`):** entry count / display paths / file mtimes must
match the index over the full membership set; otherwise list falls back to a
catalog scan.

**Search index (`.tessera/fts`):** Tantivy segments + `tessera_signatures.json`
(path/mtime). Indexed search rebuilds when missing/stale. Small vaults never
create `.tessera/` unless `--index` / `--rebuild` is used.

**Paths:** under the vault root → stored relative; outside → absolute. In-tree
files remain auto-scanned without registration.

**Scale note:** this slice searches projected prose + catalog fields only. Path /
attachment metadata at corpus scale may later use something like **nefaxer**
(or a similar sidecar); that is a follow-up, not part of the first search cut.

---

## `tes serve`

Loopback browser preview via the same semantic HTML export used by
`tes export --html`, styled by an external template pack.

```bash
# From the repo root (ships templates/minimal):
tes serve fixtures/v0/note_one_chunk.tes --theme draft
tes serve paper.tes --theme print --watch
```

| Flag                  | Effect                                                                        |
| --------------------- | ----------------------------------------------------------------------------- |
| `--template ID`       | Pack id under `--template-root` (default: catalog `template_id` or `minimal`) |
| `--template-root DIR` | Pack search root (env: `TES_TEMPLATE_ROOT`, default `templates`)              |
| `--theme ID`          | Pack theme (`draft` / `print` / `manuscript`, or catalog `theme_id`)          |
| `--host`              | Loopback only: `127.0.0.1`, `localhost`, or `::1`                             |
| `--port`              | Bind port (default `7878`; `0` = ephemeral)                                   |
| `--watch`             | Inject HTML meta-refresh (no theme JavaScript)                                |
| `--watch-secs N`      | Refresh interval when `--watch` is set                                        |
| `--allow-theme-js`    | Opt in for packs that declare `requires_theme_js` (still CSS-served)          |

Routes: `/` (standalone HTML), `/theme.css` (selected theme), `/media/{chunk_id}`
(image bytes), `/healthz`. Each request re-opens the `.tes` file. CSP is CSS-only
by default. See [security.md](security.md).

**`minimal` pack themes (v0.1.1):** `draft` (screen), `print` (academic PDF /
print preview), `manuscript` (beta-reader). Deck/slide layouts share the same
CSS files (`.deck` / `.slide` rules). Themes stay CSS-only — no exporter-owned
presentation.

---

## Mutation protocol

```bash
# Editor-neutral virtual buffer protocol.
tes edit-read paper.tes --format tessprek
tes format --stdin < buffer.tessprek
tes format -i buffer.tessprek -o buffer.tessprek
tes format --check --stdin < buffer.tessprek
tes edit-write paper.tes --format tessprek --source-hash HASH --stdin
tes edit-write paper.tes --source-hash HASH -i buffer.tessprek --dry-run

# Agent-safe structural mutation.
tes apply paper.tes --ops ops.json --source-hash HASH --dry-run
tes apply paper.tes --patch change.tessprek --source-hash HASH
```

`edit-read` prints Tessera Markdown (Tessprek) to stdout and the SHA-256
`source-hash=…` on stderr. Tessprek v2 is a hybrid: plain Markdown for
heading/paragraph/list/quote/table/math/fenced-code, plus LaTeX-lite brace
commands for structured chunks. Full grammar: [docs/tessprek.md](tessprek.md).

```text
\tessera{format=tessprek version=2 source-hash=…}
\ids{1,2,3}
\media{
  id=4
  media_type=image/png
  sha256=…
  width=1
  height=1
}

# Title

\figure{
  image=4
  placement=flow
  alt="…"
  caption="…"
}
```

`tes format` normalizes a Tessprek buffer: Markdown-shaped bodies get correct
role / list depth / fence language purely from their Markdown shape (same
inference as `tes import --markdown`), multi-block bodies are split into one
block per chunk, and `\ids{}` is reused positionally when possible. Free
Markdown (no `\text{}`) is accepted. `--check` exits non-zero when the input is
not already normalized. `edit-write` and `apply` acquire an advisory per-file
lock, re-check the source hash, compile to a sibling temporary file,
deep-verify, and atomically replace. `--dry-run` stops before replace and
prints a line diff. Vim/Neovim integrations are thin adapters over these
commands (CLI today; LSP below as it lands).

Library callers can inject **new** image/attachment bytes via
`edit_write_with_media` / `EditWriteOptions.media` (`EditMediaBag`): a new
`\figure{image=… alt=…}` / `media:N` or `\attach{}` chunk's id (from its
position in `\ids{}`) resolves from the bag when absent from the source `.tes`
(THI-233). CLI `edit-write` remains source-copy only for media today.

Typed ops (`--ops`) are a JSON array of closed `TesOp` variants: `set_title`,
`set_aliases`, `set_tags`, `set_slug`, `set_category`, `set_section`, `set_text`,
`append_paragraph`, `delete_chunk`. Catalog fields (`set_aliases` / `set_tags` /
`set_slug` / `set_category` / `set_section`) persist on the `.tes`; refresh
`vault.tes` with `tes vault rebuild` (no auto-rebuild on apply).

### Tessprek LSP (`tes-lsp`)

In-repo language server for Tessprek editors. Same crate as `tes`; stdout is the
LSP wire (log to stderr only). Stack: **tokio + tower-lsp** over stdio.

**Full reference:** [docs/lsp.md](lsp.md) (capabilities, document model, Neovim
snippet, smoke). Hover covers Tessprek header / chunk markers.

```bash
mise run tes-lsp
mise run lsp-smoke          # init / open / change / write / hover
# mise check  → fmt + clippy + test + lsp-smoke
```

Command: `tessera.write` with argument `"file:///…/doc.tes"` (or `{"uri":"…"}`).

---

## History commands (M10)

```bash
tes save paper.tes --draft outline -m "first cut"
tes log paper.tes
tes diff paper.tes rev-a rev-b
tes changelog paper.tes --between rev-a rev-b
tes blame paper.tes
tes blame paper.tes --chunk 2 --json
tes export-revs paper.tes rev-a -o paper-rev-a.tes
tes export-revs paper.tes outline -o draft.tes --keep-history
tes checkout paper.tes rev-a
tes textconv paper.tes
```

`tes save` appends a content-addressed revision (exact-hash payload store +
chunk manifests) into the optional `THST` footer without bumping
`layout_version`. Drafts are named pointers into that revision list.
Structural `tes diff` / `tes changelog` compare chunk ids and hashes, then
show text line diffs for changed text payloads.

`tes blame` walks the parent chain from history `head` (or `--rev`) and
attributes each tip text line (or whole non-text chunk) to the revision that
last introduced that content. Columns: `chunk[:line]`, revision id, timestamp,
source, optional message, text.

`tes export-revs` materializes a revision as a **new** self-contained `.tes`
(body only unless `--keep-history`, which attaches the current footer).
`tes checkout` replaces the live sealed body with the chosen revision and
**re-attaches the full current `THST` footer** (draft/head pointers unchanged).

**Limitation:** revision manifests store catalog + chunk payloads only — not
`TLNK` rows — so materialization does not rewrite the link table yet.

`tes textconv` prints Tessprek on stdout only (no `source-hash=` stderr banner)
for git. Example attributes:

```gitattributes
*.tes diff=tessera merge=tessera
```

```gitconfig
[diff "tessera"]
    textconv = tes textconv
[merge "tessera"]
    name = Tessera verified structural merge
    driver = tes merge-file %O %A %B
```

### Local vs GitHub

| Context                                       | Readable `.tes` diffs?                       |
| --------------------------------------------- | -------------------------------------------- |
| Local `git diff` / `git show`                 | Yes, with `diff=tessera` + `tes textconv`    |
| Local merges                                  | Yes, with `merge=tessera` + `tes merge-file` |
| github.com Files tab / merge UI               | No — blobs stay binary (no textconv hook)    |
| GitHub pull requests                          | Yes — sticky Tessprek comment (Action)       |
| GitHub pushes (`master` / `dev` / any branch) | Yes — Actions job summary + artifact         |

This repo’s `.github/workflows/tes-pr-preview.yml` builds `tes`, runs
`scripts/tes-pr-textconv-diff.sh` on changed `*.tes` files between the event’s
base and head SHAs, then either posts a sticky PR comment or writes the report
to the workflow summary / uploads `tessprek-tes-preview`. Vaults can copy the
template under [`contrib/github/`](../contrib/github/README.md).

`tes merge-file BASE OURS THEIRS` performs a chunk-hash 3-way merge (git
`%O %A %B`): non-overlapping chunk edits auto-merge into `OURS` after deep
verify; overlapping edits exit nonzero and leave `OURS` untouched. `TLNK` is
omitted on rebuild (same limitation as revision materialization). Smoke:

```bash
mise run merge-smoke   # scripts/merge-file-smoke.sh (CLI + temp git driver)
```

## Pending ops (redline)

```bash
tes pending note.tes list
tes pending note.tes suggest --ops edit.json --source-hash "$HASH" -m "try this"
tes pending note.tes redline --source-hash "$HASH"
tes pending note.tes accept --source-hash "$HASH"          # all
tes pending note.tes accept --id pend_… --source-hash "$HASH"
tes pending note.tes reject --id pend_… --source-hash "$HASH"
```

Suggestions land in the THST `pending` slot and do **not** change the sealed
body until `accept`. `redline` is a dry-run Tessprek diff. `reject` drops the
suggestion only. Deep verify runs on accept (and on footer rewrites).

---

## Environment variables

| Variable                | Effect                                                              |
| ----------------------- | ------------------------------------------------------------------- |
| `TES_TEMPLATE_ROOT`     | Default template pack root for `tes serve` / `--pdf`                |
| `TES_CHROME`            | Chromium/Chrome binary for `tes export --pdf`                       |
| `TES_CHROME_NO_SANDBOX` | Force `--no-sandbox` for headless print (also auto on Linux / `CI`) |
| `RUST_LOG`              | `trace`/`debug` for library logging                                 |

---

## CI usage

```yaml
- run: cargo build --release
- run: tes verify fixtures/v0/*.tes --quiet
- run: tes export fixtures/v0/note_one_chunk.tes --raw | diff -u golden.txt -
```

---

## Library parity

Every CLI command maps to a library entry point. The `tes` binary only calls
`tessera_doc::cli::run`; `tes-lsp` only calls `tessera_doc::lsp::run`:

| CLI                                                                                                          | Library                                                                                             |
| ------------------------------------------------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------- |
| `tes info`                                                                                                   | `tessera_doc::catalog::read_summary_v0` / `read_summary_v0_with`                                    |
| `tes verify`                                                                                                 | `tessera_doc::verify::verify_tes_file` / `verify_tes_file_with`                                     |
| `tes repair`                                                                                                 | `tessera_doc::repair::repair_tes_file`                                                              |
| `tes export`                                                                                                 | `tessera_doc::io::export::export_view` (also `--pdf` → `render::pdf`, `--bibliography` → `io::bib`) |
| `tes import`                                                                                                 | `tessera_doc::io::import::*` / `io::bib::import_bibliography`                                       |
| `tes link` / `tes vault`                                                                                     | `tessera_doc::vault::*`                                                                             |
| `tes save` / `log` / `diff` / `changelog` / `blame` / `pending` / `export-revs` / `checkout` / `textconv` / `merge-file` | `tessera_doc::history::*`                                                                           |
| `tes serve`                                                                                                  | `tessera_doc::render::preview::serve_preview`                                                       |
| `tes edit-read`                                                                                              | `tessera_doc::edit::edit_read`                                                                      |
| `tes format`                                                                                                 | `tessera_doc::edit::normalize_tessprek` (`edit::tessprek`)                                            |
| `tes edit-write`                                                                                             | `tessera_doc::edit::edit_write` / `edit_write_with_media`                                            |
| `tes apply`                                                                                                  | `tessera_doc::edit::apply_ops` / `apply_patch`                                                      |
| `tes-lsp`                                                                                                    | `tessera_doc::lsp::run`                                                                             |

Embedders use the library directly; CLI / LSP binaries are thin wrappers.
