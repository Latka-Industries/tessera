//! **tessera** — Rust library for the Tessera open document format (`.tes`).
//!
//! This crate is the reference engine described in `docs/engine.md`. The v0
//! container layer is spec'd in `docs/layout_v0.md`:
//!
//! - [`layout`] — the fixed 64-byte superblock (`TESS`).
//! - [`catalog::index`] — the chunk index (`TIDX`) header and 48-byte rows.
//! - [`wire`] — little-endian primitives and `align8`.
//!
//! Higher layers (catalog JSON, link table, writer session, verify, export,
//! import, vault) land in later milestones.

pub mod catalog;
pub mod error;
pub mod layout;
pub mod wire;

/// Common types for embedders: `use tessera::prelude::*;`.
pub mod prelude {
    pub use crate::catalog::index::{ChunkIndexEntry, ChunkIndexHeader, ChunkType, Codec};
    pub use crate::error::{Result, TesError};
    pub use crate::layout::{DocKind, Region, SuperblockV0};
}
