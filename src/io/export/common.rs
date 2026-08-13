//! Shared export helpers: scope selection, payload decode, escaping, media URLs.

use crate::catalog::chunk::{CitePayload, TextHeader, TextRole, decode_text_payload};
use crate::catalog::file::TesFile;
use crate::catalog::index::{ChunkIndexEntry, ChunkType};
use crate::catalog::media::{AttachmentPayload, FigureRef, base64_encode};
use crate::error::{Result, TesError};
use crate::io::bib::BibEntry;

use super::ExportOptions;

pub(super) fn markdown_escape_alt(value: &str) -> String {
    value.replace('[', "\\[").replace(']', "\\]")
}

pub(crate) fn escape_html(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

pub(super) fn html_class_attr(classes: &[String]) -> String {
    if classes.is_empty() {
        String::new()
    } else {
        format!(" class=\"{}\"", escape_html(&classes.join(" ")))
    }
}

pub(super) fn selected_text_entries<'a>(
    file: &'a TesFile,
    options: &ExportOptions,
) -> Result<Vec<&'a ChunkIndexEntry>> {
    let entries = reading_order_scoped(file, options)?;
    if let Some(id) = options.chunk_id {
        let entry = entries[0];
        if entry.chunk_type != ChunkType::Text {
            return Err(TesError::Decode {
                chunk_id: id,
                message: format!(
                    "chunk type is '{}'; --raw/--linear require text",
                    entry.chunk_type.as_str()
                ),
            });
        }
        return Ok(entries);
    }
    Ok(entries
        .into_iter()
        .filter(|c| c.chunk_type == ChunkType::Text)
        .collect())
}

pub(super) fn selected_content_entries<'a>(
    file: &'a TesFile,
    options: &ExportOptions,
) -> Result<Vec<&'a ChunkIndexEntry>> {
    let entries = reading_order_scoped(file, options)?;
    if let Some(id) = options.chunk_id {
        let entry = entries[0];
        if !is_content_export_type(entry.chunk_type) {
            return Err(TesError::Decode {
                chunk_id: id,
                message: format!(
                    "chunk type is '{}'; content exports require text, figure, cite, slide, or attachment",
                    entry.chunk_type.as_str()
                ),
            });
        }
        return Ok(entries);
    }
    Ok(entries
        .into_iter()
        .filter(|c| is_content_export_type(c.chunk_type))
        .collect())
}

/// Reading-order chunks, optionally scoped to `--chunk` or `--chapter`.
pub(super) fn reading_order_scoped<'a>(
    file: &'a TesFile,
    options: &ExportOptions,
) -> Result<Vec<&'a ChunkIndexEntry>> {
    if options.chunk_id.is_some() && options.chapter.is_some() {
        return Err(TesError::ExportScope {
            message: "--chunk and --chapter are mutually exclusive".into(),
        });
    }
    if let Some(id) = options.chunk_id {
        return Ok(vec![file.chunk_by_id(id)?]);
    }
    let entries = file.reading_order_chunks();
    if let Some(chapter) = options.chapter {
        return chapter_slice(file, &entries, chapter);
    }
    Ok(entries)
}

/// Default heading level that opens a manuscript chapter (H1).
const CHAPTER_HEADING_LEVEL: u32 = 1;

/// Slice reading-order entries to the Nth chapter (1-based).
///
/// A chapter starts at a text heading with level [`CHAPTER_HEADING_LEVEL`] and
/// ends just before the next such heading. Front matter before the first H1 is
/// excluded. Scene headings (H2+) stay inside their parent chapter.
pub(crate) fn chapter_slice<'a>(
    file: &'a TesFile,
    entries: &[&'a ChunkIndexEntry],
    chapter: u32,
) -> Result<Vec<&'a ChunkIndexEntry>> {
    if chapter == 0 {
        return Err(TesError::ExportScope {
            message: "--chapter must be >= 1 (1-based)".into(),
        });
    }
    let mut starts: Vec<usize> = Vec::new();
    for (i, entry) in entries.iter().enumerate() {
        if is_chapter_heading(file, entry)? {
            starts.push(i);
        }
    }
    if starts.is_empty() {
        return Err(TesError::ExportScope {
            message: format!(
                "no chapter headings (H{CHAPTER_HEADING_LEVEL}) found; cannot select --chapter {chapter}"
            ),
        });
    }
    let idx = (chapter as usize).saturating_sub(1);
    let Some(&start) = starts.get(idx) else {
        return Err(TesError::ExportScope {
            message: format!(
                "chapter {chapter} not found (document has {} chapter{})",
                starts.len(),
                if starts.len() == 1 { "" } else { "s" }
            ),
        });
    };
    let end = starts.get(idx + 1).copied().unwrap_or(entries.len());
    Ok(entries[start..end].to_vec())
}

pub(crate) fn is_chapter_heading(file: &TesFile, entry: &ChunkIndexEntry) -> Result<bool> {
    if entry.chunk_type != ChunkType::Text {
        return Ok(false);
    }
    let (header, _) = decode_text_entry(file, entry)?;
    Ok(header.role == TextRole::Heading
        && header.level.unwrap_or(CHAPTER_HEADING_LEVEL) == CHAPTER_HEADING_LEVEL)
}

pub(super) fn is_content_export_type(chunk_type: ChunkType) -> bool {
    matches!(
        chunk_type,
        ChunkType::Text
            | ChunkType::Figure
            | ChunkType::Cite
            | ChunkType::Slide
            | ChunkType::Attachment
            | ChunkType::Layout
    )
}

pub(crate) fn cite_number_map(
    file: &TesFile,
    entries: &[&ChunkIndexEntry],
) -> Result<std::collections::HashMap<u64, usize>> {
    let mut map = std::collections::HashMap::new();
    let mut n = 0usize;
    for entry in entries {
        if entry.chunk_type != ChunkType::Cite {
            continue;
        }
        let cite = decode_cite_entry(file, entry)?;
        if !crate::io::cite::is_biblio_cite(&cite) {
            continue;
        }
        n += 1;
        map.insert(entry.chunk_id, n);
    }
    Ok(map)
}

pub(crate) fn decode_cite_entry(file: &TesFile, entry: &ChunkIndexEntry) -> Result<CitePayload> {
    let raw = file.decode_payload(entry)?;
    CitePayload::from_bytes(&raw).map_err(|e| TesError::Decode {
        chunk_id: entry.chunk_id,
        message: e.to_string(),
    })
}

pub(crate) fn decode_numbered_cite(
    file: &TesFile,
    entry: &ChunkIndexEntry,
    numbers: &std::collections::HashMap<u64, usize>,
) -> Result<(usize, CitePayload, BibEntry)> {
    let cite = decode_cite_entry(file, entry)?;
    let n = *numbers.get(&entry.chunk_id).unwrap_or(&0);
    let bib = if let Some(source) = &cite.source {
        source.clone()
    } else {
        BibEntry {
            cite_key: cite
                .label
                .clone()
                .unwrap_or_else(|| format!("chunk-{}", entry.chunk_id)),
            entry_type: "misc".into(),
            title: if cite.quote.trim().is_empty() {
                None
            } else {
                Some(cite.quote.clone())
            },
            note: cite.page.map(|p| format!("page {p}")),
            ..BibEntry::default()
        }
    };
    Ok((n, cite, bib))
}

pub(crate) fn decode_text_entry(
    file: &TesFile,
    entry: &ChunkIndexEntry,
) -> Result<(TextHeader, String)> {
    let raw = file.decode_payload(entry)?;
    decode_text_payload(&raw).map_err(|e| TesError::Decode {
        chunk_id: entry.chunk_id,
        message: e.to_string(),
    })
}

pub(crate) fn decode_figure_entry(file: &TesFile, entry: &ChunkIndexEntry) -> Result<FigureRef> {
    let raw = file.decode_payload(entry)?;
    FigureRef::from_bytes(&raw).map_err(|e| TesError::Decode {
        chunk_id: entry.chunk_id,
        message: e.to_string(),
    })
}

pub(super) fn decode_attachment_entry(
    file: &TesFile,
    entry: &ChunkIndexEntry,
) -> Result<AttachmentPayload> {
    let raw = file.decode_payload(entry)?;
    AttachmentPayload::from_bytes(&raw).map_err(|e| TesError::Decode {
        chunk_id: entry.chunk_id,
        message: e.to_string(),
    })
}
/// Decode a single attachment chunk's opaque bytes for explicit download/export.
///
/// # Errors
///
/// Returns [`TesError::ChunkNotFound`], [`TesError::Decode`], or
/// [`TesError::InvalidAttachment`] when the chunk is missing or not an attachment.
pub fn export_attachment_bytes(file: &TesFile, chunk_id: u64) -> Result<AttachmentPayload> {
    let entry = file.chunk_by_id(chunk_id)?;
    if entry.chunk_type != ChunkType::Attachment {
        return Err(TesError::InvalidAttachment {
            message: format!(
                "chunk {chunk_id} is type '{}', expected attachment",
                entry.chunk_type.as_str()
            ),
        });
    }
    decode_attachment_entry(file, entry)
}

pub(crate) fn decode_slide_entry(
    file: &TesFile,
    entry: &ChunkIndexEntry,
) -> Result<crate::catalog::SlidePayload> {
    let raw = file.decode_payload(entry)?;
    crate::catalog::SlidePayload::from_bytes(raw.as_ref())
}

pub(crate) fn decode_layout_entry(
    file: &TesFile,
    entry: &ChunkIndexEntry,
) -> Result<crate::catalog::LayoutPayload> {
    let raw = file.decode_payload(entry)?;
    crate::catalog::LayoutPayload::from_bytes(raw.as_ref()).map_err(|e| TesError::Decode {
        chunk_id: entry.chunk_id,
        message: e.to_string(),
    })
}

pub(super) fn image_src(
    options: &ExportOptions,
    chunk_id: u64,
    media_type: &str,
    data: &[u8],
) -> String {
    if let Some(prefix) = options.media_url_prefix.as_deref() {
        format!("{prefix}{chunk_id}")
    } else {
        format!("data:{media_type};base64,{}", base64_encode(data))
    }
}

/// Plain-prose citation line for AI exports (no markdown/HTML).
pub(super) fn format_ai_cite_prose(cite: &CitePayload) -> String {
    if cite.quote.trim().is_empty() {
        format!(
            "[citation unresolved: {}]",
            cite.label.as_deref().unwrap_or("unknown")
        )
    } else if let Some(label) = cite.label.as_deref() {
        format!("{label} reported that {}", cite.quote.trim())
    } else {
        cite.quote.trim().to_owned()
    }
}
