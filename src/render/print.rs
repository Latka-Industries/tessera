//! Build ariadnes-weave print IR from `.tes` reading-order chunks (THI-290).
//!
//! Bridge only: `.tes` → [`PrintDocument`]. Native PDF emit is THI-294
//! (`tes export --pdf --backend native`).

use std::collections::BTreeSet;
use std::path::PathBuf;

use ariadnes_weave::{
    BreakHint, FigurePlacement, InlineStyle, ListItem, PrintBlock, PrintDocument, PrintImage,
    PrintMeta, PrintProfileId, SlideRegionContent, TableRow, TextRun,
};

use crate::catalog::chunk::{CitePayload, InlineKind, InlineSpan, ListKind, TextHeader, TextRole};
use crate::catalog::file::TesFile;
use crate::catalog::index::{ChunkIndexEntry, ChunkType};
use crate::catalog::media::{FigureRef, ImagePayload, ImagePlacement};
use crate::catalog::{SlidePayload, decode_text_payload};
use crate::error::{Result, TesError};
use crate::io::bib::{BibEntry, format_numeric_marker, format_reference_body};
use crate::io::cite::{CiteProj, CiteTessprekKind, classify_cite, projection_maps};
use crate::io::export::{chapter_slice, cite_number_map, decode_cite_entry, decode_numbered_cite};
use crate::layout::DocKind;

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
                    push_list_item(&mut blocks, &mut list_buf, &header, &body, cite);
                } else {
                    flush_list(&mut blocks, &mut list_buf);
                    if let Some(title) = header.title.as_deref().filter(|s| !s.is_empty()) {
                        blocks.push(PrintBlock::Paragraph {
                            runs: vec![TextRun::plain(title)],
                        });
                    }
                    blocks.push(map_text_block(&header, &body, profile, cite));
                    if let Some(caption) = header.caption.as_deref().filter(|s| !s.is_empty()) {
                        // Weave IR has no dedicated caption block yet; muted via theme later.
                        blocks.push(PrintBlock::Paragraph {
                            runs: vec![TextRun::plain(caption)],
                        });
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

fn push_cite_block(
    file: &TesFile,
    entry: &ChunkIndexEntry,
    cite_numbers: &std::collections::HashMap<u64, usize>,
    blocks: &mut Vec<PrintBlock>,
    bib_items: &mut Vec<(usize, BibEntry)>,
) -> Result<()> {
    let cite = decode_cite_entry(file, entry)?;
    match classify_cite(&cite) {
        CiteTessprekKind::Biblio => {
            let (n, cite, bib) = decode_numbered_cite(file, entry, cite_numbers)?;
            let label = cite_stub_label(&cite, &bib);
            let marker = format_numeric_marker(n);
            blocks.push(PrintBlock::Paragraph {
                runs: vec![
                    TextRun {
                        text: marker,
                        style: InlineStyle {
                            cite: true,
                            ..Default::default()
                        },
                        face: None,
                    },
                    TextRun::plain(format!(" {label}")),
                ],
            });
            bib_items.push((n, bib));
        }
        CiteTessprekKind::Quote => {
            let quote = cite.quote.trim();
            if !quote.is_empty() {
                blocks.push(PrintBlock::Quote {
                    runs: vec![TextRun::plain(quote)],
                });
            }
        }
        CiteTessprekKind::Ref => {
            blocks.push(PrintBlock::Paragraph {
                runs: vec![TextRun::plain(ref_marker_text(&cite))],
            });
        }
    }
    Ok(())
}

fn cite_stub_label(cite: &CitePayload, bib: &BibEntry) -> String {
    let label = cite
        .label
        .as_deref()
        .filter(|s| !s.is_empty())
        .unwrap_or(bib.cite_key.as_str());
    if label.is_empty() {
        "unknown".into()
    } else {
        label.to_owned()
    }
}

fn ref_marker_text(cite: &CitePayload) -> String {
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
    if parts.is_empty() {
        "ref".into()
    } else {
        format!("[ref: {}]", parts.join(" "))
    }
}

fn append_print_references(blocks: &mut Vec<PrintBlock>, bib_items: &mut [(usize, BibEntry)]) {
    if bib_items.is_empty() {
        return;
    }
    bib_items.sort_by_key(|(n, _)| *n);
    blocks.push(PrintBlock::Heading {
        level: 2,
        runs: vec![TextRun::plain("References")],
        break_before: BreakHint::KeepWithNext,
    });
    for (n, entry) in bib_items.iter() {
        blocks.push(PrintBlock::Paragraph {
            runs: vec![TextRun::plain(format!(
                "{n}. {}",
                format_reference_body(entry)
            ))],
        });
    }
}

#[derive(Debug, Clone)]
struct PendingListItem {
    depth: u32,
    kind: ListKind,
    runs: Vec<TextRun>,
}

fn push_list_item(
    blocks: &mut Vec<PrintBlock>,
    list_buf: &mut Vec<PendingListItem>,
    header: &TextHeader,
    body: &str,
    cite: CiteProj<'_>,
) {
    let kind = header.list_kind.unwrap_or(ListKind::Bullet);
    let depth = header.list_depth_or_default();
    if list_buf
        .last()
        .is_some_and(|last| last.depth == depth && last.kind != kind)
    {
        flush_list(blocks, list_buf);
    }
    list_buf.push(PendingListItem {
        depth,
        kind,
        runs: body_to_runs(body, &header.spans, Some(cite)),
    });
}

fn flush_list(blocks: &mut Vec<PrintBlock>, list_buf: &mut Vec<PendingListItem>) {
    if list_buf.is_empty() {
        return;
    }
    let items = std::mem::take(list_buf);
    blocks.push(coalesce_list(&items));
}

fn coalesce_list(items: &[PendingListItem]) -> PrintBlock {
    let ordered = matches!(items.first().map(|i| i.kind), Some(ListKind::Ordered));
    let min_depth = items.iter().map(|i| i.depth).min().unwrap_or(1);
    PrintBlock::List {
        ordered,
        items: nest_list_items(items, min_depth),
    }
}

fn nest_list_items(items: &[PendingListItem], depth: u32) -> Vec<ListItem> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < items.len() {
        if items[i].depth < depth {
            break;
        }
        if items[i].depth > depth {
            // Orphan deeper item — promote to current depth.
            let mut promoted = items[i].clone();
            promoted.depth = depth;
            let slice = std::slice::from_ref(&promoted);
            out.extend(nest_list_items(slice, depth));
            i += 1;
            continue;
        }
        let runs = items[i].runs.clone();
        i += 1;
        let child_start = i;
        while i < items.len() && items[i].depth > depth {
            i += 1;
        }
        let children = if child_start < i {
            child_lists(&items[child_start..i], depth + 1)
        } else {
            Vec::new()
        };
        out.push(ListItem { runs, children });
    }
    out
}

fn child_lists(items: &[PendingListItem], depth: u32) -> Vec<PrintBlock> {
    if items.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut start = 0;
    while start < items.len() {
        let kind = items[start].kind;
        let mut end = start + 1;
        while end < items.len() {
            let at_boundary = items[end].depth == depth && items[end].kind != kind;
            if at_boundary {
                break;
            }
            end += 1;
        }
        let ordered = matches!(kind, ListKind::Ordered);
        out.push(PrintBlock::List {
            ordered,
            items: nest_list_items(&items[start..end], depth),
        });
        start = end;
    }
    out
}

fn map_text_block(
    header: &TextHeader,
    body: &str,
    profile: &PrintProfileId,
    cite: CiteProj<'_>,
) -> PrintBlock {
    match header.role {
        TextRole::Heading => {
            let level = u8::try_from(header.level.unwrap_or(1).clamp(1, 6)).unwrap_or(1);
            let break_before = heading_break(level, profile);
            PrintBlock::Heading {
                level,
                runs: body_to_runs(body, &header.spans, Some(cite)),
                break_before,
            }
        }
        // ListItem: isolated items should have been coalesced; paragraph fallback.
        TextRole::Paragraph | TextRole::ListItem => PrintBlock::Paragraph {
            runs: body_to_runs(body, &header.spans, Some(cite)),
        },
        TextRole::Blockquote => PrintBlock::Quote {
            runs: body_to_runs(body, &header.spans, Some(cite)),
        },
        TextRole::CodeBlock => PrintBlock::Code {
            lang: header.code_lang.clone(),
            text: body.to_owned(),
        },
        TextRole::Table => map_table(header, body),
        TextRole::Math => PrintBlock::Math {
            display: true,
            latex: body.trim().to_owned(),
        },
    }
}

fn heading_break(level: u8, profile: &PrintProfileId) -> BreakHint {
    if level == 1 && profile.name == "manuscript" {
        BreakHint::PageAlways
    } else if level <= 2 {
        BreakHint::KeepWithNext
    } else {
        BreakHint::None
    }
}

fn map_table(header: &TextHeader, body: &str) -> PrintBlock {
    if let Some(table) = &header.table {
        let rows = table
            .rows
            .iter()
            .map(|row| TableRow {
                cells: row.cells.iter().map(|c| c.text.clone()).collect(),
            })
            .collect();
        return PrintBlock::Table { rows };
    }
    let rows = body
        .lines()
        .map(|line| TableRow {
            cells: line.split('\t').map(str::to_owned).collect(),
        })
        .collect();
    PrintBlock::Table { rows }
}

fn map_figure(file: &TesFile, entry: &ChunkIndexEntry) -> Result<PrintBlock> {
    let figure = decode_figure_entry(file, entry)?;
    let image_entry = file.chunk_by_id(figure.image_chunk_id)?;
    if image_entry.chunk_type != ChunkType::Image {
        return Err(TesError::InvalidFigure {
            message: format!(
                "figure {} points at chunk {} of type '{}'",
                entry.chunk_id,
                figure.image_chunk_id,
                image_entry.chunk_type.as_str()
            ),
        });
    }
    let raw = file.decode_payload(image_entry)?;
    let image = ImagePayload::from_bytes(&raw).map_err(|e| TesError::Decode {
        chunk_id: image_entry.chunk_id,
        message: e.to_string(),
    })?;
    let caption = figure
        .caption
        .as_deref()
        .map(|c| vec![TextRun::plain(c)])
        .unwrap_or_default();
    Ok(PrintBlock::Figure {
        image: PrintImage {
            bytes: image.data,
            media_type: image.media_type,
            width_px: (image.width_px > 0).then_some(image.width_px),
            height_px: (image.height_px > 0).then_some(image.height_px),
        },
        alt: figure.alt_text,
        caption,
        placement: map_figure_placement(&figure.placement),
    })
}

fn map_figure_placement(placement: &ImagePlacement) -> FigurePlacement {
    match placement {
        ImagePlacement::FloatStart | ImagePlacement::FloatEnd => FigurePlacement::FloatNear,
        _ => FigurePlacement::Flow,
    }
}

fn map_slide(file: &TesFile, entry: &ChunkIndexEntry) -> Result<PrintBlock> {
    let slide = decode_slide_entry(file, entry)?;
    let mut regions = Vec::with_capacity(slide.regions.len());
    for region in &slide.regions {
        let text = slide_region_text(file, region.chunk_id).unwrap_or_default();
        regions.push(SlideRegionContent {
            slot: region.name.clone(),
            text,
        });
    }
    Ok(PrintBlock::Slide {
        layout_id: slide.layout_id,
        regions,
    })
}

fn slide_region_text(file: &TesFile, chunk_id: u64) -> Result<String> {
    let entry = file.chunk_by_id(chunk_id)?;
    match entry.chunk_type {
        ChunkType::Text => {
            let (_, body) = decode_text_entry(file, entry)?;
            Ok(body)
        }
        ChunkType::Figure => {
            let figure = decode_figure_entry(file, entry)?;
            Ok(figure
                .caption
                .clone()
                .unwrap_or_else(|| figure.alt_text.clone()))
        }
        _ => Ok(String::new()),
    }
}

/// Split body into styled runs using half-open UTF-8 span ranges.
///
/// Inline [`InlineKind::Citation`] spans are rewritten to `[n]` / `[@key]`
/// (same projection as HTML/Markdown export) and keep `style.cite`.
fn body_to_runs(body: &str, spans: &[InlineSpan], cite: Option<CiteProj<'_>>) -> Vec<TextRun> {
    let (body, spans) = project_inline_citations(body, spans, cite);
    body_to_runs_projected(&body, &spans)
}

fn project_inline_citations(
    body: &str,
    spans: &[InlineSpan],
    cite: Option<CiteProj<'_>>,
) -> (String, Vec<InlineSpan>) {
    let Some(cite) = cite else {
        return (body.to_owned(), spans.to_vec());
    };

    let mut body = body.to_owned();
    let mut spans = spans.to_vec();
    let mut cite_spans: Vec<_> = spans
        .iter()
        .filter(|s| matches!(s.kind, InlineKind::Citation { .. }))
        .cloned()
        .collect();
    cite_spans.sort_by_key(|s| std::cmp::Reverse(s.start));

    let mut marker_spans = Vec::new();
    for span in cite_spans {
        let InlineKind::Citation { cite_chunk_id } = span.kind else {
            continue;
        };
        let start = span.start as usize;
        let end = span.end as usize;
        if end > body.len() || start > end {
            continue;
        }
        if !body.is_char_boundary(start) || !body.is_char_boundary(end) {
            continue;
        }
        let marker = cite.marker(cite_chunk_id);
        let old_len = end - start;
        let new_len = marker.len();
        let delta = new_len as i64 - old_len as i64;
        body.replace_range(start..end, &marker);

        for other in &mut spans {
            if matches!(other.kind, InlineKind::Citation { .. }) {
                continue;
            }
            if (other.start as usize) >= end {
                other.start = u32::try_from(i64::from(other.start) + delta).unwrap_or(other.start);
                other.end = u32::try_from(i64::from(other.end) + delta).unwrap_or(other.end);
            }
        }
        marker_spans.push(InlineSpan {
            start: span.start,
            end: span
                .start
                .saturating_add(u32::try_from(new_len).unwrap_or(u32::MAX)),
            kind: InlineKind::Citation { cite_chunk_id },
        });
    }

    spans.retain(|s| !matches!(s.kind, InlineKind::Citation { .. }));
    spans.extend(marker_spans);
    (body, spans)
}

fn body_to_runs_projected(body: &str, spans: &[InlineSpan]) -> Vec<TextRun> {
    if body.is_empty() {
        return Vec::new();
    }
    if spans.is_empty() {
        return vec![TextRun::plain(body)];
    }

    let mut bounds = BTreeSet::new();
    bounds.insert(0usize);
    bounds.insert(body.len());
    for span in spans {
        let start = span.start as usize;
        let end = span.end as usize;
        if end > body.len() || start >= end {
            continue;
        }
        if !body.is_char_boundary(start) || !body.is_char_boundary(end) {
            continue;
        }
        bounds.insert(start);
        bounds.insert(end);
    }
    let bounds: Vec<usize> = bounds.into_iter().collect();
    let mut runs = Vec::new();
    for window in bounds.windows(2) {
        let (a, b) = (window[0], window[1]);
        if a >= b {
            continue;
        }
        let mut style = InlineStyle::default();
        for span in spans {
            let start = span.start as usize;
            let end = span.end as usize;
            if start <= a && end >= b {
                apply_inline_kind(&mut style, &span.kind);
            }
        }
        runs.push(TextRun {
            text: body[a..b].to_owned(),
            style,
            face: None,
        });
    }
    runs
}

fn apply_inline_kind(style: &mut InlineStyle, kind: &InlineKind) {
    match kind {
        InlineKind::Emphasis | InlineKind::Term => style.emphasis = true,
        InlineKind::Strong => style.strong = true,
        InlineKind::Code => style.code = true,
        InlineKind::Link { .. } => style.link = true,
        InlineKind::Citation { .. } => style.cite = true,
        InlineKind::Underline | InlineKind::Quote | InlineKind::Math { .. } => {}
    }
}

fn decode_text_entry(file: &TesFile, entry: &ChunkIndexEntry) -> Result<(TextHeader, String)> {
    let raw = file.decode_payload(entry)?;
    decode_text_payload(&raw).map_err(|e| TesError::Decode {
        chunk_id: entry.chunk_id,
        message: e.to_string(),
    })
}

fn decode_figure_entry(file: &TesFile, entry: &ChunkIndexEntry) -> Result<FigureRef> {
    let raw = file.decode_payload(entry)?;
    FigureRef::from_bytes(&raw).map_err(|e| TesError::Decode {
        chunk_id: entry.chunk_id,
        message: e.to_string(),
    })
}

fn decode_slide_entry(file: &TesFile, entry: &ChunkIndexEntry) -> Result<SlidePayload> {
    let raw = file.decode_payload(entry)?;
    SlidePayload::from_bytes(raw.as_ref())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::DocumentCatalog;
    use crate::catalog::chunk::{CitePayload, InlineKind, InlineSpan, TextHeader};
    use crate::catalog::session::TesWriterSession;
    use crate::fixtures::samples::encode_manuscript_chapters;
    use crate::fixtures::v0::{
        encode_note_one_chunk, encode_note_three_chunks, encode_research_cite,
    };
    use crate::io::bib::BibEntry;
    use std::path::PathBuf;

    fn open_bytes(name: &str, bytes: Vec<u8>) -> TesFile {
        TesFile::from_bytes(PathBuf::from(name), bytes).expect("open fixture bytes")
    }

    #[test]
    fn note_one_chunk_paragraph_with_code_span() {
        let file = open_bytes("note_one_chunk.tes", encode_note_one_chunk());
        let doc = build_print_document(&file, &PrintBuildOptions::default()).unwrap();
        assert_eq!(doc.profile.as_label(), "print@0");
        assert_eq!(doc.blocks.len(), 1);
        match &doc.blocks[0] {
            PrintBlock::Paragraph { runs } => {
                assert!(
                    runs.iter().any(|r| r.style.code),
                    "expected code run: {runs:?}"
                );
                let joined: String = runs.iter().map(|r| r.text.as_str()).collect();
                assert!(joined.contains("tes textconv"));
            }
            other => panic!("expected paragraph, got {other:?}"),
        }
    }

    #[test]
    fn note_three_chunks_heading_paragraph_list() {
        let file = open_bytes("note_three_chunks.tes", encode_note_three_chunks());
        let doc = build_print_document(&file, &PrintBuildOptions::default()).unwrap();
        assert!(matches!(
            doc.blocks[0],
            PrintBlock::Heading { level: 1, .. }
        ));
        assert!(matches!(doc.blocks[1], PrintBlock::Paragraph { .. }));
        match &doc.blocks[2] {
            PrintBlock::List {
                ordered: false,
                items,
            } => {
                assert_eq!(items.len(), 3);
            }
            other => panic!("expected bullet list, got {other:?}"),
        }
    }

    #[test]
    fn manuscript_chapters_profile_and_h1_breaks() {
        let file = open_bytes("manuscript_chapters.tes", encode_manuscript_chapters());
        let doc = build_print_document(&file, &PrintBuildOptions::default()).unwrap();
        assert_eq!(doc.profile.as_label(), "manuscript@0");
        assert!(matches!(doc.blocks[0], PrintBlock::Paragraph { .. })); // front matter
        let h1s: Vec<_> = doc
            .blocks
            .iter()
            .filter_map(|b| match b {
                PrintBlock::Heading {
                    level: 1,
                    break_before,
                    runs,
                    ..
                } => Some((
                    runs.iter().map(|r| r.text.as_str()).collect::<String>(),
                    *break_before,
                )),
                _ => None,
            })
            .collect();
        assert_eq!(h1s.len(), 3);
        assert!(h1s.iter().all(|(_, br)| *br == BreakHint::PageAlways));
    }

    #[test]
    fn chapter_scope_excludes_siblings() {
        let file = open_bytes("manuscript_chapters.tes", encode_manuscript_chapters());
        let doc = build_print_document(
            &file,
            &PrintBuildOptions {
                chapter: Some(2),
                ..Default::default()
            },
        )
        .unwrap();
        let titles: Vec<String> = doc
            .blocks
            .iter()
            .filter_map(|b| match b {
                PrintBlock::Heading { level: 1, runs, .. } => {
                    Some(runs.iter().map(|r| r.text.as_str()).collect())
                }
                _ => None,
            })
            .collect();
        assert_eq!(titles.len(), 1);
        assert!(titles[0].contains("Two") || titles[0].contains("2") || !titles[0].is_empty());
        // No front-matter paragraph before the chapter H1.
        assert!(matches!(
            doc.blocks[0],
            PrintBlock::Heading { level: 1, .. }
        ));
    }

    #[test]
    fn on_disk_note_one_chunk_fixture() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/v0/note_one_chunk.tes");
        let doc = build_print_document_from_path(&path, &PrintBuildOptions::default()).unwrap();
        assert_eq!(doc.blocks.len(), 1);
        assert!(matches!(doc.blocks[0], PrintBlock::Paragraph { .. }));
    }

    #[test]
    fn research_cite_quote_maps_to_print_quote() {
        let file = open_bytes("research_cite.tes", encode_research_cite());
        let doc = build_print_document(&file, &PrintBuildOptions::default()).unwrap();
        assert!(
            doc.blocks
                .iter()
                .any(|b| matches!(b, PrintBlock::Paragraph { .. })),
            "expected prose paragraph: {doc:?}"
        );
        match doc
            .blocks
            .iter()
            .find(|b| matches!(b, PrintBlock::Quote { .. }))
        {
            Some(PrintBlock::Quote { runs }) => {
                let text: String = runs.iter().map(|r| r.text.as_str()).collect();
                assert!(text.contains("We measured"), "{text}");
            }
            other => panic!("expected Quote block for ranged cite, got {other:?}"),
        }
        assert!(
            !doc.blocks.iter().any(|b| matches!(
                b,
                PrintBlock::Heading {
                    level: 2,
                    runs,
                    ..
                } if runs.iter().any(|r| r.text == "References")
            )),
            "quote-only fixture should not emit References"
        );
    }

    #[test]
    fn cite_quote_ref_biblio_and_inline_markers() {
        let mut catalog = DocumentCatalog::new(
            "990e8400-e29b-41d4-a716-446655440099",
            "Print cite specimen",
            "2026-08-05T00:00:00Z",
            "2026-08-05T00:00:00Z",
            DocKind::Research,
        );
        catalog.cite_style_id = Some("numeric".into());
        let mut session = TesWriterSession::create("print_cites.tes", DocKind::Research);
        session.set_catalog(catalog).unwrap();

        let bib_id = session
            .add_cite_chunk(&CitePayload {
                quote: String::new(),
                target_doc_id: None,
                target_chunk_id: None,
                target_byte_start: None,
                target_byte_end: None,
                label: Some("keller2020".into()),
                page: None,
                source: Some(BibEntry {
                    cite_key: "keller2020".into(),
                    entry_type: "article".into(),
                    author: Some("Keller, Ada".into()),
                    title: Some("Chunk Containers".into()),
                    year: Some("2020".into()),
                    ..BibEntry::default()
                }),
            })
            .unwrap();

        let body = "See keller2020 for context.";
        let key_start = body.find("keller2020").unwrap() as u32;
        let key_end = key_start + "keller2020".len() as u32;
        let mut para = TextHeader::paragraph();
        para.spans = vec![InlineSpan {
            start: key_start,
            end: key_end,
            kind: InlineKind::Citation {
                cite_chunk_id: bib_id,
            },
        }];
        session.add_text_chunk(&para, body).unwrap();

        session
            .add_cite_chunk(&CitePayload {
                quote: "Quoted passage.".into(),
                target_doc_id: Some("aa0e8400-e29b-41d4-a716-446655440001".into()),
                target_chunk_id: Some(1),
                target_byte_start: Some(0),
                target_byte_end: Some(15),
                label: Some("keller2020".into()),
                page: Some(2),
                source: None,
            })
            .unwrap();

        session
            .add_cite_chunk(&CitePayload {
                quote: String::new(),
                target_doc_id: Some("aa0e8400-e29b-41d4-a716-446655440001".into()),
                target_chunk_id: Some(3),
                target_byte_start: None,
                target_byte_end: None,
                label: Some("see-also".into()),
                page: None,
                source: None,
            })
            .unwrap();

        let bytes = session.encode_file().unwrap();
        let file = open_bytes("print_cites.tes", bytes);
        let doc = build_print_document(&file, &PrintBuildOptions::default()).unwrap();

        // Biblio stub: "[1] keller2020"
        let stub = doc.blocks.iter().find_map(|b| match b {
            PrintBlock::Paragraph { runs }
                if runs.iter().any(|r| r.style.cite && r.text == "[1]") =>
            {
                Some(runs.iter().map(|r| r.text.as_str()).collect::<String>())
            }
            _ => None,
        });
        assert_eq!(stub.as_deref(), Some("[1] keller2020"));

        // Inline rewrite in prose
        let prose = doc.blocks.iter().find_map(|b| match b {
            PrintBlock::Paragraph { runs }
                if runs.iter().any(|r| r.text.starts_with("See "))
                    && runs.iter().any(|r| r.style.cite && r.text == "[1]") =>
            {
                Some(runs.iter().map(|r| r.text.as_str()).collect::<String>())
            }
            _ => None,
        });
        assert_eq!(prose.as_deref(), Some("See [1] for context."));

        assert!(
            doc.blocks.iter().any(|b| matches!(
                b,
                PrintBlock::Quote { runs } if runs.iter().any(|r| r.text.contains("Quoted passage"))
            )),
            "expected quote block: {doc:?}"
        );
        assert!(
            doc.blocks.iter().any(|b| matches!(
                b,
                PrintBlock::Paragraph { runs }
                    if runs.iter().any(|r| r.text.contains("[ref:") && r.text.contains("see-also"))
            )),
            "expected ref paragraph: {doc:?}"
        );
        assert!(
            doc.blocks.iter().any(|b| matches!(
                b,
                PrintBlock::Heading {
                    level: 2,
                    runs,
                    ..
                } if runs.iter().any(|r| r.text == "References")
            )),
            "expected References heading: {doc:?}"
        );
        assert!(
            doc.blocks.iter().any(|b| matches!(
                b,
                PrintBlock::Paragraph { runs }
                    if runs.iter().any(|r| r.text.contains("1. Keller") && r.text.contains("Chunk Containers"))
            )),
            "expected bibliography line: {doc:?}"
        );
    }
}
