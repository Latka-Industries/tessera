//! Decoded export views under [`crate::io`] (`docs/exports.md`).
//!
//! Exports are **projections** of a sealed `.tes` file — never the canonical
//! source. Models and pipelines should call these views rather than hex-dumping
//! the wire format.

mod ai;
mod common;
mod html;
mod jsonl;
mod linear;
mod markdown;
mod raw;

#[cfg(test)]
mod tests;

use std::path::Path;

use crate::catalog::file::TesFile;
use crate::error::Result;

pub use ai::{AiPart, export_ai_parts};
pub(crate) use common::chapter_slice;
pub use common::export_attachment_bytes;

/// Which decoded view to emit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportView {
    /// Concatenate text chunk bodies (`--raw`).
    Raw,
    /// Reading-order prose with light structure markers (`--linear`).
    Linear,
    /// LLM-oriented plain text, no exporter-introduced markup (`--ai-text`).
    AiText,
    /// One JSON object per reading-order chunk (`--chunks-jsonl`).
    ChunksJsonl,
    /// Lossy GFM-ish Markdown projection (`--markdown`).
    Markdown,
    /// Semantic HTML5 fragment or standalone document (`--html`).
    Html,
}

/// Options that refine an [`ExportView`].
#[derive(Debug, Clone, Default)]
#[allow(clippy::struct_excessive_bools)]
pub struct ExportOptions {
    /// Restrict output to a single chunk id (where applicable).
    pub chunk_id: Option<u64>,
    /// Restrict output to the Nth chapter (1-based), bounded by level-1 headings.
    ///
    /// Mutually exclusive with [`Self::chunk_id`]. See manuscript conventions in
    /// `docs/decisions.md`.
    pub chapter: Option<u32>,
    /// Prefix each `--raw` chunk with a debug header line.
    pub include_headers: bool,
    /// Prefix each `--ai-text` chunk with `<!-- chunk:N -->`.
    pub annotate: bool,
    /// Include non-reading-order / non-text rows in `--chunks-jsonl`.
    pub all_types: bool,
    /// Omit cite chunk expansion from `--ai-text`.
    pub no_cites: bool,
    /// Stylesheet href for HTML export.
    pub theme_href: Option<String>,
    /// Wrap HTML output in a complete document.
    pub standalone: bool,
    /// CSS embedded in a `<style>` element.
    pub embedded_css: Option<String>,
    /// When set, figure `<img src>` uses `{prefix}{image_chunk_id}` instead of data URIs.
    pub media_url_prefix: Option<String>,
    /// When set, attachment download links use `{prefix}{attachment_chunk_id}`.
    ///
    /// Attachments are never inlined as data URIs.
    pub attachment_url_prefix: Option<String>,
}

/// Export `path` as the selected view.
///
/// # Errors
///
/// Returns open/parse errors from [`TesFile::open`], or view-specific errors from
/// [`export_file`].
pub fn export_view(
    path: impl AsRef<Path>,
    view: ExportView,
    options: &ExportOptions,
) -> Result<String> {
    let file = TesFile::open(path.as_ref())?;
    export_file(&file, view, options)
}

/// Export an already-open file.
///
/// # Errors
///
/// Returns [`TesError::ChunkNotFound`] if a requested chunk is missing,
/// [`TesError::Decode`] / [`TesError::InvalidFigure`] when a payload cannot be decoded,
/// or other payload errors from [`TesFile::decode_payload`].
pub fn export_file(file: &TesFile, view: ExportView, options: &ExportOptions) -> Result<String> {
    match view {
        ExportView::Raw => raw::export_raw(file, options),
        ExportView::Linear => linear::export_linear(file, options),
        ExportView::AiText => ai::export_ai_text(file, options),
        ExportView::ChunksJsonl => jsonl::export_chunks_jsonl(file, options),
        ExportView::Markdown => markdown::export_markdown(file, options),
        ExportView::Html => html::export_html(file, options),
    }
}
