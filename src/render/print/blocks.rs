//! Map text / figure / slide chunks to weave [`PrintBlock`]s.

use ariadnes_weave::{
    BreakHint, FigurePlacement, PrintBlock, PrintImage, PrintProfileId, SlideRegionContent,
    TableRow, TextRun,
};

use crate::catalog::chunk::{TextHeader, TextRole};
use crate::catalog::file::TesFile;
use crate::catalog::index::{ChunkIndexEntry, ChunkType};
use crate::catalog::media::{ImagePayload, ImagePlacement};
use crate::error::{Result, TesError};
use crate::io::cite::CiteProj;
use crate::io::export::{decode_figure_entry, decode_slide_entry, decode_text_entry};

use super::runs::body_to_runs;

pub(crate) fn map_text_block(
    header: &TextHeader,
    body: &str,
    profile: &PrintProfileId,
    cite: CiteProj<'_>,
) -> PrintBlock {
    let runs = || body_to_runs(body, &header.spans, Some(cite));
    match header.role {
        TextRole::Heading => {
            let level = u8::try_from(header.level.unwrap_or(1).clamp(1, 6)).unwrap_or(1);
            PrintBlock::Heading {
                level,
                runs: runs(),
                break_before: heading_break(level, profile),
            }
        }
        // ListItem: isolated items should have been coalesced; paragraph fallback.
        TextRole::Paragraph | TextRole::ListItem => PrintBlock::Paragraph { runs: runs() },
        TextRole::Blockquote => PrintBlock::Quote { runs: runs() },
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

pub(crate) fn map_figure(file: &TesFile, entry: &ChunkIndexEntry) -> Result<PrintBlock> {
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

pub(crate) fn map_slide(file: &TesFile, entry: &ChunkIndexEntry) -> Result<PrintBlock> {
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
