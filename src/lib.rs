//! **tessera-doc** — Rust library for the Tessera open document format (`.tes`).
//!
//! This crate is the reference engine described in `docs/engine.md`. The v0
//! container layer is spec'd in `docs/layout_v0.md`.
//!
//! ## Module map
//!
//! - [`layout`] — fixed 64-byte superblock (`TESS`), mmap / copy open.
//! - [`catalog`] — document model: index, session writer, payloads, `THST` wire.
//! - [`verify`] — layout health findings for `tes verify`.
//! - [`repair`] — salvage damaged containers for `tes repair` (complements verify).
//! - [`io`] — import / export / bibliography interchange.
//! - [`edit`] — Tessera Markdown virtual editing (`edit-read` / `edit-write` / `apply`).
//! - [`history`] — content-addressed drafts (`tes save` / `log` / `diff` / `changelog`).
//! - [`vault`] — stable link resolution, backlinks, and FTS search.
//! - [`render`] — template packs, `tes serve` preview, and PDF print.
//! - [`cli`] — `tes` command surface (`src/bin/tes.rs` → [`cli::run`]).
//! - [`lsp`] — `tes-lsp` Tessprek language server (`src/bin/tes_lsp.rs` → [`lsp::run`]).
//! - [`error`] — shared [`error::TesError`] / [`error::Result`].
//!
//! Crate-root aliases re-export [`io`] and [`render`] children for short paths
//! (`tessera_doc::export`, `tessera_doc::preview`, …).
//!
//! Wire helpers (`LeReader` / `align8` / codecs) come from
//! [`argus`](https://crates.io/crates/argus-chunk).

#![deny(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
#![deny(clippy::undocumented_unsafe_blocks)]

pub mod catalog;
pub mod cli;
pub mod edit;
pub mod error;
pub mod fixtures;
pub mod history;
pub mod io;
pub mod layout;
pub mod lsp;
pub mod render;
pub mod repair;
pub mod vault;
pub mod verify;

/// Crate-root aliases for [`io`] and [`render`] submodules.
pub use io::{bib, export, import};
pub use render::{pdf, preview, template};

#[cfg(test)]
mod tests;

/// Common types for embedders: `use tessera_doc::prelude::*;`.
pub mod prelude {
    pub use crate::catalog::{
        AttachmentPayload, ChunkIndexEntry, ChunkIndexHeader, ChunkType, Codec, DocumentCatalog,
        FigureRef, ImagePayload, ImagePlacement, SlidePayload, SlideRegion, TesFile, TesInfoReport,
        TesWriterSession, TextAlign, TextHeader, TextRole, read_summary_v0,
    };
    pub use crate::edit::{
        EditMediaBag, EditReadReport, EditWriteOptions, EditWriteReport, TesOp, apply_ops,
        apply_patch, edit_read, edit_write, edit_write_with_media, file_source_hash,
        normalize_tessprek,
    };
    pub use crate::error::{Result, TesError};
    pub use crate::history::{
        BlameOptions, BlameRegion, BlameReport, DiffEntry, DiffReport, PendingActionOptions,
        PendingSuggestion, SaveOptions, SaveReport, SuggestOptions, accept_pending, blame_file,
        checkout_revision, diff_revisions, export_revision, format_blame, format_blame_json,
        format_changelog, format_diff, format_log, format_pending, list_pending,
        materialize_revision, merge_files, pending_redline, read_history, reject_pending,
        save_revision, suggest_pending, textconv,
    };
    pub use crate::io::bib::{
        BibEntry, BibFormat, BibImportOptions, export_bibliography, import_bibliography,
        parse_bibtex,
    };
    pub use crate::io::export::{AiPart, ExportOptions, ExportView, export_ai_parts, export_view};
    pub use crate::io::import::{
        HtmlImportOptions, HtmlImportReport, MarkdownFrontMatter, MarkdownImportOptions,
        MarkdownImportReport, WikilinkResolver, WikilinkSpan, collect_unresolved_wikilinks,
        import_html_v0, import_markdown_v0, parse_front_matter, resolve_import_doc_id,
        rewrite_wikilinks, visit_wikilinks,
    };
    pub use crate::layout::{DocKind, OpenMode, Region, SuperblockV0};
    pub use crate::render::pdf::{PdfExportOptions, export_pdf, render_themed_html};
    pub use crate::render::preview::{ServeOptions, preview_html_for_path, serve_preview};
    pub use crate::render::template::{TemplateManifest, TemplatePack};
    pub use crate::vault::{
        Backlink, ResolvedTarget, Vault, VaultDocument, VaultIndex, VaultIndexEntry,
        VaultMarkdownImportOptions, VaultMarkdownImportReport, VaultMember, VaultMemberKind,
        import_markdown_vault, list_vault_documents, rebuild_vault_index, register_member,
        unregister_member,
    };
    pub use crate::verify::{TesVerifyReport, verify_tes_file, verify_tes_file_with};
}
