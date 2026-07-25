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

    /// A region extends past the end of the file (or payload past EOF).
    #[error("{structure} out of bounds: offset {offset} + length {length} > file_len {file_len}")]
    OutOfBounds {
        /// Name of the region or payload.
        structure: &'static str,
        /// Start offset.
        offset: u64,
        /// Requested length.
        length: u64,
        /// Mapped file length.
        file_len: u64,
    },

    /// Chunk index region length does not match `32 + entry_count × 48`.
    #[error("chunk index length mismatch: expected {expected} bytes, region is {got}")]
    IndexLengthMismatch {
        /// Size implied by the `TIDX` header.
        expected: u64,
        /// Size from the superblock region.
        got: u64,
    },

    /// Link table region length does not match `24 + entry_count × 48`.
    #[error("link table length mismatch: expected {expected} bytes, region is {got}")]
    LinkTableLengthMismatch {
        /// Size implied by the `TLNK` header.
        expected: u64,
        /// Size from the superblock region.
        got: u64,
    },

    /// Payload codec decode failed or length mismatched `raw_byte_len`.
    #[error("decode failed for chunk {chunk_id}: {message}")]
    Decode {
        /// Chunk that failed to decode.
        chunk_id: u64,
        /// Human-readable reason.
        message: String,
    },

    /// Requested chunk id was not found in the index.
    #[error("chunk {chunk_id} not found")]
    ChunkNotFound {
        /// Requested chunk id.
        chunk_id: u64,
    },

    /// Export was called without selecting a view.
    #[error(
        "export requires a view flag (--raw, --linear, --ai-text, --chunks-jsonl, --markdown, or --html)"
    )]
    ExportViewRequired,

    /// A supplied catalog document id was not a UUID.
    #[error("invalid document UUID '{value}'")]
    InvalidDocId {
        /// User-supplied value.
        value: String,
    },

    /// Two files in one vault declared the same document UUID.
    #[error("duplicate document UUID {doc_id}: {first} and {second}")]
    DuplicateDocId {
        /// Duplicated UUID.
        doc_id: String,
        /// First path encountered.
        first: String,
        /// Conflicting path.
        second: String,
    },

    /// No document with this UUID exists in the vault.
    #[error("document {doc_id} not found in vault")]
    DocumentNotFound {
        /// Requested UUID.
        doc_id: String,
    },

    /// Underlying filesystem I/O failed.
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// JSON encode/decode failed.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}
