//! Catalog layer: the document model stored inside a single `.tes` file.

pub mod chunk;
pub mod document;
pub mod file;
pub mod history;
pub mod index;
pub mod info;
pub mod link;
pub mod media;
pub mod session;
pub mod slide;

pub use chunk::{
    CitePayload, ListKind, TextHeader, TextRole, decode_text_payload, encode_text_payload,
};
pub use document::DocumentCatalog;
pub use file::TesFile;
pub use history::{
    ChunkManifest, HISTORY_VERSION, HistoryV1, Revision, THST_MAGIC, attach_footer, content_hash,
    decode_footer, encode_footer, footer_suffix_len, revision_id, split_body_and_history,
    usable_file_len,
};
pub use index::{ChunkIndexEntry, ChunkIndexHeader, ChunkType, Codec};
pub use info::{
    TesInfoReport, format_info_human, format_info_json, format_info_quiet, read_summary_v0,
};
pub use link::{LinkEntry, LinkKind};
pub use media::{FigureRef, ImagePayload, ImagePlacement, base64_decode, base64_encode};
pub use session::TesWriterSession;
pub use slide::{SlidePayload, SlideRegion};
