//! Build ariadnes-weave print IR from `.tes` reading-order chunks (THI-290).
//!
//! Bridge only: `.tes` → [`PrintDocument`]. Native PDF emit is THI-294
//! (`tes export --pdf --backend native`). Inline [`InlineKind::Font`](crate::catalog::InlineKind::Font)
//! maps to weave `TextRun.face` (pack pins loaded at emit, not here).

mod blocks;
mod cite;
mod lists;
mod runs;

#[cfg(test)]
mod tests;

use std::path::PathBuf;

use ariadnes_weave::{InlineStyle, PrintBlock, PrintDocument, PrintMeta, PrintProfileId, TextRun};

use crate::catalog::chunk::TextRole;
use crate::catalog::file::TesFile;
use crate::catalog::index::{ChunkIndexEntry, ChunkType};
use crate::error::Result;
use crate::io::bib::BibEntry;
use crate::io::cite::{CiteProj, projection_maps};
use crate::io::export::{chapter_slice, cite_number_map, decode_text_entry};
use crate::layout::DocKind;

use blocks::{map_figure, map_layout, map_slide, map_text_block};
use cite::{append_print_references, push_cite_block};
use lists::{PendingListItem, flush_list, push_list_item};

/// Options for building a [`PrintDocument`] from a `.tes` file.
#[derive(Debug, Clone, Default)]
pub struct PrintBuildOptions {
    /// Restrict body to the Nth chapter (1-based H1 slice). Same rules as export `--chapter`.
    pub chapter: Option<u32>,
    /// Override automatic profile selection from `doc_kind`.
    pub profile: Option<PrintProfileId>,
}

/// Build print IR from an open [`TesFile`].
///
/// # Errors
///
/// Returns decode / scope errors from chunk payloads or chapter selection.
pub fn build_print_document(file: &TesFile, options: &PrintBuildOptions) -> Result<PrintDocument> {
    let profile = options
        .profile
        .clone()
        .unwrap_or_else(|| default_profile(file.superblock().doc_kind));
    let meta = print_meta(file);
    let entries = scoped_entries(file, options.chapter)?;
    let blocks = map_entries(file, &entries, &profile)?;
    Ok(PrintDocument {
        meta,
        profile,
        blocks,
    })
}

/// Open `path` and build print IR.
///
/// # Errors
///
/// Returns open errors from [`TesFile::open`] or [`build_print_document`].
pub fn build_print_document_from_path(
    path: impl Into<PathBuf>,
    options: &PrintBuildOptions,
) -> Result<PrintDocument> {
    let file = TesFile::open(path)?;
    build_print_document(&file, options)
}

fn default_profile(kind: DocKind) -> PrintProfileId {
    match kind {
        DocKind::Manuscript => PrintProfileId::manuscript_v0(),
        DocKind::Deck => PrintProfileId::deck_v0(),
        _ => PrintProfileId::print_v0(),
    }
}

fn print_meta(file: &TesFile) -> PrintMeta {
    let catalog = file.catalog();
    PrintMeta {
        title: catalog.map(|c| c.title.clone()).unwrap_or_default(),
        doc_kind: catalog.map_or_else(
            || file.superblock().doc_kind.as_str().to_owned(),
            |c| c.doc_kind.clone(),
        ),
        language: catalog.and_then(|c| c.language.clone()),
        source_doc_id: catalog.map(|c| c.doc_id.clone()),
    }
}

fn scoped_entries(file: &TesFile, chapter: Option<u32>) -> Result<Vec<&ChunkIndexEntry>> {
    let entries = file.reading_order_chunks();
    if let Some(chapter) = chapter {
        return chapter_slice(file, &entries, chapter);
    }
    Ok(entries)
}

fn single_run_paragraph(run: TextRun) -> PrintBlock {
    PrintBlock::Paragraph { runs: vec![run] }
}

fn emphasized_run(text: impl Into<String>) -> TextRun {
    TextRun {
        text: text.into(),
        style: InlineStyle {
            emphasis: true,
            ..InlineStyle::default()
        },
        face: None,
        link_uri: None,
    }
}

pub(crate) fn plain_paragraph(text: impl Into<String>) -> PrintBlock {
    single_run_paragraph(TextRun::plain(text))
}

/// Chunk `title` above a block — strong so it reads as a label, not body prose.
pub(crate) fn title_paragraph(text: impl Into<String>) -> PrintBlock {
    single_run_paragraph(TextRun::strong(text))
}

/// Non-figure chunk caption stand-in — italic `Paragraph` until a Caption IR exists.
/// Figure captions use `Figure.caption` + weave `[caption]` knobs instead.
pub(crate) fn caption_paragraph(text: impl Into<String>) -> PrintBlock {
    single_run_paragraph(emphasized_run(text))
}

pub(crate) fn nonempty_label(label: Option<&str>) -> Option<&str> {
    label.filter(|s| !s.is_empty())
}

fn map_entries(
    file: &TesFile,
    entries: &[&ChunkIndexEntry],
    profile: &PrintProfileId,
) -> Result<Vec<PrintBlock>> {
    let cite_numbers = cite_number_map(file, entries)?;
    let (cite_keys, cite_style) = projection_maps(file);
    let cite = CiteProj {
        numbers: &cite_numbers,
        keys: &cite_keys,
        style: cite_style,
    };

    let mut blocks = Vec::new();
    let mut list_buf: Vec<PendingListItem> = Vec::new();
    let mut bib_items: Vec<(usize, BibEntry)> = Vec::new();

    for entry in entries {
        match entry.chunk_type {
            ChunkType::Text => {
                let (header, body) = decode_text_entry(file, entry)?;
                if header.role == TextRole::ListItem {
                    push_list_item(
                        &mut blocks,
                        &mut list_buf,
                        &header,
                        &body,
                        cite,
                        file.links(),
                    );
                } else {
                    flush_list(&mut blocks, &mut list_buf);
                    if let Some(title) = nonempty_label(header.title.as_deref()) {
                        blocks.push(title_paragraph(title));
                    }
                    blocks.push(map_text_block(&header, &body, profile, cite, file.links()));
                    // Non-figure captions: no Caption IR yet (weave `[caption]` is figure-only).
                    if let Some(caption) = nonempty_label(header.caption.as_deref()) {
                        blocks.push(caption_paragraph(caption));
                    }
                }
            }
            ChunkType::Figure => {
                flush_list(&mut blocks, &mut list_buf);
                blocks.push(map_figure(file, entry)?);
            }
            ChunkType::Slide => {
                flush_list(&mut blocks, &mut list_buf);
                blocks.push(map_slide(file, entry)?);
            }
            ChunkType::Layout => {
                flush_list(&mut blocks, &mut list_buf);
                blocks.push(map_layout(file, entry)?);
            }
            ChunkType::Cite => {
                flush_list(&mut blocks, &mut list_buf);
                push_cite_block(file, entry, &cite_numbers, &mut blocks, &mut bib_items)?;
            }
            ChunkType::Attachment => {
                // Attachments are not prose print blocks.
                flush_list(&mut blocks, &mut list_buf);
            }
            _ => {}
        }
    }
    flush_list(&mut blocks, &mut list_buf);
    append_print_references(&mut blocks, &mut bib_items);
    Ok(blocks)
}
