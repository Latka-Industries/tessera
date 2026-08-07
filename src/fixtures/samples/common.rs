//! Shared helpers for browse / demo sample encoders.

use crate::catalog::{
    DocumentCatalog, FigureRef, ImagePayload, ImagePlacement, SlidePayload, SlideRegion, TableCell,
    TesWriterSession,
};
use crate::error::Result;
use crate::layout::DocKind;

use super::super::v0::{PNG_SWATCH, PNG_SWATCH_HEIGHT, PNG_SWATCH_WIDTH};

pub(super) fn catalog(
    doc_id: &str,
    title: &str,
    created: &str,
    modified: &str,
    kind: DocKind,
    tags: &[&str],
) -> DocumentCatalog {
    let mut catalog = DocumentCatalog::new(doc_id, title, created, modified, kind);
    catalog.tags = tags.iter().map(|s| (*s).to_owned()).collect();
    catalog
}

pub(super) fn cell(text: &str, is_header: bool) -> TableCell {
    TableCell {
        text: text.into(),
        spans: Vec::new(),
        align: None,
        is_header,
        rowspan: None,
        colspan: None,
    }
}

pub(super) fn title_body_slide(title_id: u64, body_id: u64) -> SlidePayload {
    SlidePayload {
        layout_id: "title_body".into(),
        regions: vec![
            SlideRegion {
                name: "title".into(),
                chunk_id: title_id,
            },
            SlideRegion {
                name: "body".into(),
                chunk_id: body_id,
            },
        ],
    }
}

/// Visible 240×120 swatch image chunk (native PDF can paint this; 1×1 cannot).
pub(super) fn add_swatch_image(session: &mut TesWriterSession) -> Result<u64> {
    session.add_image_chunk(&ImagePayload {
        media_type: "image/png".into(),
        width_px: PNG_SWATCH_WIDTH,
        height_px: PNG_SWATCH_HEIGHT,
        data: PNG_SWATCH.to_vec(),
    })
}

/// Flow figure pointing at an existing image chunk.
pub(super) fn add_flow_figure(
    session: &mut TesWriterSession,
    image_chunk_id: u64,
    alt_text: &str,
    title: Option<&str>,
    caption: Option<&str>,
) -> Result<u64> {
    session.add_figure(&FigureRef {
        image_chunk_id,
        alt_text: alt_text.into(),
        title: title.map(str::to_owned),
        caption: caption.map(str::to_owned),
        placement: ImagePlacement::Flow,
    })
}
