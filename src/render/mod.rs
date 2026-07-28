//! Presentation surfaces: template packs, browser preview, and PDF print.
//!
//! Shared pipeline: `io::export` HTML → [`template`] pack/theme → screen ([`preview`])
//! or Chromium print ([`pdf`]).
//!
//! - [`template`] — external theme/template packs (`docs/structure_v1.md`).
//! - [`preview`] — loopback `tes serve` HTML preview.
//! - [`pdf`] — print-theme HTML → headless Chromium PDF.

pub mod pdf;
pub mod preview;
pub mod template;
