//! Presentation surfaces: template packs, browser preview, and PDF print.
//!
//! Shared pipeline: `io::export` HTML → [`template`] pack/theme → screen ([`preview`])
//! or Chromium print ([`pdf`]). Native print IR → ariadnes-weave is [`print`]
//! (THI-290); CLI `--backend native` is THI-294.
//!
//! - [`template`] — external theme/template packs (`docs/structure_v1.md`).
//! - [`preview`] — loopback `tes serve` HTML preview.
//! - [`pdf`] — print-theme HTML → headless Chromium PDF.
//! - [`print`] — `.tes` → ariadnes-weave [`PrintDocument`](ariadnes_weave::PrintDocument).

pub mod pdf;
pub mod preview;
pub mod print;
pub mod template;
