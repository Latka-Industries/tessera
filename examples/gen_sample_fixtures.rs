//! Regenerate browse `.tes` samples under `fixtures/samples/`.
//!
//! ```bash
//! cargo run --example gen_sample_fixtures
//! ```
//!
//! These are not byte-golden; CI does not assert their contents.

use std::path::PathBuf;

fn main() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/samples");
    tessera_doc::fixtures::samples::write_all(&dir).expect("write sample fixtures");
    println!("wrote samples under {}", dir.display());
}
