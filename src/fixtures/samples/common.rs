//! Shared helpers for browse / demo sample encoders.

use crate::catalog::{DocumentCatalog, SlidePayload, SlideRegion, TableCell};
use crate::layout::DocKind;

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
