//! One JSON object per reading-order chunk (`--chunks-jsonl`).

use serde::Serialize;

use crate::catalog::chunk::{CitePayload, ListKind, TextRole};
use crate::catalog::file::TesFile;
use crate::catalog::index::{ChunkIndexEntry, ChunkType};
use crate::catalog::media::ImagePayload;
use crate::error::Result;

use super::ExportOptions;
use super::common::{
    decode_attachment_entry, decode_figure_entry, decode_slide_entry, decode_text_entry,
    is_content_export_type, reading_order_scoped,
};

#[derive(Serialize)]
struct ChunkJsonlRow<'a> {
    doc_id: &'a str,
    doc_title: &'a str,
    chunk_id: u64,
    chunk_type: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    role: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    level: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    list_kind: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    list_depth: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    byte_len: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    quote: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    target_doc_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    target_chunk_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    label: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    resolved_text: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    image_chunk_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    alt_text: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    caption: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    placement: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    media_type: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    layout_id: Option<&'a str>,
}

pub(super) fn export_chunks_jsonl(file: &TesFile, options: &ExportOptions) -> Result<String> {
    let doc_id = file.catalog().map_or("", |c| c.doc_id.as_str());
    let doc_title = file.catalog().map_or("", |c| c.title.as_str());
    let entries = jsonl_entries(file, options)?;

    let mut out = String::new();
    for entry in entries {
        match entry.chunk_type {
            ChunkType::Text => {
                append_jsonl_text(&mut out, file, entry, doc_id, doc_title)?;
            }
            ChunkType::Cite => {
                append_jsonl_cite(&mut out, file, entry, doc_id, doc_title)?;
            }
            ChunkType::Figure => {
                append_jsonl_figure(&mut out, file, entry, doc_id, doc_title)?;
            }
            ChunkType::Slide => {
                append_jsonl_slide(&mut out, file, entry, doc_id, doc_title)?;
            }
            ChunkType::Attachment => {
                append_jsonl_attachment(&mut out, file, entry, doc_id, doc_title)?;
            }
            other if options.all_types => {
                push_jsonl_row(
                    &mut out,
                    &ChunkJsonlRow::bare(doc_id, doc_title, entry.chunk_id, other.as_str()),
                )?;
            }
            _ => {}
        }
    }
    Ok(out)
}

fn jsonl_entries<'a>(
    file: &'a TesFile,
    options: &ExportOptions,
) -> Result<Vec<&'a ChunkIndexEntry>> {
    if options.chunk_id.is_some() || options.chapter.is_some() {
        let scoped = reading_order_scoped(file, options)?;
        if options.all_types {
            return Ok(scoped);
        }
        return Ok(scoped
            .into_iter()
            .filter(|c| is_content_export_type(c.chunk_type))
            .collect());
    }
    if options.all_types {
        return Ok(file.chunks().iter().collect());
    }
    Ok(file
        .reading_order_chunks()
        .into_iter()
        .filter(|c| is_content_export_type(c.chunk_type))
        .collect())
}

impl<'a> ChunkJsonlRow<'a> {
    fn bare(doc_id: &'a str, doc_title: &'a str, chunk_id: u64, chunk_type: &'static str) -> Self {
        Self {
            doc_id,
            doc_title,
            chunk_id,
            chunk_type,
            role: None,
            level: None,
            list_kind: None,
            list_depth: None,
            byte_len: None,
            text: None,
            quote: None,
            target_doc_id: None,
            target_chunk_id: None,
            label: None,
            resolved_text: None,
            image_chunk_id: None,
            alt_text: None,
            title: None,
            caption: None,
            placement: None,
            media_type: None,
            layout_id: None,
        }
    }
}

fn push_jsonl_row(out: &mut String, row: &ChunkJsonlRow<'_>) -> Result<()> {
    out.push_str(&serde_json::to_string(row)?);
    out.push('\n');
    Ok(())
}

fn append_jsonl_text(
    out: &mut String,
    file: &TesFile,
    entry: &ChunkIndexEntry,
    doc_id: &str,
    doc_title: &str,
) -> Result<()> {
    let (header, body) = decode_text_entry(file, entry)?;
    let list_kind = header.list_kind.map(|k| match k {
        ListKind::Bullet => "bullet",
        ListKind::Ordered => "ordered",
    });
    let mut row = ChunkJsonlRow::bare(doc_id, doc_title, entry.chunk_id, "text");
    row.role = Some(header.role.as_str());
    row.level = header.level;
    row.list_kind = list_kind;
    if header.role == TextRole::ListItem {
        let depth = header.list_depth_or_default();
        if depth > 1 {
            row.list_depth = Some(depth);
        }
    }
    row.byte_len = Some(body.len());
    row.text = Some(&body);
    row.title = header.title.as_deref();
    row.caption = header.caption.as_deref();
    push_jsonl_row(out, &row)
}

fn append_jsonl_cite(
    out: &mut String,
    file: &TesFile,
    entry: &ChunkIndexEntry,
    doc_id: &str,
    doc_title: &str,
) -> Result<()> {
    let raw = file.decode_payload(entry)?;
    let cite = CitePayload::from_bytes(&raw).unwrap_or(CitePayload {
        quote: String::new(),
        target_doc_id: None,
        target_chunk_id: None,
        target_byte_start: None,
        target_byte_end: None,
        label: None,
        page: None,
        source: None,
    });
    let resolved = if cite.quote.is_empty() {
        None
    } else {
        Some(cite.quote.as_str())
    };
    let mut row = ChunkJsonlRow::bare(doc_id, doc_title, entry.chunk_id, "cite");
    row.quote = Some(cite.quote.as_str());
    row.target_doc_id = cite.target_doc_id.as_deref();
    row.target_chunk_id = cite.target_chunk_id;
    row.label = cite.label.as_deref();
    row.resolved_text = resolved;
    push_jsonl_row(out, &row)
}

fn append_jsonl_figure(
    out: &mut String,
    file: &TesFile,
    entry: &ChunkIndexEntry,
    doc_id: &str,
    doc_title: &str,
) -> Result<()> {
    let figure = decode_figure_entry(file, entry)?;
    let media_type = file
        .chunk_by_id(figure.image_chunk_id)
        .ok()
        .filter(|e| e.chunk_type == ChunkType::Image)
        .and_then(|e| file.decode_payload(e).ok())
        .and_then(|raw| ImagePayload::from_bytes(&raw).ok())
        .map(|img| img.media_type);
    let mut row = ChunkJsonlRow::bare(doc_id, doc_title, entry.chunk_id, "figure");
    row.image_chunk_id = Some(figure.image_chunk_id);
    row.alt_text = Some(figure.alt_text.as_str());
    row.title = figure.title.as_deref();
    row.caption = figure.caption.as_deref();
    row.placement = Some(figure.placement.as_str());
    row.media_type = media_type.as_deref();
    push_jsonl_row(out, &row)
}

fn append_jsonl_slide(
    out: &mut String,
    file: &TesFile,
    entry: &ChunkIndexEntry,
    doc_id: &str,
    doc_title: &str,
) -> Result<()> {
    let slide = decode_slide_entry(file, entry)?;
    let summary = slide
        .regions
        .iter()
        .map(|r| format!("{}={}", r.name, r.chunk_id))
        .collect::<Vec<_>>()
        .join(",");
    let owned = summary; // keep alive for row
    let mut row = ChunkJsonlRow::bare(doc_id, doc_title, entry.chunk_id, "slide");
    row.layout_id = Some(slide.layout_id.as_str());
    row.text = Some(owned.as_str());
    push_jsonl_row(out, &row)
}

fn append_jsonl_attachment(
    out: &mut String,
    file: &TesFile,
    entry: &ChunkIndexEntry,
    doc_id: &str,
    doc_title: &str,
) -> Result<()> {
    let att = decode_attachment_entry(file, entry)?;
    let mut row = ChunkJsonlRow::bare(doc_id, doc_title, entry.chunk_id, "attachment");
    row.text = Some(att.filename.as_str());
    row.media_type = Some(att.media_type.as_str());
    row.caption = att.caption.as_deref();
    row.label = Some(att.sha256.as_str());
    row.byte_len = Some(att.data.len());
    push_jsonl_row(out, &row)
}
