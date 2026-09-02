# Tessera

[![Crates.io](https://img.shields.io/crates/v/tessera-doc.svg)](https://crates.io/crates/tessera-doc)
[![docs.rs](https://img.shields.io/docsrs/tessera-doc)](https://docs.rs/tessera-doc)
![Build](https://github.com/Latka-Industries/tessera/workflows/Build/badge.svg)
![Rust](https://img.shields.io/badge/rust-1.95-orange.svg)

**Open document format (`.tes`)** — mmap-friendly chunked binary for notes, wikis, manuscripts, research, and slides. Structure in the file; themes outside it; exports for humans and models.

**In active development — layout v0 wire may change before a stable v1.**
Crate **0.2.12** ships native PDF via `ariadnes-weave` **0.2.14** (alongside Chromium print): per-chunk `align`, live `{heading}` chrome, and `\footnote` / `\endnote` (THI-398 / 409 / 396) on top of the THI-316 long-doc surface (`\toc` / `\lof` / `\lot`, `\columns`, hyphen/widow knobs, PDF outline), resume `\row` / `\icon` / indent (THI-324), and the D23 / THI-349 surface. Weave 0.2.14 keeps headings, `\box` bands, display math, and tables **in** `\columns` (figures still span).

## What it does today

- **Container** — `TESS` superblock, catalog JSON, `TIDX` chunk index, `TLNK` links, sealed writer + mmap / buffered reader, golden fixtures, deep `tes verify` (`--copy`), `cargo-fuzz` on `verify_bytes`
- **Exports** — raw / linear / AI text / JSONL / Markdown / semantic HTML / print PDF (Chromium default; `--backend native` via `ariadnes-weave`); chapter-scoped `--chapter`
- **Import** — CommonMark subset, semantic HTML, BibTeX / CSL-JSON → cite chunks
- **Media & research** — reusable image payloads + figure refs; cite chunks with TLNK mirrors; numeric bibliography rendering; slides
- **Preview** — loopback `tes serve` with external template/theme packs (`draft` / `print` / `manuscript`)
- **Packs (D23)** — sparse overlays or master `tessera.toml`: `weave.toml` → native knobs; typography / aliases / phrases at format/seal; `\font{id}{…}` + `fonts.toml` pins
- **Edit** — Tessprek `edit-read` / `format` / `edit-write` / `apply` (typed `TesOp`s including catalog tags/aliases); stable attach ids across format/insert
- **LSP** — `tes-lsp` pack-aware `\font` / `\phrase` / alias completions + post-write buffer refresh ([docs/lsp.md](docs/lsp.md); Neovim: [contrib/nvim](contrib/nvim/README.md))
- **History** — `THST` revisions, drafts, diff/changelog, blame, pending redline, export-revs/checkout/textconv/merge-file; GitHub Tessprek Action
- **Vault** — resolve / backlinks / broken-link check; `vault.tes` TOC; multi-root members; scan + Tantivy FTS

## Quick start

```bash
# Library-friendly release (panic = unwind)
cargo build --release
# CLI shipping with panic = abort
cargo build --profile release-cli --bins
alias tes="$PWD/target/release/tes"
# or: alias tes="$PWD/target/release-cli/tes"

tes info fixtures/v0/note_one_chunk.tes
tes verify --deep fixtures/conformance/accept/*.tes
tes verify --copy --deep /mnt/nfs/untrusted.tes
tes export fixtures/v0/note_one_chunk.tes --markdown
tes serve fixtures/v0/note_one_chunk.tes --template-root templates
tes export fixtures/v0/note_one_chunk.tes --pdf -o /tmp/note.pdf --template-root templates
tes export fixtures/v0/note_one_chunk.tes --pdf --backend native -o /tmp/note-native.pdf

# Tessprek browse / Neovim: fixtures/samples/tessprek_showcase.tes
# Pack phrases: fixtures/samples/phrases_demo.tessprek
```

Cargo features (library): `native-pdf` (default), plus optional `weave-cjk` /
`weave-emoji` / `weave-icons` pass-throughs to `ariadnes-weave`. Format-only
embeds can use `default-features = false`.

```bash
tes import --markdown notes.md notes.tes
tes import --bibtex fixtures/assets/citations/sample.bib refs.tes
tes export refs.tes --bibliography --bib-format bibtex
tes link --vault ./vault check
```

Measure open-format claims (mmap / import / export / vault) with
[`docs/benchmarks.md`](docs/benchmarks.md) — run `mise run bench` or
`cargo bench -p tessera-doc --bench open_format`. Paste only measured numbers.

See `tes --help` and [docs/cli.md](docs/cli.md).

**Release:** push a `v*` tag that matches `Cargo.toml` (e.g. `v0.2.12`).
`.github/workflows/release.yml` publishes a GitHub Release (not a draft) with
generated notes and `cargo publish --locked` to crates.io. `workflow_dispatch`
from a branch does not publish. Chromium stays the `tes export --pdf` default
until THI-350.

## Documentation

Spec, decisions, and engine notes live under [`docs/`](docs/) as Markdown so
they stay readable on GitHub. A later dogfood step may add a `.tes` mirror of
that tree and generate the `.md` from it — Markdown remains the published view;
binary alone would not.

|                                                    |                                                             |
| -------------------------------------------------- | ----------------------------------------------------------- |
| **[Layout v0](docs/layout_v0.md)**                 | On-disk superblock, catalog, `TIDX`, `TLNK`, chunk payloads |
| **[Structure v1](docs/structure_v1.md)**           | Frozen next wire: spans, tables, math, media, templates     |
| **[CLI](docs/cli.md)**                             | `tes` command surface                                       |
| **[LSP](docs/lsp.md)**                             | `tes-lsp` Tessprek language server                          |
| **[Exports](docs/exports.md)**                     | Decoded views (AI text, Markdown, HTML, PDF, bibliography)  |
| **[Engine](docs/engine.md)**                       | Reader/writer paths and module map                          |
| **[MIME + magic](docs/mime.md)**                   | `.tes` MIME, `file(1)` magic, conformance kit               |
| **[Benchmarks](docs/benchmarks.md)**               | Claim-backed Criterion harness (`cargo bench`)              |
| **[Print IR](docs/print_ir.md)**                   | Native PDF tree + `ariadnes-weave` (D21)                |
| **[Roadmap](docs/roadmap.md)**                     | Milestones (M0–M11)                                         |
| **[Decisions](docs/decisions.md)**                 | Accepted design calls                                       |
| **[Security](docs/security.md)**                   | Threat model for serve / themes                             |
| **[Glossary](docs/glossary.md)**                   | Terms                                                       |
| **[Format comparison](docs/format-comparison.md)** | vs Markdown / DOCX / PDF                                    |

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE).

Specification prose under [`docs/`](docs/) is [CC BY 4.0](docs/LICENSE).
