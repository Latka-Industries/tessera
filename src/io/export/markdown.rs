//! Lossy GFM-ish Markdown projection (`--markdown`).

use std::fmt::Write as _;

use crate::catalog::chunk::{CitePayload, OrderedListNumbering, TextHeader};
use crate::catalog::file::TesFile;
use crate::catalog::index::{ChunkIndexEntry, ChunkType};
use crate::catalog::media::{AttachmentPayload, FigureRef};
use crate::error::Result;
use crate::io::bib::{BibEntry, format_numeric_reference, format_pandoc_cite};

use super::ExportOptions;
use super::common::{
    cite_number_map, decode_attachment_entry, decode_figure_entry, decode_numbered_cite,
    decode_slide_entry, decode_text_entry, markdown_escape_alt, selected_content_entries,
};

pub(super) fn export_markdown(file: &TesFile, options: &ExportOptions) -> Result<String> {
    let entries = selected_content_entries(file, options)?;
    let cite_numbers = cite_number_map(file, &entries)?;
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
            let (n, cite, bib) = decode_numbered_cite(file, entry, cite_numbers)?;
            parts.push(markdown_cite_block(&cite, &bib));
            bib_items.push((n, bib));
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
    links: &[crate::catalog::LinkEntry],
    ordered_index: Option<u32>,
) -> String {
    let rendered = header.render_markdown_with_links_indexed(body, links, ordered_index);
    markdown_wrap_title_caption(
        header.title.as_deref(),
        &rendered,
        header.caption.as_deref(),
    )
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
    let mut block = format_pandoc_cite(label);
    if !cite.quote.trim().is_empty() {
        block.push(' ');
        block.push('"');
        block.push_str(cite.quote.trim());
        block.push('"');
    }
    block
}

fn markdown_slide_block(slide: &crate::catalog::slide::SlidePayload) -> String {
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
