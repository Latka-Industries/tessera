//! Error types for the Tessera container layer.

use thiserror::Error;

/// Result alias used throughout the crate.
pub type Result<T> = std::result::Result<T, TesError>;

/// Errors produced while encoding or decoding `.tes` container structures.
#[derive(Debug, Error)]
pub enum TesError {
    /// A fixed-size structure was decoded from a buffer that was too small.
    #[error("buffer too small for {structure}: need {need} bytes, got {got}")]
    BufferTooSmall {
        /// Name of the structure being decoded.
        structure: &'static str,
        /// Bytes required to decode the structure.
        need: usize,
        /// Bytes actually available.
        got: usize,
    },

    /// A magic tag did not match the expected ASCII bytes.
    #[error("bad magic for {structure}: expected {expected:?}, found {found:?}")]
    BadMagic {
        /// Name of the structure whose magic was checked.
        structure: &'static str,
        /// Expected 4-byte ASCII tag.
        expected: [u8; 4],
        /// Bytes found in the buffer.
        found: [u8; 4],
    },

    /// An on-disk version field is newer than this build understands.
    #[error("unsupported {structure} version {found}: this build supports {supported}")]
    UnsupportedVersion {
        /// Name of the versioned structure.
        structure: &'static str,
        /// Version read from the buffer.
        found: u32,
        /// Highest version this build can decode.
        supported: u32,
    },

    /// An enum discriminant read from disk is outside the known range.
    #[error("invalid {field} value {value}")]
    InvalidEnum {
        /// Name of the field carrying the discriminant.
        field: &'static str,
        /// Raw value read from the buffer.
        value: u32,
    },

    /// Document catalog exceeds the v0 reference writer limit (16 KiB).
    #[error("document catalog too large: {len} bytes (limit {limit})")]
    CatalogTooLarge {
        /// Serialized catalog size.
        len: usize,
        /// Maximum allowed size.
        limit: usize,
    },

    /// Text chunk semantic header exceeds the v0 limit (4 KiB).
    #[error("text chunk header too large: {len} bytes (limit {limit})")]
    TextHeaderTooLarge {
        /// Serialized header size.
        len: usize,
        /// Maximum allowed size.
        limit: usize,
    },

    /// The writer session was already sealed (committed).
    #[error("writer session already sealed")]
    SessionSealed,

    /// A text payload body was not valid UTF-8.
    #[error("invalid UTF-8 in {structure}")]
    InvalidUtf8 {
        /// Name of the structure containing the bad bytes.
        structure: &'static str,
    },

    /// Underlying filesystem I/O failed.
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// JSON encode/decode failed.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}
