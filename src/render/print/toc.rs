//! Print IR expansion for sealed [`TextRole::Toc`] (THI-390).

use ariadnes_weave::{ListItem, PrintBlock, TextRun};

use crate::catalog::chunk::TextHeader;
use crate::render::toc::{TocHeading, filter_headings};

use super::title_paragraph;

/// Expand a sealed TOC marker into print IR blocks.
#[must_use]
pub(super) fn expand_toc_print(header: &TextHeader, headings: &[TocHeading]) -> Vec<PrintBlock> {
    let included = filter_headings(header, headings);
    let mut blocks = Vec::new();
    if let Some(title) = header.title.as_deref().filter(|s| !s.is_empty()) {
        // Strong paragraph — not a Heading, so chapter slicing / H1 breaks stay clean.
        blocks.push(title_paragraph(title));
    }
    if included.is_empty() {
        return blocks;
    }
    if header.toc_pages == Some(true) {
        // Stub leaders until weave resolves heading pages (THI-393 follow-on).
        for h in &included {
            let indent = h.level.saturating_sub(1);
            blocks.push(PrintBlock::row_indent(
                vec![
                    vec![TextRun::plain(h.text.clone())],
                    vec![TextRun::plain("—")],
                ],
                indent,
            ));
        }
    } else {
        let min_level = included.iter().map(|h| h.level).min().unwrap_or(1);
        blocks.push(PrintBlock::List {
            ordered: false,
            items: nest_toc_items(&included, min_level),
            indent: 0,
        });
    }
    blocks
}

fn nest_toc_items(headings: &[&TocHeading], depth: u32) -> Vec<ListItem> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < headings.len() {
        if headings[i].level < depth {
            break;
        }
        while i < headings.len() && headings[i].level > depth {
            let sub = headings[i].level;
            let start = i;
            i += 1;
            while i < headings.len() && headings[i].level > sub {
                i += 1;
            }
            out.extend(nest_toc_items(&headings[start..i], sub));
        }
        if i >= headings.len() || headings[i].level < depth {
            break;
        }
        let runs = vec![TextRun::plain(headings[i].text.clone())];
        i += 1;
        let child_start = i;
        while i < headings.len() && headings[i].level > depth {
            i += 1;
        }
        let children = if child_start < i {
            vec![PrintBlock::List {
                ordered: false,
                items: nest_toc_items(&headings[child_start..i], depth + 1),
                indent: 0,
            }]
        } else {
            Vec::new()
        };
        out.push(ListItem { runs, children });
    }
    out
}
