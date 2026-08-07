//! Regenerate browse samples + figure smoke packs.
//!
//! ```bash
//! cargo run --example gen_sample_fixtures
//! ```
//!
//! These are not byte-golden; CI does not assert their contents.

use std::path::PathBuf;

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let samples = root.join("fixtures/samples");
    tessera_doc::fixtures::samples::write_all(&samples).expect("write sample fixtures");
    println!("wrote samples under {}", samples.display());

    let packs = root.join("fixtures/packs");
    tessera_doc::fixtures::packs::write_all(&packs).expect("write pack fixtures");
    println!("wrote packs under {}", packs.display());
}
