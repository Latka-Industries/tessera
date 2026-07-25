//! Catalog layer: the document model stored inside a single `.tes` file.

pub mod chunk;
pub mod document;
pub mod file;
pub mod index;
pub mod info;
pub mod session;

pub use chunk::{ListKind, TextHeader, TextRole, decode_text_payload, encode_text_payload};
pub use document::DocumentCatalog;
pub use file::TesFile;
pub use index::{ChunkIndexEntry, ChunkIndexHeader, ChunkType, Codec};
pub use info::{
    TesInfoReport, format_info_human, format_info_json, format_info_quiet, read_summary_v0,
};
pub use session::TesWriterSession;
