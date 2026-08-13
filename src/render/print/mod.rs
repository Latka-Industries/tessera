//! Build ariadnes-weave print IR from `.tes` reading-order chunks (THI-290).
//!
//! Bridge only: `.tes` → [`PrintDocument`]. Native PDF emit is THI-294
//! (`tes export --pdf --backend native`). Inline [`InlineKind::Font`](crate::catalog::InlineKind::Font)
//! maps to weave `TextRun.face` (pack pins loaded at emit, not here).

mod blocks;
mod cite;
mod lists;
mod lof;
mod runs;
mod toc;

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
use crate::render::floats::{FloatListKind, collect_figures, collect_tables};
use crate::render::toc::collect_headings;

use blocks::{map_figure, map_layout, map_slide, map_text_block};
use cite::{append_print_references, push_cite_block};
use lists::{PendingListItem, flush_list, push_list_item};
use lof::expand_float_list_print;
use toc::expand_toc_print;

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
    PrintBlock::paragraph(vec![run])
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

/// Print IR destination id for a heading chunk (TOC `GoTo` + PDF outline; THI-390/393).
#[must_use]
pub(crate) fn heading_dest_id(chunk_id: u64) -> String {
    format!("h-{chunk_id}")
}

/// One list-nav line (`TocEntry`) with optional page resolve and leaders.
pub(super) fn push_list_nav_entry(
    blocks: &mut Vec<PrintBlock>,
    title_text: impl Into<String>,
    dest_id: Option<String>,
    indent: u32,
    pages: bool,
    leaders: bool,
) {
    let mut run = TextRun::plain(title_text.into());
    run.style = InlineStyle {
        link: true,
        ..InlineStyle::default()
    };
    // `None` → weave resolves page digits from `dest_id`.
    // `Some("")` → no page column (pages explicitly off).
    let page_label = if pages { None } else { Some(String::new()) };
    blocks.push(PrintBlock::toc_entry_leaders(
        vec![run],
        page_label,
        dest_id,
        indent,
        leaders,
    ));
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

    let headings = collect_headings(file, entries)?;
    let figures = collect_figures(file, entries)?;
    let tables = collect_tables(file, entries)?;

    let mut blocks = Vec::new();
    let mut list_buf: Vec<PendingListItem> = Vec::new();
    let mut bib_items: Vec<(usize, BibEntry)> = Vec::new();
    // Open `\columns` region: count, gap, children (THI-391). Soft-flush at EOF.
    let mut columns: Option<(u8, Option<u16>, Vec<PrintBlock>)> = None;

    for entry in entries {
        match entry.chunk_type {
            ChunkType::Text => push_text_entry(
                file,
                entry,
                profile,
                cite,
                &headings,
                &figures,
                &tables,
                &mut blocks,
                &mut list_buf,
                &mut columns,
            )?,
            ChunkType::Figure => {
                flush_list(columns_sink(&mut blocks, &mut columns), &mut list_buf);
                push_block(&mut blocks, &mut columns, map_figure(file, entry)?);
            }
            ChunkType::Slide => {
                flush_list(columns_sink(&mut blocks, &mut columns), &mut list_buf);
                push_block(&mut blocks, &mut columns, map_slide(file, entry)?);
            }
            ChunkType::Layout => {
                flush_list(columns_sink(&mut blocks, &mut columns), &mut list_buf);
                push_block(&mut blocks, &mut columns, map_layout(file, entry)?);
            }
            ChunkType::Cite => {
                flush_list(columns_sink(&mut blocks, &mut columns), &mut list_buf);
                let mut cite_blocks = Vec::new();
                push_cite_block(file, entry, &cite_numbers, &mut cite_blocks, &mut bib_items)?;
                push_blocks(&mut blocks, &mut columns, cite_blocks);
            }
            ChunkType::Attachment => {
                flush_list(columns_sink(&mut blocks, &mut columns), &mut list_buf);
            }
            _ => {}
        }
    }
    flush_list(columns_sink(&mut blocks, &mut columns), &mut list_buf);
    flush_columns(&mut blocks, &mut columns);
    append_print_references(&mut blocks, &mut bib_items);
    Ok(blocks)
}

#[allow(clippy::too_many_arguments)]
fn push_text_entry(
    file: &TesFile,
    entry: &ChunkIndexEntry,
    profile: &PrintProfileId,
    cite: CiteProj<'_>,
    headings: &[crate::render::toc::TocHeading],
    figures: &[crate::render::floats::FloatCandidate],
    tables: &[crate::render::floats::FloatCandidate],
    blocks: &mut Vec<PrintBlock>,
    list_buf: &mut Vec<PendingListItem>,
    columns: &mut Option<(u8, Option<u16>, Vec<PrintBlock>)>,
) -> Result<()> {
    let (header, body) = decode_text_entry(file, entry)?;
    if header.role == TextRole::Columns {
        flush_list(columns_sink(blocks, columns), list_buf);
        flush_columns(blocks, columns);
        *columns = Some((
            header.columns_count_or_default(),
            header.columns_gap,
            Vec::new(),
        ));
        return Ok(());
    }
    if header.role == TextRole::ColumnsEnd {
        flush_list(columns_sink(blocks, columns), list_buf);
        flush_columns(blocks, columns);
        return Ok(());
    }
    if header.role == TextRole::ListItem {
        push_list_item(
            columns_sink(blocks, columns),
            list_buf,
            &header,
            &body,
            cite,
            file.links(),
        );
        return Ok(());
    }

    flush_list(columns_sink(blocks, columns), list_buf);
    if header.role == TextRole::Toc {
        push_blocks(blocks, columns, expand_toc_print(&header, headings));
        return Ok(());
    }
    if header.role.is_float_list() {
        let (candidates, kind) = if header.role == TextRole::Lof {
            (figures, FloatListKind::Figures)
        } else {
            (tables, FloatListKind::Tables)
        };
        push_blocks(
            blocks,
            columns,
            expand_float_list_print(&header, candidates, kind),
        );
        return Ok(());
    }
    if let Some(title) = nonempty_label(header.title.as_deref()) {
        push_block(blocks, columns, title_paragraph(title));
    }
    push_block(
        blocks,
        columns,
        map_text_block(entry.chunk_id, &header, &body, profile, cite, file.links()),
    );
    if let Some(caption) = nonempty_label(header.caption.as_deref()) {
        push_block(blocks, columns, caption_paragraph(caption));
    }
    Ok(())
}

fn columns_sink<'a>(
    blocks: &'a mut Vec<PrintBlock>,
    columns: &'a mut Option<(u8, Option<u16>, Vec<PrintBlock>)>,
) -> &'a mut Vec<PrintBlock> {
    if let Some((_, _, children)) = columns {
        children
    } else {
        blocks
    }
}

fn push_block(
    blocks: &mut Vec<PrintBlock>,
    columns: &mut Option<(u8, Option<u16>, Vec<PrintBlock>)>,
    block: PrintBlock,
) {
    columns_sink(blocks, columns).push(block);
}

fn push_blocks(
    blocks: &mut Vec<PrintBlock>,
    columns: &mut Option<(u8, Option<u16>, Vec<PrintBlock>)>,
    extra: Vec<PrintBlock>,
) {
    columns_sink(blocks, columns).extend(extra);
}

fn flush_columns(
    blocks: &mut Vec<PrintBlock>,
    columns: &mut Option<(u8, Option<u16>, Vec<PrintBlock>)>,
) {
    if let Some((count, gap, children)) = columns.take() {
        blocks.push(PrintBlock::columns(count, gap, children));
    }
}
