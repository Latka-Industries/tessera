//! Regenerate the sample vault under `fixtures/vault/`.
//!
//! ```bash
//! cargo run --example gen_vault_fixtures
//! ```

use std::path::PathBuf;

fn main() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/vault");
    tessera_doc::fixtures::vault::write_sample(&dir).expect("write vault fixtures");
}
