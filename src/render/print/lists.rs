//! Coalesce consecutive list-item chunks into nested weave lists.

use ariadnes_weave::{ListItem, PrintBlock, TextAlign as WeaveAlign, TextRun};

use crate::catalog::chunk::{ListKind, TextHeader};
use crate::io::cite::CiteProj;

use super::runs::body_to_runs;

#[derive(Debug, Clone)]
pub(crate) struct PendingListItem {
    depth: u32,
    kind: ListKind,
    indent: u32,
    align: Option<WeaveAlign>,
    runs: Vec<TextRun>,
    notes: Vec<PrintBlock>,
}

pub(crate) fn push_list_item(
    blocks: &mut Vec<PrintBlock>,
    list_buf: &mut Vec<PendingListItem>,
    header: &TextHeader,
    body: &str,
    cite: CiteProj<'_>,
    links: &[crate::catalog::link::LinkEntry],
    chunk_id: u64,
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
        indent: header.indent_or_default(),
        align: super::map_text_align(header.align),
        runs: body_to_runs(body, &header.spans, Some(cite), links, chunk_id),
        notes: super::runs::collect_print_notes(chunk_id, &header.spans),
    });
}

pub(crate) fn flush_list(blocks: &mut Vec<PrintBlock>, list_buf: &mut Vec<PendingListItem>) {
    if list_buf.is_empty() {
        return;
    }
    let items = std::mem::take(list_buf);
    let notes: Vec<_> = items.iter().flat_map(|i| i.notes.iter().cloned()).collect();
    blocks.push(coalesce_list(&items));
    blocks.extend(notes);
}

fn coalesce_list(items: &[PendingListItem]) -> PrintBlock {
    let ordered = matches!(items.first().map(|i| i.kind), Some(ListKind::Ordered));
    let min_depth = items.iter().map(|i| i.depth).min().unwrap_or(1);
    let indent = items.iter().map(|i| i.indent).find(|&n| n > 0).unwrap_or(0);
    let text_align = items.iter().find_map(|i| i.align);
    PrintBlock::List {
        ordered,
        items: nest_list_items(items, min_depth, indent),
        indent,
        text_align,
    }
}

fn nest_list_items(items: &[PendingListItem], depth: u32, band_indent: u32) -> Vec<ListItem> {
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
            out.extend(nest_list_items(slice, depth, band_indent));
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
            child_lists(&items[child_start..i], depth + 1, band_indent)
        } else {
            Vec::new()
        };
        out.push(ListItem { runs, children });
    }
    out
}

fn child_lists(items: &[PendingListItem], depth: u32, band_indent: u32) -> Vec<PrintBlock> {
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
        let text_align = items[start..end].iter().find_map(|i| i.align);
        out.push(PrintBlock::List {
            ordered: matches!(kind, ListKind::Ordered),
            items: nest_list_items(&items[start..end], depth, band_indent),
            indent: band_indent,
            text_align,
        });
        start = end;
    }
    out
}
