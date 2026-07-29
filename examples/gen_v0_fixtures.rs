//! Regenerate golden `.tes` fixtures under `fixtures/v0/`.
//!
//! ```bash
//! cargo run --example gen_v0_fixtures
//! cp fixtures/v0/*.tes fixtures/conformance/accept/
//! ```
//!
//! Builders live in [`tessera_doc::fixtures::v0`] so CI goldens stay single-sourced.

use std::path::PathBuf;

fn main() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/v0");
    tessera_doc::fixtures::v0::write_all(&dir).expect("write v0 fixtures");
}
