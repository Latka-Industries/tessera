//! Import and export surfaces for Tessera documents (`docs/exports.md`, `docs/engine.md`).
//!
//! - [`import`] — compile `CommonMark` / semantic HTML into `.tes` chunks.
//! - [`export`] — decode sealed files into views (`--raw`, `--ai-text`, …).
//! - [`bib`] — BibTeX / CSL-JSON bibliography interchange (never canonical cite wire).
//! - [`cite`] — cite-key index + inline citation projection helpers.
//! - [`font`] — Tessprek `\font{id}{text}` extract helpers ([`font::PendingFont`]; D23 / THI-356).

pub mod bib;
pub mod cite;
pub mod export;
pub mod font;
pub mod import;
