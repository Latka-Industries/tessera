//! Tessera Markdown (Tessprek) encode/decode for virtual editor buffers.
//!
//! Format (v2): hybrid plain Markdown for heading/paragraph/list/quote/table/
//! math/fenced-code, plus LaTeX-lite brace commands for structured chunks
//! (`\figure{}` / `\cite{}` / `\slide{}` / `\attach{}`) and an optional
//! `\block{title=… caption=… class=… …}` directive before a Markdown block when
//! those attrs cannot live in Markdown itself (legacy `\text{…}` still parses).
//! Inline pack fonts use sealed
//! `\font{id}{text}` ([`inline_font`]); pack `\phrase` expands at format/seal
//! (lossy). Brace commands accept the same multiline form as `\tessera{…}`.
//! See `docs/tessprek.md`.
//!
//! `.tes` stays canonical; Tessprek is a lossy projection only.

mod brace;
mod decode;
mod encode;
mod format;
mod inline_cite;
mod inline_font;
mod layout_ops;
pub mod markers;
mod types;
mod util;
mod write;

#[cfg(test)]
mod tests;

pub(crate) use types::parse_media_header;
pub use types::{TessprekDocMeta, TessprekMediaEntry};

pub use decode::decode_tessprek;
pub(crate) use decode::decode_tessprek_with_spans;
pub(crate) use decode::{decode_named_directive, set_chunk_id};
pub use encode::encode_tessprek;

pub use write::encode_content_blocks;

pub(crate) use brace::{
    scan_tessprek_preamble, skip_blank_lines, take_brace_command, take_leading_tessera_header,
};
pub(crate) use util::{parse_attrs, trim_block_body};

pub use format::{normalize_tessprek, tessprek_needs_format};
