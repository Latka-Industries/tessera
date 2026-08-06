//! Virtual editor mutation protocol (`edit-read` / `edit-write` / `apply`).
//!
//! Safe mutation gate from `docs/security.md`:
//! advisory lock → source-hash recheck → sibling temp compile → deep verify →
//! atomic replace. Pack typography / aliases / phrases expand at format/write
//! when a template pack is resolvable (D23).

mod block;
mod compile;
mod diff;
mod gate;
mod hash;
mod lock;
mod media;
mod ops;
mod report;
pub mod tessprek;

#[cfg(test)]
mod tests;

pub use block::ContentBlock;
pub use gate::{apply_ops, apply_patch, edit_read, edit_write, edit_write_with_media};
pub use hash::{file_source_hash, hash_bytes};
pub use media::EditMediaBag;
pub use ops::{CatalogPatch, TesOp, apply_ops_to_blocks, parse_ops_json};
pub use report::{EditReadReport, EditWriteOptions, EditWriteReport};
pub use tessprek::markers;
pub use tessprek::{
    TessprekDocMeta, TessprekMediaEntry, decode_tessprek, encode_content_blocks, encode_tessprek,
    normalize_tessprek, tessprek_needs_format,
};
