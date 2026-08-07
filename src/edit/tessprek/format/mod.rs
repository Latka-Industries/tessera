//! Normalize Tessprek buffers so Markdown-shaped bodies imply correct directives.
//!
//! Reuses [`crate::io::import::parse_markdown_blocks`] for role / list depth /
//! fence language inference (same rules as `tes import --markdown`).
//!
//! v2 has no per-block ids: `\ids{…}` is a flat, positional, reading-order list
//! under the header. [`build_content_blocks`] scans the body into free Markdown
//! runs (optionally preceded by `\block{…}`) and brace-command directives
//! (`\figure{}` / `\cite{}` / `\slide{}` / `\attach{}`), producing blocks with
//! `chunk_id: None`; callers assign ids afterward ([`decode_tessprek`] strictly
//! from `\ids{}`, [`normalize_tessprek`] via [`IdAllocator`]).
//!
//! [`decode_tessprek`]: super::decode_tessprek

mod build;
mod markdown;
mod normalize;
mod scan;

#[cfg(test)]
mod tests;

pub use normalize::{normalize_tessprek, tessprek_needs_format};

pub(crate) use build::build_content_blocks_with_spans;

#[cfg(test)]
pub(crate) use normalize::normalize_newlines;

pub(super) use super::util::parse_err;
