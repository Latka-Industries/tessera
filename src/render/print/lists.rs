//! Coalesce consecutive list-item chunks into nested weave lists.

use ariadnes_weave::{ListItem, PrintBlock, TextRun};

use crate::catalog::chunk::{ListKind, TextHeader};
use crate::io::cite::CiteProj;

use super::runs::body_to_runs;

#[derive(Debug, Clone)]
pub(crate) struct PendingListItem {
    depth: u32,
    kind: ListKind,
    runs: Vec<TextRun>,
}

pub(crate) fn push_list_item(
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

pub(crate) fn flush_list(blocks: &mut Vec<PrintBlock>, list_buf: &mut Vec<PendingListItem>) {
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
