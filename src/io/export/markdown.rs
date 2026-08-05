//! Lossy GFM-ish Markdown projection (`--markdown`).

use std::fmt::Write as _;

use crate::catalog::chunk::{CitePayload, OrderedListNumbering, TextHeader};
use crate::catalog::file::TesFile;
use crate::catalog::index::{ChunkIndexEntry, ChunkType};
use crate::catalog::media::{AttachmentPayload, FigureRef};
use crate::catalog::slide::SlidePayload;
use crate::catalog::{InlineKind, LinkEntry};
use crate::error::Result;
use crate::io::bib::{BibEntry, format_numeric_reference, format_pandoc_cite};
use crate::io::cite::{self, CiteProj};

use super::ExportOptions;
use super::common::{
    cite_number_map, decode_attachment_entry, decode_cite_entry, decode_figure_entry,
    decode_numbered_cite, decode_slide_entry, decode_text_entry, markdown_escape_alt,
    selected_content_entries,
};

pub(super) fn export_markdown(file: &TesFile, options: &ExportOptions) -> Result<String> {
    let entries = selected_content_entries(file, options)?;
    let cite_numbers = cite_number_map(file, &entries)?;
    let (cite_keys, style) = cite::projection_maps(file);
    let cite = CiteProj {
        numbers: &cite_numbers,
        keys: &cite_keys,
        style,
    };
    let mut parts = Vec::with_capacity(entries.len());
    let mut bib_items: Vec<(usize, BibEntry)> = Vec::new();
    let mut ordered = OrderedListNumbering::default();
    for entry in entries {
        match entry.chunk_type {
            ChunkType::Text => {
                let (header, body) = decode_text_entry(file, entry)?;
                let ordered_index = ordered.take_for_text(&header);
                parts.push(markdown_text_block(
                    &header,
                    &body,
                    file.links(),
                    ordered_index,
                    cite,
                ));
            }
            other => {
                ordered.clear();
                push_markdown_non_text(
                    &mut parts,
                    &mut bib_items,
                    file,
                    entry,
                    other,
                    options,
                    &cite_numbers,
                )?;
            }
        }
    }
    append_markdown_references(&mut parts, &mut bib_items);
    let mut out = parts.join("\n\n");
    if !out.is_empty() {
        out.push('\n');
    }
    Ok(out)
}

fn push_markdown_non_text(
    parts: &mut Vec<String>,
    bib_items: &mut Vec<(usize, BibEntry)>,
    file: &TesFile,
    entry: &ChunkIndexEntry,
    kind: ChunkType,
    options: &ExportOptions,
    cite_numbers: &std::collections::HashMap<u64, usize>,
) -> Result<()> {
    match kind {
        ChunkType::Figure => {
            parts.push(markdown_figure_block(&decode_figure_entry(file, entry)?));
        }
        ChunkType::Cite if !options.no_cites => {
            let cite = decode_cite_entry(file, entry)?;
            match crate::io::cite::classify_cite(&cite) {
                crate::io::cite::CiteTessprekKind::Biblio => {
                    let (n, cite, bib) = decode_numbered_cite(file, entry, cite_numbers)?;
                    parts.push(markdown_cite_block(&cite, &bib));
                    bib_items.push((n, bib));
                }
                crate::io::cite::CiteTessprekKind::Quote => {
                    parts.push(markdown_quote_block(&cite));
                }
                crate::io::cite::CiteTessprekKind::Ref => {
                    parts.push(markdown_ref_block(&cite));
                }
            }
        }
        ChunkType::Slide => {
            parts.push(markdown_slide_block(&decode_slide_entry(file, entry)?));
        }
        ChunkType::Attachment => {
            parts.push(markdown_attachment_block(&decode_attachment_entry(
                file, entry,
            )?));
        }
        _ => {}
    }
    Ok(())
}

fn markdown_wrap_title_caption(title: Option<&str>, body: &str, caption: Option<&str>) -> String {
    let mut block = String::new();
    if let Some(title) = title.filter(|s| !s.is_empty()) {
        block.push_str("**");
        block.push_str(title.trim());
        block.push_str("**\n\n");
    }
    block.push_str(body);
    if let Some(caption) = caption.filter(|s| !s.is_empty()) {
        block.push_str("\n\n*");
        block.push_str(caption.trim());
        block.push('*');
    }
    block
}

fn markdown_text_block(
    header: &TextHeader,
    body: &str,
    links: &[LinkEntry],
    ordered_index: Option<u32>,
    cite: CiteProj<'_>,
) -> String {
    let rendered = render_markdown_with_cites(header, body, links, ordered_index, cite);
    markdown_wrap_title_caption(
        header.title.as_deref(),
        &rendered,
        header.caption.as_deref(),
    )
}

fn render_markdown_with_cites(
    header: &TextHeader,
    body: &str,
    links: &[LinkEntry],
    ordered_index: Option<u32>,
    cite: CiteProj<'_>,
) -> String {
    let mut header = header.clone();
    let mut body = body.to_owned();
    let mut cite_spans: Vec<_> = header
        .spans
        .iter()
        .filter(|s| matches!(s.kind, InlineKind::Citation { .. }))
        .cloned()
        .collect();
    cite_spans.sort_by_key(|s| std::cmp::Reverse(s.start));
    for span in cite_spans {
        let InlineKind::Citation { cite_chunk_id } = span.kind else {
            continue;
        };
        let start = span.start as usize;
        let end = span.end as usize;
        if end > body.len() || start > end {
            continue;
        }
        let marker = cite.marker(cite_chunk_id);
        body.replace_range(start..end, &marker);
    }
    header
        .spans
        .retain(|s| !matches!(s.kind, InlineKind::Citation { .. }));
    header.render_markdown_with_links_indexed(&body, links, ordered_index)
}

fn markdown_figure_block(figure: &FigureRef) -> String {
    let mut body = String::new();
    let _ = write!(
        body,
        "![{}](media:{})",
        markdown_escape_alt(&figure.alt_text),
        figure.image_chunk_id
    );
    markdown_wrap_title_caption(figure.title.as_deref(), &body, figure.caption.as_deref())
}

fn markdown_cite_block(cite: &CitePayload, bib: &BibEntry) -> String {
    let label = cite
        .label
        .as_deref()
        .filter(|s| !s.is_empty())
        .unwrap_or(bib.cite_key.as_str());
    let label = if label.is_empty() { "unknown" } else { label };
    format_pandoc_cite(label)
}

fn markdown_quote_block(cite: &CitePayload) -> String {
    let mut block = String::new();
    for line in cite.quote.lines() {
        if line.is_empty() {
            block.push('>');
        } else {
            let _ = write!(block, "> {line}");
        }
        block.push('\n');
    }
    block.trim_end().to_owned()
}

fn markdown_ref_block(cite: &CitePayload) -> String {
    let mut parts = Vec::new();
    if let Some(label) = cite.label.as_deref().filter(|s| !s.is_empty()) {
        parts.push(label.to_owned());
    }
    if let Some(doc) = cite.target_doc_id.as_deref() {
        parts.push(format!("doc:{doc}"));
    }
    if let Some(chunk) = cite.target_chunk_id {
        parts.push(format!("chunk:{chunk}"));
    }
    format!("[ref: {}]", parts.join(" "))
}

fn markdown_slide_block(slide: &SlidePayload) -> String {
    let mut block = format!("<!-- slide layout={} -->", slide.layout_id);
    for region in &slide.regions {
        let _ = write!(block, "\n[{}]: chunk-{}", region.name, region.chunk_id);
    }
    block
}

fn markdown_attachment_block(att: &AttachmentPayload) -> String {
    let mut block = format!(
        "*Attachment:* `{}` (`{}`)",
        att.filename.replace('`', "'"),
        att.media_type
    );
    if let Some(caption) = att.caption.as_deref() {
        block.push_str(" — ");
        block.push_str(caption.trim());
    }
    block
}

fn append_markdown_references(parts: &mut Vec<String>, bib_items: &mut [(usize, BibEntry)]) {
    if bib_items.is_empty() {
        return;
    }
    bib_items.sort_by_key(|(n, _)| *n);
    let mut refs = String::from("## References\n");
    for (n, entry) in bib_items.iter() {
        let _ = writeln!(refs, "{}", format_numeric_reference(*n, entry));
    }
    parts.push(refs.trim_end().to_owned());
}
