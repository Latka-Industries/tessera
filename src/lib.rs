//! **tessera** — Rust library for the Tessera open document format (`.tes`).
//!
//! This crate is the reference engine described in `docs/engine.md`. The v0
//! container layer is spec'd in `docs/layout_v0.md`:
//!
//! - [`layout`] — the fixed 64-byte superblock (`TESS`) and mmap open.
//! - [`catalog::index`] — the chunk index (`TIDX`) header and 48-byte rows.
//! - [`catalog::session`] — [`TesWriterSession`] sealed-file writer.
//! - [`catalog::file`] — [`TesFile`] mmap reader + catalog/index parse.
//! - [`verify`] — layout health findings for `tes verify`.
//! - [`wire`] — little-endian primitives and `align8`.
//!
//! Higher layers (link table, export, import, vault) land in later milestones.

pub mod catalog;
pub mod error;
pub mod layout;
pub mod verify;
pub mod wire;

#[cfg(test)]
mod tests;

/// Common types for embedders: `use tessera::prelude::*;`.
pub mod prelude {
    pub use crate::catalog::{
        ChunkIndexEntry, ChunkIndexHeader, ChunkType, Codec, DocumentCatalog, TesFile,
        TesInfoReport, TesWriterSession, TextHeader, TextRole, read_summary_v0,
    };
    pub use crate::error::{Result, TesError};
    pub use crate::layout::{DocKind, Region, SuperblockV0};
    pub use crate::verify::{TesVerifyReport, verify_tes_file};
}
