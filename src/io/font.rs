//! Pack-pinned font helpers under [`crate::io`] (D23 / THI-356).
//!
//! [`PendingFont`] is the pre-seal form of Tessprek `\font{id}{text}` after
//! extraction (bare inner text + span). Compile seals these to
//! [`crate::catalog::InlineKind::Font`].

/// Tessprek `\font{id}{text}` after extraction, before seal to [`InlineKind::Font`](crate::catalog::InlineKind::Font).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingFont {
    /// Inclusive start byte offset in the rewritten body (over the inner text).
    pub start: u32,
    /// Exclusive end byte offset in the rewritten body.
    pub end: u32,
    /// Pack font id (`armenian`, `test`, …); must exist in pack `fonts.toml` for native PDF.
    pub font_id: Option<String>,
    /// Point size × 1000 (`12000` = 12pt) from Tessprek `\size{N}{…}`.
    pub size_milli_pt: Option<u32>,
}
