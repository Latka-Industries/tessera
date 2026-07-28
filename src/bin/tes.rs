//! `tes` — command-line interface for the Tessera document format.
//!
//! Thin entry: all clap parsing and command runners live in [`tessera_doc::cli`].

fn main() -> std::process::ExitCode {
    tessera_doc::cli::run()
}
