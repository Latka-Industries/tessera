//! Chunk payload codecs (`docs/layout_v0.md` — *Text chunk payload*).
//!
//! Layout-v1 text structure (spans, math, structured tables, language) is stored
//! as additive JSON fields on the text header — `layout_version` stays 0 until a
//! full container bump.

mod cite;
mod codec;
mod inline;
mod table;
mod text;

#[cfg(test)]
mod tests;

pub use cite::CitePayload;
pub use codec::{
    decode_text_payload, encode_text_payload, encode_u32_prefixed, split_u32_prefixed,
};
pub use inline::{
    InlineKind, InlineSpan, NOTE_MARKER, NoteKind, PendingNote, TextAlign, is_ascii_ident,
    is_font_id,
};
pub use table::{TableCell, TableData, TableRow};
pub use text::{
    FloatListSource, ListKind, OrderedListNumbering, TEXT_CAPTION_MAX, TEXT_HEADER_MAX_BYTES,
    TextHeader, TextRole, is_theorem_kind,
};
