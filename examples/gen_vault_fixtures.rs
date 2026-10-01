//! Regenerate the sample vault under `fixtures/local-fixtures/`.
//!
//! ```bash
//! cargo run --example gen_vault_fixtures
//! ```

use std::path::PathBuf;

fn main() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/local-fixtures");
    tessera_doc::fixtures::vault::write_sample(&dir).expect("write vault fixtures");
}
