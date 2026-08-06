//! Import and export surfaces for Tessera documents (`docs/exports.md`, `docs/engine.md`).
//!
//! - [`import`] — compile `CommonMark` / semantic HTML into `.tes` chunks.
//! - [`export`] — decode sealed files into views (`--raw`, `--ai-text`, …).
//! - [`bib`] — BibTeX / CSL-JSON bibliography interchange (never canonical cite wire).
//! - [`cite`] — cite-key index + inline citation projection helpers.
//! - [`face`] — pending `\face{id}{text}` helpers (D23 / THI-356).

pub mod bib;
pub mod cite;
pub mod export;
pub mod face;
pub mod import;
