//! Source-hash helpers for the edit-write optimistic concurrency gate.

use std::fs;
use std::path::Path;

use sha2::{Digest, Sha256};

use crate::error::Result;

/// Hex-encoded SHA-256 of file bytes.
///
/// # Errors
///
/// Returns [`TesError::Io`] if the file cannot be read.
pub fn file_source_hash(path: impl AsRef<Path>) -> Result<String> {
    let bytes = fs::read(path.as_ref())?;
    Ok(hash_bytes(&bytes))
}

/// Hex-encoded SHA-256 of an in-memory buffer.
#[must_use]
pub fn hash_bytes(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    hex_encode(&digest)
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0xf) as usize] as char);
    }
    out
}
