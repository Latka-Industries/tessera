//! Pack-pinned face helpers under [`crate::io`] (D23 / THI-356).

/// Pending `\face{id}{text}` discovered in Tessprek body text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingFace {
    /// Inclusive start byte offset in the rewritten body (over the inner text).
    pub start: u32,
    /// Exclusive end byte offset in the rewritten body.
    pub end: u32,
    /// Pack face id (`armenian`, `test`, …).
    pub face_id: String,
}
