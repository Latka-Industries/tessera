# `tes` CLI reference

**Status:** design reference for the **`tes`** binary. Flags may change until v0.1; **`tes -h`** is authoritative after implementation.

Related: [layout_v0.md](layout_v0.md), [exports.md](exports.md), [engine.md](engine.md), [roadmap.md](roadmap.md).

---

## Command summary

| Command | Alias | Role |
| --- | --- | --- |
| [`tes info`](#tes-info) `<path.tes>` | — | Summarize document (catalog + chunk table) |
| [`tes verify`](#tes-verify) `<path.tes>` | — | Layout health check (exit 1 on failure) |
| [`tes export`](#tes-export) `<path.tes>` | — | Decoded views ([exports.md](exports.md)) |
| [`tes import`](#tes-import) `<in> <out.tes>` | — | Foreign format → `.tes` (staged rollout) |
| [`tes link`](#tes-link) | — | Resolve / inspect links (vault-aware, later) |

v0 ships **`info`**, **`verify`**, **`export`** first.

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
```

| Flag | Effect |
| --- | --- |
| `--raw` | UTF-8 text bodies ([exports — raw](exports.md#--raw)) |
| `--linear` | Reading-order text with light headings |
| `--ai-text` | LLM-oriented plain text |
| `--chunks-jsonl` | One JSON object per chunk line |
| `--markdown` | Lossy Markdown |
| `--html` | HTML fragment (+ `--theme`, `--standalone`) |
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
tes import --pdf scan.pdf scan.tes --page-rasters
```

| Flag | Effect |
| --- | --- |
| `--markdown` | CommonMark subset ([decisions](decisions.md#markdown-import--export)) |
| `--html` | Semantic block import |
| `--pdf` | Text extract + optional `--page-rasters` |
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

**Phase:** Phase 5 ([roadmap](roadmap.md)). Stub returns exit 2 in v0.

---

## Environment variables

| Variable | Effect |
| --- | --- |
| `TES_VAULT` | Default vault directory for `tes link` |
| `TES_THEME` | Default CSS path for `--html` export |
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

Every CLI command maps to a library entry point:

| CLI | Library (planned) |
| --- | --- |
| `tes info` | `tessera::catalog::read_summary_v0` |
| `tes verify` | `tessera::verify::verify_tes_file` |
| `tes export` | `tessera::export::export_view` |
| `tes import` | `tessera::import::import_markdown_v0` |

Embedders use the library directly; CLI is a thin wrapper.
