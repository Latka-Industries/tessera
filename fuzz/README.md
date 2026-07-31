# Fuzzing Tessera

In-memory layout checks via `verify_bytes` (no mmap).

## Setup

```bash
cargo install cargo-fuzz
rustup component add rust-src --toolchain nightly
```

`cargo-fuzz` needs **nightly** (AddressSanitizer uses `-Z` flags). Plain
`cargo fuzz …` on stable will fail with `the option Z is only accepted on the
nightly compiler`.

## Run

```bash
mise fuzz              # preferred
# or:
cargo +nightly fuzz run verify_bytes
```

Seeds are under `corpus/verify_bytes/` (`*.tes` from conformance accept + reject).
LibFuzzer mutates more inputs beside them; those hash-named files and
`artifacts/` are gitignored.

```bash
mise fuzz-clean    # drop mutated corpus + crash artifacts; keep *.tes
mise fuzz-reseed   # refresh *.tes seeds from fixtures/conformance/
```

## Build-only smoke

```bash
mise fuzz-build
# or:
cargo +nightly fuzz build verify_bytes
```
