# Tessera

[![Crates.io](https://img.shields.io/crates/v/tessera-doc.svg)](https://crates.io/crates/tessera-doc)
[![docs.rs](https://img.shields.io/docsrs/tessera-doc)](https://docs.rs/tessera-doc)
![Build](https://github.com/Latka-Industries/tessera/workflows/Build/badge.svg)
![Rust](https://img.shields.io/badge/rust-1.95-orange.svg)

**Open document format (`.tes`)** — mmap-friendly chunked binary for notes, wikis, manuscripts, research, and slides. Structure in the file; themes outside it; exports for humans and models.

**In active development — layout v0 wire may change before a stable v1.**

## What it does today

- **Container** — `TESS` superblock, catalog JSON, `TIDX` chunk index, `TLNK` links, sealed writer + mmap reader, golden fixtures, deep `tes verify`
- **Exports** — raw / linear / AI text / JSONL / Markdown / semantic HTML / print PDF (headless Chromium)
- **Import** — CommonMark subset, semantic HTML, BibTeX / CSL-JSON → cite chunks
- **Media & research** — reusable image payloads + figure refs; cite chunks with TLNK mirrors; numeric bibliography rendering
- **Preview** — loopback `tes serve` with external template/theme packs
- **Vault** — resolve / backlinks / broken-link check across a directory of `.tes` files

## Quick start

```bash
cargo build --release
alias tes="$PWD/target/release/tes"

tes info fixtures/v0/note_one_chunk.tes
tes verify --deep fixtures/v0/*.tes
tes export fixtures/v0/note_one_chunk.tes --markdown
tes serve fixtures/v0/note_one_chunk.tes --template-root templates
tes export fixtures/v0/note_one_chunk.tes --pdf -o /tmp/note.pdf --template-root templates
```

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

## Documentation

Spec, decisions, and engine notes live under [`docs/`](docs/) (this README stays short).

| | |
| --- | --- |
| **[Layout v0](docs/layout_v0.md)** | On-disk superblock, catalog, `TIDX`, `TLNK`, chunk payloads |
| **[Structure v1](docs/structure_v1.md)** | Frozen next wire: spans, tables, math, media, templates |
| **[CLI](docs/cli.md)** | `tes` command surface |
| **[Exports](docs/exports.md)** | Decoded views (AI text, Markdown, HTML, PDF, bibliography) |
| **[Engine](docs/engine.md)** | Reader/writer paths and module map |
| **[MIME + magic](docs/mime.md)** | `.tes` MIME, `file(1)` magic, conformance kit |
| **[Benchmarks](docs/benchmarks.md)** | Claim-backed Criterion harness (`cargo bench`) |
| **[Roadmap](docs/roadmap.md)** | Milestones (M0–M10) |
| **[Decisions](docs/decisions.md)** | Accepted design calls |
| **[Security](docs/security.md)** | Threat model for serve / themes |
| **[Glossary](docs/glossary.md)** | Terms |
| **[Format comparison](docs/format-comparison.md)** | vs Markdown / DOCX / PDF |

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE).

Specification prose under [`docs/`](docs/) is [CC BY 4.0](docs/LICENSE).