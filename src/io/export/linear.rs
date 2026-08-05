//! `--linear` reading-order prose with light structure markers.

use std::fmt::Write as _;

use crate::catalog::chunk::{OrderedListNumbering, TextHeader, TextRole};
use crate::catalog::file::TesFile;
use crate::catalog::index::ChunkType;
use crate::catalog::media::{AttachmentPayload, FigureRef};
use crate::error::Result;
use crate::io::bib::{BibEntry, format_numeric_marker, format_numeric_reference};

use super::ExportOptions;
use super::common::{
    cite_number_map, decode_attachment_entry, decode_figure_entry, decode_numbered_cite,
    decode_slide_entry, decode_text_entry, selected_content_entries,
};

pub(super) fn export_linear(file: &TesFile, options: &ExportOptions) -> Result<String> {
    let entries = selected_content_entries(file, options)?;
    let cite_numbers = cite_number_map(file, &entries)?;
    let mut out = String::new();
    let mut bib_items: Vec<(usize, BibEntry)> = Vec::new();
    let mut ordered = OrderedListNumbering::default();
    for (i, entry) in entries.iter().enumerate() {
        match entry.chunk_type {
            ChunkType::Text => {
                let (header, body) = decode_text_entry(file, entry)?;
                let ordered_index = ordered.take_for_text(&header);
                append_linear_text(&mut out, &header, &body, ordered_index);
            }
            other => {
                ordered.clear();
                match other {
                    ChunkType::Figure => {
                        let figure = decode_figure_entry(file, entry)?;
                        append_linear_figure(&mut out, &figure);
                    }
                    ChunkType::Cite if !options.no_cites => {
                        let (n, cite, bib) = decode_numbered_cite(file, entry, &cite_numbers)?;
                        let marker = format_numeric_marker(n);
                        if cite.quote.trim().is_empty() {
                            let _ = writeln!(out, "{marker}");
                        } else {
                            let _ = writeln!(out, "{marker} {}", cite.quote.trim());
                        }
                        bib_items.push((n, bib));
                    }
                    ChunkType::Slide => {
                        let slide = decode_slide_entry(file, entry)?;
                        let _ = writeln!(out, "[slide layout={}]", slide.layout_id);
                        for region in &slide.regions {
                            let _ = writeln!(out, "  {}: chunk-{}", region.name, region.chunk_id);
                        }
                    }
                    ChunkType::Attachment => {
                        let att = decode_attachment_entry(file, entry)?;
                        append_linear_attachment(&mut out, &att);
                    }
                    _ => {}
                }
            }
        }
        if i + 1 < entries.len() {
            out.push('\n');
        }
    }
    if !bib_items.is_empty() {
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        out.push_str("\nReferences\n");
        bib_items.sort_by_key(|(n, _)| *n);
        for (n, entry) in bib_items {
            let _ = writeln!(out, "{}", format_numeric_reference(n, &entry));
        }
    }
    Ok(out)
}

fn append_linear_text(
    out: &mut String,
    header: &TextHeader,
    body: &str,
    ordered_index: Option<u32>,
) {
    match header.role {
        TextRole::Heading => {
            let level = header.level.unwrap_or(1).clamp(1, 6) as usize;
            out.push_str(&"#".repeat(level));
            out.push(' ');
            out.push_str(body.trim_end());
            out.push('\n');
        }
        TextRole::ListItem => {
            out.push_str(&header.list_marker_prefix(ordered_index));
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
            append_linear_title(out, header.title.as_deref());
            out.push_str("```");
            if let Some(lang) = header.code_lang.as_deref() {
                out.push_str(lang);
            }
            out.push('\n');
            out.push_str(body.trim_end());
            out.push_str("\n```\n");
            append_linear_caption(out, header.caption.as_deref());
        }
        TextRole::Math => {
            append_linear_title(out, header.title.as_deref());
            out.push_str("$$\n");
            out.push_str(body.trim_end());
            out.push_str("\n$$\n");
            append_linear_caption(out, header.caption.as_deref());
        }
        TextRole::Paragraph | TextRole::Table => {
            if header.role == TextRole::Table {
                append_linear_title(out, header.title.as_deref());
            }
            if header.role == TextRole::Table && header.table.is_some() {
                out.push_str(&header.render_markdown(""));
            } else {
                out.push_str(body.trim_end());
            }
            out.push('\n');
            if header.role == TextRole::Table {
                append_linear_caption(out, header.caption.as_deref());
            }
        }
    }
}

fn append_linear_title(out: &mut String, title: Option<&str>) {
    if let Some(title) = title.filter(|s| !s.is_empty()) {
        let _ = writeln!(out, "**{title}**");
    }
}

fn append_linear_caption(out: &mut String, caption: Option<&str>) {
    if let Some(caption) = caption.filter(|s| !s.is_empty()) {
        let _ = writeln!(out, "*{caption}*");
    }
}

fn append_linear_figure(out: &mut String, figure: &FigureRef) {
    let _ = writeln!(
        out,
        "[figure image={} placement={}]\n{}",
        figure.image_chunk_id,
        figure.placement.as_str(),
        figure.alt_text.trim_end()
    );
    if let Some(caption) = figure.caption.as_deref() {
        let _ = writeln!(out, "{caption}");
    }
}

fn append_linear_attachment(out: &mut String, att: &AttachmentPayload) {
    let _ = writeln!(
        out,
        "[attachment filename={} media_type={} sha256={}]",
        att.filename, att.media_type, att.sha256
    );
    if let Some(caption) = att.caption.as_deref() {
        let _ = writeln!(out, "{caption}");
    }
}
