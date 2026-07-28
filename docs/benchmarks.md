# Claim-backed benchmarks

Focused Criterion harnesses that back open-format performance claims
([structure_v1 — conformance](structure_v1.md#forward-compatibility-and-conformance),
[roadmap](roadmap.md)).

## Run

```bash
cargo bench -p tessera-doc --bench open_format
# or
mise run bench
```

HTML reports land under `target/criterion/` (open `report/index.html` in a
browser). PDF export benches run only when a Chromium-family browser is
detectable via [`find_chrome`](../src/render/pdf.rs).

## Axes (`benches/open_format.rs`)

| Group | What it measures |
| --- | --- |
| `mmap_partial_chunk` | Open + decode chunk 1 on `note_one_chunk` and imported `lorem_long` |
| `import_markdown` | MD → `.tes` for `minimal` and ~900 KiB `lorem_long` |
| `export` | raw (small), linear + HTML on long fixture; optional PDF on small |
| `vault` | backlinks (8 hubs); Markdown vault full-file reads vs `.tes` vault open/list |

## Publishing numbers

Do **not** invent timing claims in the README. Prefer:

1. Point readers here / at `cargo bench --bench open_format`.
2. Optionally paste a dated local or CI Criterion summary after a measured run.
3. Optional workflow: `.github/workflows/bench.yml` (`workflow_dispatch` / schedule).

CI unit tests never require Criterion results; benches are intentionally offline
by default so PR latency stays low.
