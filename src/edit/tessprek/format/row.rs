//! Tessprek `\row{pane}{pane}…` → sealed [`TextRole::Row`](crate::catalog::TextRole::Row).

use crate::catalog::OutboundLink;
use crate::catalog::chunk::{InlineKind, InlineSpan, TableCell, TextHeader};
use crate::edit::ContentBlock;
use crate::error::Result;
use crate::io::import::parse_markdown_blocks;

use super::parse_err;

/// Decode consecutive Tessprek row panes into a text block (`role=row`).
///
/// # Errors
///
/// Returns [`TesError::EditParse`] when fewer than two panes are supplied.
pub(super) fn decode_row_panes(panes: &[String], line_no: usize) -> Result<ContentBlock> {
    if panes.len() < 2 {
        return Err(parse_err(
            line_no,
            1,
            format!("\\row requires at least 2 panes, found {}", panes.len()),
        ));
    }

    let mut cells = Vec::with_capacity(panes.len());
    let mut pending_links = Vec::new();

    for raw in panes {
        let (text, spans, links) = parse_pane_markdown(raw);
        let mut cell_spans = spans;
        for link in links {
            let link_id = u64::try_from(pending_links.len()).unwrap_or(u64::MAX);
            cell_spans.push(InlineSpan {
                start: link.start,
                end: link.end,
                kind: InlineKind::Link { link_id },
            });
            pending_links.push(link);
        }
        cells.push(TableCell {
            text,
            spans: cell_spans,
            align: None,
            is_header: false,
            rowspan: None,
            colspan: None,
        });
    }

    Ok(ContentBlock::Text {
        chunk_id: None,
        header: TextHeader::row(cells),
        body: String::new(),
        pending_links,
        pending_cites: Vec::new(),
        pending_fonts: Vec::new(),
        pending_notes: Vec::new(),
    })
}

fn parse_pane_markdown(raw: &str) -> (String, Vec<InlineSpan>, Vec<OutboundLink>) {
    let parsed = parse_markdown_blocks(raw);
    if let Some(block) = parsed.into_iter().next() {
        // Prefer the first block's body (paragraph / heading stripped to text).
        let mut spans = block.header.spans;
        spans.retain(|s| !matches!(s.kind, InlineKind::Link { .. }));
        return (block.body, spans, block.pending_links);
    }
    (raw.trim().to_owned(), Vec::new(), Vec::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::chunk::TextRole;

    #[test]
    fn decodes_two_panes_with_link() {
        let block = decode_row_panes(
            &["[Org](https://example.com)".into(), "New York, NY".into()],
            1,
        )
        .expect("decode");
        let ContentBlock::Text {
            header,
            pending_links,
            ..
        } = block
        else {
            panic!("expected text");
        };
        assert_eq!(header.role, TextRole::Row);
        let panes = header.panes.expect("panes");
        assert_eq!(panes.len(), 2);
        assert_eq!(panes[0].text, "Org");
        assert_eq!(pending_links.len(), 1);
        assert_eq!(pending_links[0].dest, "https://example.com");
    }
}
