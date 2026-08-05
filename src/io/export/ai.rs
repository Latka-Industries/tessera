//! LLM-oriented plain text and multipart AI export.

use crate::catalog::chunk::CitePayload;
use crate::catalog::file::TesFile;
use crate::catalog::index::ChunkType;
use crate::catalog::media::ImagePayload;
use crate::error::{Result, TesError};

use super::ExportOptions;
use super::common::{
    decode_attachment_entry, decode_figure_entry, decode_text_entry, format_ai_cite_prose,
    reading_order_scoped,
};

pub(super) fn export_ai_text(file: &TesFile, options: &ExportOptions) -> Result<String> {
    let mut parts: Vec<String> = Vec::new();

    let entries = reading_order_scoped(file, options)?;

    for entry in entries {
        match entry.chunk_type {
            ChunkType::Text => {
                let (_header, body) = decode_text_entry(file, entry)?;
                let body = body.trim_end().to_owned();
                if body.is_empty() {
                    continue;
                }
                if options.annotate {
                    parts.push(format!("<!-- chunk:{} -->\n{body}", entry.chunk_id));
                } else {
                    parts.push(body);
                }
            }
            ChunkType::Cite if !options.no_cites => {
                let raw = file.decode_payload(entry)?;
                match CitePayload::from_bytes(&raw) {
                    Ok(cite) => {
                        let text = format_ai_cite_prose(&cite);
                        if options.annotate {
                            parts.push(format!("<!-- chunk:{} -->\n{text}", entry.chunk_id));
                        } else {
                            parts.push(text);
                        }
                    }
                    Err(_) => {
                        parts.push(format!("[citation unresolved: chunk {}]", entry.chunk_id));
                    }
                }
            }
            ChunkType::Figure => {
                let figure = decode_figure_entry(file, entry)?;
                let mut text = format!("[image: {}]", figure.alt_text.trim());
                if let Some(caption) = figure.caption.as_deref() {
                    text.push(' ');
                    text.push_str(caption.trim());
                }
                if options.annotate {
                    parts.push(format!("<!-- chunk:{} -->\n{text}", entry.chunk_id));
                } else {
                    parts.push(text);
                }
            }
            ChunkType::Attachment => {
                let att = decode_attachment_entry(file, entry)?;
                let mut text = format!(
                    "[attachment: {} ({}) sha256={}]",
                    att.filename, att.media_type, att.sha256
                );
                if let Some(caption) = att.caption.as_deref() {
                    text.push(' ');
                    text.push_str(caption.trim());
                }
                if options.annotate {
                    parts.push(format!("<!-- chunk:{} -->\n{text}", entry.chunk_id));
                } else {
                    parts.push(text);
                }
            }
            _ => {}
        }
    }

    let mut out = parts.join("\n\n");
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    Ok(out)
}

pub enum AiPart {
    /// Plain UTF-8 prose.
    Text(String),
    /// Image bytes referenced from a figure (or direct image chunk).
    Image {
        /// Figure chunk id when exported from a figure; else the image chunk id.
        chunk_id: u64,
        /// Target image chunk id holding the bytes.
        image_chunk_id: u64,
        /// IANA media type.
        media_type: String,
        /// Intrinsic width (0 = unknown).
        width_px: u32,
        /// Intrinsic height (0 = unknown).
        height_px: u32,
        /// Raw image bytes.
        data: Vec<u8>,
        /// Alt text from the figure (or empty for bare image).
        alt_text: String,
        /// Optional title above the figure.
        title: Option<String>,
        /// Optional caption under the figure.
        caption: Option<String>,
    },
}

/// Export reading-order content as typed [`AiPart`]s (text + image bytes).
///
/// # Errors
///
/// Returns [`TesError::ChunkNotFound`] if a requested chunk is missing, or
/// [`TesError::Decode`] when a text/figure/image payload cannot be decoded.
pub fn export_ai_parts(file: &TesFile, options: &ExportOptions) -> Result<Vec<AiPart>> {
    let entries = reading_order_scoped(file, options)?;

    let mut parts = Vec::new();
    for entry in entries {
        match entry.chunk_type {
            ChunkType::Text => {
                let (_header, body) = decode_text_entry(file, entry)?;
                let body = body.trim_end().to_owned();
                if !body.is_empty() {
                    parts.push(AiPart::Text(body));
                }
            }
            ChunkType::Figure => {
                let figure = decode_figure_entry(file, entry)?;
                let image_entry = file.chunk_by_id(figure.image_chunk_id)?;
                let raw = file.decode_payload(image_entry)?;
                let image = ImagePayload::from_bytes(&raw).map_err(|e| TesError::Decode {
                    chunk_id: image_entry.chunk_id,
                    message: e.to_string(),
                })?;
                parts.push(AiPart::Image {
                    chunk_id: entry.chunk_id,
                    image_chunk_id: figure.image_chunk_id,
                    media_type: image.media_type,
                    width_px: image.width_px,
                    height_px: image.height_px,
                    data: image.data,
                    alt_text: figure.alt_text,
                    title: figure.title,
                    caption: figure.caption,
                });
            }
            ChunkType::Cite if !options.no_cites => {
                let raw = file.decode_payload(entry)?;
                if let Ok(cite) = CitePayload::from_bytes(&raw) {
                    parts.push(AiPart::Text(format_ai_cite_prose(&cite)));
                }
            }
            _ => {}
        }
    }
    Ok(parts)
}
