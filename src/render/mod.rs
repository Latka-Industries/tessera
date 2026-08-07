//! Presentation surfaces: template packs, browser preview, and PDF print.
//!
//! Shared pipeline: `io::export` HTML → [`template`] pack/theme → screen ([`preview`])
//! or Chromium print ([`pdf`]). Native print IR → ariadnes-weave is [`print`]
//! (feature `native-pdf`, THI-290); CLI `--backend native` is THI-294.
//!
//! D23 pack overlays (sparse): [`pack_text`] (typography / aliases / phrases),
//! [`pack_fonts`] (`fonts.toml` → pinned TTFs), and `weave_pack` (`weave.toml`
//! → layout knobs; feature `native-pdf`). Optional master [`pack_master`]
//! (`tessera.toml`, THI-367) consolidates those sections.
//!
//! - [`template`] — external theme/template packs (`docs/structure_v1.md`).
//! - [`preview`] — loopback `tes serve` HTML preview.
//! - [`pdf`] — print-theme HTML → headless Chromium PDF (+ native when enabled).
//! - [`print`] — `.tes` → ariadnes-weave `PrintDocument` (`native-pdf` feature).

pub mod pack_fonts;
mod pack_master;
pub mod pack_text;
pub mod pdf;
pub mod preview;
#[cfg(feature = "native-pdf")]
pub mod print;
pub mod template;
#[cfg(feature = "native-pdf")]
pub mod weave_pack;
