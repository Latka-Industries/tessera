//! Map text / figure / slide chunks to weave [`PrintBlock`]s.

use ariadnes_weave::{
    BreakHint, EmAmount as WeaveEm, FigurePlacement, LayoutOp as WeaveLayoutOp,
    MeasureFrac as WeaveFrac, PlaceSkip as WeavePlaceSkip, PrintBlock, PrintImage, PrintProfileId,
    RuleWidth as WeaveRuleWidth, SlideRegionContent, TableRow, TextRun,
    VspaceAmount as WeaveVspace,
};

use crate::catalog::chunk::{TextHeader, TextRole};
use crate::catalog::file::TesFile;
use crate::catalog::index::{ChunkIndexEntry, ChunkType};
use crate::catalog::layout::{EmAmount, LayoutOp, MeasureFrac, PlaceSkip, RuleWidth, VspaceAmount};
use crate::catalog::media::{ImagePayload, ImagePlacement};
use crate::error::{Result, TesError};
use crate::io::cite::CiteProj;
use crate::io::export::{
    decode_figure_entry, decode_layout_entry, decode_slide_entry, decode_text_entry,
};

use super::heading_dest_id;
use super::runs::{body_to_runs, cell_to_runs};
use crate::render::floats::{FloatListKind, float_dest_id};

pub(crate) fn map_text_block(
    chunk_id: u64,
    header: &TextHeader,
    body: &str,
    profile: &PrintProfileId,
    cite: CiteProj<'_>,
    links: &[crate::catalog::link::LinkEntry],
) -> PrintBlock {
    let runs = || body_to_runs(body, &header.spans, Some(cite), links);
    match header.role {
        TextRole::Heading => {
            let level = u8::try_from(header.level.unwrap_or(1).clamp(1, 6)).unwrap_or(1);
            PrintBlock::heading_dest(
                level,
                runs(),
                heading_break(level, profile),
                heading_dest_id(chunk_id),
            )
        }
        // ListItem: isolated items should have been coalesced; paragraph fallback.
        TextRole::Paragraph | TextRole::ListItem => {
            PrintBlock::paragraph_indent(runs(), header.indent_or_default())
        }
        TextRole::Blockquote => PrintBlock::Quote { runs: runs() },
        TextRole::CodeBlock => PrintBlock::Code {
            lang: header.code_lang.clone(),
            text: body.to_owned(),
        },
        TextRole::Table => map_table(chunk_id, header, body, cite, links),
        TextRole::Row => map_row(header, cite, links),
        TextRole::Math => PrintBlock::Math {
            display: true,
            latex: body.trim().to_owned(),
        },
        // Expanded / folded in `map_entries` (list-nav + columns markers).
        TextRole::Toc
        | TextRole::Lof
        | TextRole::Lot
        | TextRole::Columns
        | TextRole::ColumnsEnd => PrintBlock::paragraph(runs()),
    }
}

fn map_row(
    header: &TextHeader,
    cite: CiteProj<'_>,
    links: &[crate::catalog::link::LinkEntry],
) -> PrintBlock {
    let panes = header
        .panes
        .as_ref()
        .map(|panes| {
            panes
                .iter()
                .map(|pane| cell_to_runs(&pane.text, &pane.spans, Some(cite), links))
                .collect()
        })
        .unwrap_or_default();
    PrintBlock::row_indent(panes, header.indent_or_default())
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

fn map_table(
    chunk_id: u64,
    header: &TextHeader,
    body: &str,
    _cite: CiteProj<'_>,
    _links: &[crate::catalog::link::LinkEntry],
) -> PrintBlock {
    if let Some(table) = &header.table {
        let rows = table
            .rows
            .iter()
            .map(|row| TableRow {
                cells: row.cells.iter().map(|c| c.text.clone()).collect(),
            })
            .collect();
        return PrintBlock::table_dest(rows, float_dest_id(FloatListKind::Tables, chunk_id));
    }
    let rows: Vec<TableRow> = body
        .lines()
        .map(|line| TableRow {
            cells: line.split('\t').map(str::to_owned).collect(),
        })
        .collect();
    PrintBlock::table_dest(rows, float_dest_id(FloatListKind::Tables, chunk_id))
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
    Ok(PrintBlock::figure_dest(
        PrintImage {
            bytes: image.data,
            media_type: image.media_type,
            width_px: (image.width_px > 0).then_some(image.width_px),
            height_px: (image.height_px > 0).then_some(image.height_px),
        },
        figure.alt_text,
        plain_label_runs(figure.title.as_deref()),
        // Plain caption runs — weave `[caption]` knobs (italic/size/band) own paint.
        plain_label_runs(figure.caption.as_deref()),
        map_figure_placement(&figure.placement),
        float_dest_id(FloatListKind::Figures, entry.chunk_id),
    ))
}

/// Optional figure title/caption → zero or one plain [`TextRun`].
fn plain_label_runs(label: Option<&str>) -> Vec<TextRun> {
    super::nonempty_label(label)
        .map(|s| vec![TextRun::plain(s)])
        .unwrap_or_default()
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

pub(crate) fn map_layout(file: &TesFile, entry: &ChunkIndexEntry) -> Result<PrintBlock> {
    let layout = decode_layout_entry(file, entry)?;
    Ok(PrintBlock::Layout {
        ops: layout.ops.iter().map(map_layout_op).collect(),
    })
}

fn map_layout_op(op: &LayoutOp) -> WeaveLayoutOp {
    match op {
        LayoutOp::Place {
            skip,
            content,
            spans,
        } => WeaveLayoutOp::Place {
            skip: map_place_skip(*skip),
            runs: body_to_runs(content, spans, None, &[]),
        },
        LayoutOp::Vspace { amount } => WeaveLayoutOp::Vspace {
            amount: map_vspace(*amount),
        },
        LayoutOp::Rule { width } => WeaveLayoutOp::Rule {
            width: map_rule_width(width),
        },
    }
}

fn map_place_skip(skip: PlaceSkip) -> WeavePlaceSkip {
    match skip {
        PlaceSkip::Frac { frac } => WeavePlaceSkip::Frac {
            frac: map_frac(frac),
        },
        PlaceSkip::Em { em } => WeavePlaceSkip::Em { em: map_em(em) },
    }
}

fn map_vspace(amount: VspaceAmount) -> WeaveVspace {
    match amount {
        VspaceAmount::Small => WeaveVspace::Small,
        VspaceAmount::Med => WeaveVspace::Med,
        VspaceAmount::Big => WeaveVspace::Big,
        VspaceAmount::Em { em } => WeaveVspace::Em { em: map_em(em) },
    }
}

fn map_rule_width(width: &RuleWidth) -> WeaveRuleWidth {
    WeaveRuleWidth {
        frac: width.frac.map(map_frac),
        em: width.em.map(map_em),
    }
}

fn map_frac(frac: MeasureFrac) -> WeaveFrac {
    WeaveFrac::from_bps(frac.bps)
}

fn map_em(em: EmAmount) -> WeaveEm {
    WeaveEm::from_milli(em.milli)
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
