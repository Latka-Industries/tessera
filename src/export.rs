//! Decoded export views (`docs/exports.md`).
//!
//! Exports are **projections** of a sealed `.tes` file — never the canonical
//! source. Models and pipelines should call these views rather than hex-dumping
//! the wire format.

use std::fmt::Write as _;
use std::path::Path;

use serde::Serialize;

use crate::catalog::chunk::{CitePayload, ListKind, TextHeader, TextRole, decode_text_payload};
use crate::catalog::file::TesFile;
use crate::catalog::index::{ChunkIndexEntry, ChunkType};
use crate::error::{Result, TesError};

/// Which decoded view to emit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportView {
    /// Concatenate text chunk bodies (`--raw`).
    Raw,
    /// Reading-order prose with light structure markers (`--linear`).
    Linear,
    /// LLM-oriented plain text, no exporter-introduced markup (`--ai-text`).
    AiText,
    /// One JSON object per reading-order chunk (`--chunks-jsonl`).
    ChunksJsonl,
}

/// Options that refine an [`ExportView`].
#[derive(Debug, Clone, Default)]
pub struct ExportOptions {
    /// Restrict output to a single chunk id (where applicable).
    pub chunk_id: Option<u64>,
    /// Prefix each `--raw` chunk with a debug header line.
    pub include_headers: bool,
    /// Prefix each `--ai-text` chunk with `<!-- chunk:N -->`.
    pub annotate: bool,
    /// Include non-reading-order / non-text rows in `--chunks-jsonl`.
    pub all_types: bool,
    /// Omit cite chunk expansion from `--ai-text`.
    pub no_cites: bool,
}

/// Export `path` as the selected view.
pub fn export_view(
    path: impl AsRef<Path>,
    view: ExportView,
    options: &ExportOptions,
) -> Result<String> {
    let file = TesFile::open(path.as_ref())?;
    export_file(&file, view, options)
}

/// Export an already-open file.
pub fn export_file(file: &TesFile, view: ExportView, options: &ExportOptions) -> Result<String> {
    match view {
        ExportView::Raw => export_raw(file, options),
        ExportView::Linear => export_linear(file, options),
        ExportView::AiText => export_ai_text(file, options),
        ExportView::ChunksJsonl => export_chunks_jsonl(file, options),
    }
}

fn export_raw(file: &TesFile, options: &ExportOptions) -> Result<String> {
    let entries = selected_text_entries(file, options)?;
    let mut out = String::new();
    for (i, entry) in entries.iter().enumerate() {
        let (header, body) = decode_text_entry(file, entry)?;
        if options.include_headers {
            let _ = writeln!(
                out,
                "[chunk_id={} role={}{}]",
                entry.chunk_id,
                header.role.as_str(),
                header
                    .level
                    .map(|l| format!(" level={l}"))
                    .unwrap_or_default()
            );
        }
        out.push_str(&body);
        if i + 1 < entries.len() {
            out.push_str("\n\n");
        }
    }
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    Ok(out)
}

fn export_linear(file: &TesFile, options: &ExportOptions) -> Result<String> {
    let entries = selected_text_entries(file, options)?;
    let mut out = String::new();
    for (i, entry) in entries.iter().enumerate() {
        let (header, body) = decode_text_entry(file, entry)?;
        match header.role {
            TextRole::Heading => {
                let level = header.level.unwrap_or(1).clamp(1, 6) as usize;
                out.push_str(&"#".repeat(level));
                out.push(' ');
                out.push_str(body.trim_end());
                out.push('\n');
            }
            TextRole::ListItem => {
                let marker = match header.list_kind.unwrap_or(ListKind::Bullet) {
                    ListKind::Bullet => "- ".to_owned(),
                    ListKind::Ordered => "1. ".to_owned(),
                };
                out.push_str(&marker);
                out.push_str(body.trim_end());
                out.push('\n');
            }
            TextRole::Blockquote => {
                for line in body.lines() {
                    out.push_str("> ");
                    out.push_str(line);
                    out.push('\n');
                }
            }
            TextRole::CodeBlock => {
                out.push_str("```\n");
                out.push_str(body.trim_end());
                out.push_str("\n```\n");
            }
            TextRole::Paragraph | TextRole::Table => {
                out.push_str(body.trim_end());
                out.push('\n');
            }
        }
        if i + 1 < entries.len() {
            out.push('\n');
        }
    }
    Ok(out)
}

fn export_ai_text(file: &TesFile, options: &ExportOptions) -> Result<String> {
    let mut parts: Vec<String> = Vec::new();

    let entries: Vec<&ChunkIndexEntry> = if let Some(id) = options.chunk_id {
        vec![file.chunk_by_id(id)?]
    } else {
        file.reading_order_chunks()
    };

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
                        let text = if cite.quote.trim().is_empty() {
                            format!(
                                "[citation unresolved: {}]",
                                cite.label.as_deref().unwrap_or("unknown")
                            )
                        } else if let Some(label) = cite.label.as_deref() {
                            // Plain sentence form; no markdown/HTML introduced.
                            format!("{label} reported that {}", cite.quote.trim())
                        } else {
                            cite.quote.trim().to_owned()
                        };
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
            _ => {}
        }
    }

    let mut out = parts.join("\n\n");
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    Ok(out)
}

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
}

fn export_chunks_jsonl(file: &TesFile, options: &ExportOptions) -> Result<String> {
    let doc_id = file.catalog().map(|c| c.doc_id.as_str()).unwrap_or("");
    let doc_title = file.catalog().map(|c| c.title.as_str()).unwrap_or("");

    let entries: Vec<&ChunkIndexEntry> = if let Some(id) = options.chunk_id {
        vec![file.chunk_by_id(id)?]
    } else if options.all_types {
        file.chunks().iter().collect()
    } else {
        file.reading_order_chunks()
            .into_iter()
            .filter(|c| c.chunk_type == ChunkType::Text || c.chunk_type == ChunkType::Cite)
            .collect()
    };

    let mut out = String::new();
    for entry in entries {
        match entry.chunk_type {
            ChunkType::Text => {
                let (header, body) = decode_text_entry(file, entry)?;
                let list_kind = header.list_kind.map(|k| match k {
                    ListKind::Bullet => "bullet",
                    ListKind::Ordered => "ordered",
                });
                let row = ChunkJsonlRow {
                    doc_id,
                    doc_title,
                    chunk_id: entry.chunk_id,
                    chunk_type: "text",
                    role: Some(header.role.as_str()),
                    level: header.level,
                    list_kind,
                    byte_len: Some(body.len()),
                    text: Some(&body),
                    quote: None,
                    target_doc_id: None,
                    target_chunk_id: None,
                    label: None,
                    resolved_text: None,
                };
                out.push_str(&serde_json::to_string(&row)?);
                out.push('\n');
            }
            ChunkType::Cite => {
                let raw = file.decode_payload(entry)?;
                let cite = CitePayload::from_bytes(&raw).unwrap_or(CitePayload {
                    quote: String::new(),
                    target_doc_id: None,
                    target_chunk_id: None,
                    target_byte_start: None,
                    target_byte_end: None,
                    label: None,
                    page: None,
                });
                let resolved = if cite.quote.is_empty() {
                    None
                } else {
                    Some(cite.quote.as_str())
                };
                let row = ChunkJsonlRow {
                    doc_id,
                    doc_title,
                    chunk_id: entry.chunk_id,
                    chunk_type: "cite",
                    role: None,
                    level: None,
                    list_kind: None,
                    byte_len: None,
                    text: None,
                    quote: Some(cite.quote.as_str()),
                    target_doc_id: cite.target_doc_id.as_deref(),
                    target_chunk_id: cite.target_chunk_id,
                    label: cite.label.as_deref(),
                    resolved_text: resolved,
                };
                out.push_str(&serde_json::to_string(&row)?);
                out.push('\n');
            }
            other if options.all_types => {
                let row = ChunkJsonlRow {
                    doc_id,
                    doc_title,
                    chunk_id: entry.chunk_id,
                    chunk_type: other.as_str(),
                    role: None,
                    level: None,
                    list_kind: None,
                    byte_len: None,
                    text: None,
                    quote: None,
                    target_doc_id: None,
                    target_chunk_id: None,
                    label: None,
                    resolved_text: None,
                };
                out.push_str(&serde_json::to_string(&row)?);
                out.push('\n');
            }
            _ => {}
        }
    }
    Ok(out)
}

fn selected_text_entries<'a>(
    file: &'a TesFile,
    options: &ExportOptions,
) -> Result<Vec<&'a ChunkIndexEntry>> {
    if let Some(id) = options.chunk_id {
        let entry = file.chunk_by_id(id)?;
        if entry.chunk_type != ChunkType::Text {
            return Err(TesError::Decode {
                chunk_id: id,
                message: format!(
                    "chunk type is '{}'; --raw/--linear require text",
                    entry.chunk_type.as_str()
                ),
            });
        }
        return Ok(vec![entry]);
    }
    Ok(file
        .reading_order_chunks()
        .into_iter()
        .filter(|c| c.chunk_type == ChunkType::Text)
        .collect())
}

fn decode_text_entry(file: &TesFile, entry: &ChunkIndexEntry) -> Result<(TextHeader, String)> {
    let raw = file.decode_payload(entry)?;
    decode_text_payload(&raw).map_err(|e| TesError::Decode {
        chunk_id: entry.chunk_id,
        message: e.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{DocumentCatalog, ListKind, TesWriterSession, TextHeader};
    use crate::layout::DocKind;
    use std::path::PathBuf;
    use tempfile::tempdir;

    fn write_note(dir: &std::path::Path) -> PathBuf {
        let path = dir.join("note.tes");
        let mut s = TesWriterSession::create(&path, DocKind::Note);
        s.set_catalog(DocumentCatalog::new(
            "550e8400-e29b-41d4-a716-446655440000",
            "Meeting notes",
            "2026-06-05T12:00:00Z",
            "2026-06-05T12:30:00Z",
            DocKind::Note,
        ))
        .unwrap();
        s.add_text_chunk(&TextHeader::paragraph(), "Hello from Tessera.")
            .unwrap();
        s.commit().unwrap();
        path
    }

    fn write_article(dir: &std::path::Path) -> PathBuf {
        let path = dir.join("article.tes");
        let mut s = TesWriterSession::create(&path, DocKind::Document);
        s.set_catalog(DocumentCatalog::new(
            "660e8400-e29b-41d4-a716-446655440001",
            "Methods",
            "2026-06-05T12:00:00Z",
            "2026-06-05T12:00:00Z",
            DocKind::Document,
        ))
        .unwrap();
        s.add_text_chunk(&TextHeader::heading(1), "Methods")
            .unwrap();
        s.add_text_chunk(
            &TextHeader::paragraph(),
            "We measured temperature at 15 stations.",
        )
        .unwrap();
        s.add_text_chunk(&TextHeader::list_item(ListKind::Bullet), "Calibrate first")
            .unwrap();
        s.commit().unwrap();
        path
    }

    #[test]
    fn raw_note_one_chunk_fixture() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/v0/note_one_chunk.tes");
        let out = export_view(&path, ExportView::Raw, &ExportOptions::default()).unwrap();
        assert_eq!(out, "Hello from Tessera.\n");
    }

    #[test]
    fn ai_text_has_no_exporter_markup() {
        let dir = tempdir().unwrap();
        let path = write_article(dir.path());
        let out = export_view(&path, ExportView::AiText, &ExportOptions::default()).unwrap();
        assert!(!out.contains('#'));
        assert!(!out.contains("<"));
        assert!(!out.contains("**"));
        assert!(out.contains("We measured temperature at 15 stations."));
        assert!(out.contains("Calibrate first"));
        // Heading body is included as plain text, without # markers.
        assert!(out.contains("Methods"));
    }

    #[test]
    fn linear_emits_heading_markers() {
        let dir = tempdir().unwrap();
        let path = write_article(dir.path());
        let out = export_view(&path, ExportView::Linear, &ExportOptions::default()).unwrap();
        assert!(out.starts_with("# Methods\n"));
        assert!(out.contains("\n- Calibrate first\n"));
    }

    #[test]
    fn chunks_jsonl_line_count_matches_reading_order() {
        let dir = tempdir().unwrap();
        let path = write_article(dir.path());
        let out = export_view(&path, ExportView::ChunksJsonl, &ExportOptions::default()).unwrap();
        let lines: Vec<&str> = out.lines().filter(|l| !l.is_empty()).collect();
        assert_eq!(lines.len(), 3);
        let first: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(first["role"], "heading");
        assert_eq!(first["text"], "Methods");
        assert_eq!(first["doc_title"], "Methods");
    }

    #[test]
    fn raw_chunk_filter() {
        let dir = tempdir().unwrap();
        let path = write_article(dir.path());
        let opts = ExportOptions {
            chunk_id: Some(2),
            ..Default::default()
        };
        let out = export_view(&path, ExportView::Raw, &opts).unwrap();
        assert_eq!(out, "We measured temperature at 15 stations.\n");
    }

    #[test]
    fn annotate_ai_text() {
        let dir = tempdir().unwrap();
        let path = write_note(dir.path());
        let opts = ExportOptions {
            annotate: true,
            ..Default::default()
        };
        let out = export_view(&path, ExportView::AiText, &opts).unwrap();
        assert!(out.starts_with("<!-- chunk:1 -->\nHello from Tessera."));
    }
}
