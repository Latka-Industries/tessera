//! `tes` — command-line interface for the Tessera document format.
//!
//! Subcommands (`info`, `verify`, `export`, …) land in later milestones; for
//! now this prints build/layout metadata so the binary target compiles and
//! links against the library.

fn main() {
    println!(
        "tes {} — Tessera layout v{}",
        env!("CARGO_PKG_VERSION"),
        tessera::layout::LAYOUT_VERSION
    );
}
