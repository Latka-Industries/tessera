//! Pack-pinned font helpers under [`crate::io`] (D23 / THI-356).

/// Pending `\font{id}{text}` discovered in Tessprek body text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingFont {
    /// Inclusive start byte offset in the rewritten body (over the inner text).
    pub start: u32,
    /// Exclusive end byte offset in the rewritten body.
    pub end: u32,
    /// Pack font id (`armenian`, `test`, …).
    pub font_id: String,
}
