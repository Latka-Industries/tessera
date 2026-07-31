//! Fuzz target for `verify_bytes` (THI-296).
//!
//! ```bash
//! cargo fuzz run verify_bytes
//! ```

#![no_main]

use std::path::Path;

use libfuzzer_sys::fuzz_target;
use tessera_doc::verify::verify_bytes;

fuzz_target!(|data: &[u8]| {
    let _ = verify_bytes(Path::new("fuzz.tes"), data, true);
});
