//! **tessera-doc** — Rust library for the Tessera open document format (`.tes`).
//!
//! This crate is the reference engine described in `docs/engine.md`. The v0
//! container layer is spec'd in `docs/layout_v0.md`:
//!
//! - [`layout`] — the fixed 64-byte superblock (`TESS`) and mmap open.
//! - [`catalog::index`] — the chunk index (`TIDX`) header and 48-byte rows.
//! - [`catalog::session`] — [`TesWriterSession`] sealed-file writer.
//! - [`catalog::file`] — [`TesFile`] mmap reader + catalog/index parse.
//! - [`verify`] — layout health findings for `tes verify`.
//! - [`export`] — decoded views (`--raw`, `--ai-text`, …).
//! - [`import`] — `CommonMark` and semantic HTML compilation into chunks.
//! - [`edit`] — Tessera Markdown virtual editing (`edit-read` / `edit-write` / `apply`).
//! - [`history`] — content-addressed drafts, `tes save` / `log` / `diff` / `changelog`.
//! - [`vault`] — stable link resolution and backlinks.
//! - [`template`] — external theme/template packs.
//! - [`preview`] — loopback `tes serve` HTML preview.
//! - [`pdf`] — print-theme HTML → headless PDF.
//! - [`bib`] — BibTeX / CSL-JSON bibliography interchange.
//!
//! Wire helpers (`LeReader` / `align8` / codecs) come from
//! [`argus`](https://crates.io/crates/argus-chunk).

pub mod bib;
pub mod catalog;
pub mod edit;
pub mod error;
pub mod export;
pub mod history;
pub mod import;
pub mod layout;
pub mod pdf;
pub mod preview;
pub mod template;
pub mod vault;
pub mod verify;

#[cfg(test)]
mod tests;

/// Common types for embedders: `use tessera_doc::prelude::*;`.
pub mod prelude {
    pub use crate::bib::{
        BibEntry, BibFormat, BibImportOptions, export_bibliography, import_bibliography,
        parse_bibtex,
    };
    pub use crate::catalog::{
        ChunkIndexEntry, ChunkIndexHeader, ChunkType, Codec, DocumentCatalog, FigureRef,
        ImagePayload, ImagePlacement, SlidePayload, SlideRegion, TesFile, TesInfoReport,
        TesWriterSession, TextHeader, TextRole, read_summary_v0,
    };
    pub use crate::edit::{
        EditReadReport, EditWriteOptions, EditWriteReport, TesOp, apply_ops, apply_patch,
        edit_read, edit_write, file_source_hash,
    };
    pub use crate::error::{Result, TesError};
    pub use crate::export::{AiPart, ExportOptions, ExportView, export_ai_parts, export_view};
    pub use crate::history::{
        DiffEntry, DiffReport, SaveOptions, SaveReport, diff_revisions, format_changelog,
        format_diff, format_log, read_history, save_revision,
    };
    pub use crate::import::{
        HtmlImportOptions, HtmlImportReport, MarkdownImportOptions, MarkdownImportReport,
        import_html_v0, import_markdown_v0,
    };
    pub use crate::layout::{DocKind, Region, SuperblockV0};
    pub use crate::pdf::{PdfExportOptions, export_pdf, render_themed_html};
    pub use crate::preview::{ServeOptions, preview_html_for_path, serve_preview};
    pub use crate::template::{TemplateManifest, TemplatePack};
    pub use crate::vault::{Backlink, ResolvedTarget, Vault, VaultDocument};
    pub use crate::verify::{TesVerifyReport, verify_tes_file};
}
