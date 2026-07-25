//! Catalog layer: the document model stored inside a single `.tes` file.

pub mod chunk;
pub mod document;
pub mod index;
pub mod session;

pub use chunk::{ListKind, TextHeader, TextRole, decode_text_payload, encode_text_payload};
pub use document::DocumentCatalog;
pub use index::{ChunkIndexEntry, ChunkIndexHeader, ChunkType, Codec};
pub use session::TesWriterSession;
