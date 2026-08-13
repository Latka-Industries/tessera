//! Print IR expansion for sealed [`TextRole::Toc`] (THI-390 / THI-390 defaults).

use ariadnes_weave::PrintBlock;

use crate::catalog::chunk::TextHeader;
use crate::render::toc::{TocHeading, filter_headings, section_number_labels};

use super::heading_dest_id;
use super::{push_list_nav_entry, title_paragraph};

/// Expand a sealed TOC marker into print IR blocks.
///
/// Default look: [`PrintBlock::TocEntry`] lines with section numbers (and band
/// indent when sections are on), optional dotted leaders, optional page column
/// (weave-resolved when `page_label` is `None`), and `h-{chunk_id}` destinations.
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

    let owned: Vec<TocHeading> = included.iter().map(|h| (*h).clone()).collect();
    let sections = header.toc_sections_or_default();
    let labels = if sections {
        section_number_labels(&owned)
    } else {
        vec![String::new(); owned.len()]
    };
    let min_level = owned.iter().map(|h| h.level).min().unwrap_or(1);
    let pages = header.toc_pages_or_default();
    let leaders = header.toc_leaders_or_default();

    for (i, h) in owned.iter().enumerate() {
        let title_text = match labels.get(i).map(String::as_str).filter(|s| !s.is_empty()) {
            Some(num) => format!("{num} {}", h.text),
            None => h.text.clone(),
        };
        let indent = if sections {
            h.level.saturating_sub(min_level)
        } else {
            0
        };
        push_list_nav_entry(
            &mut blocks,
            title_text,
            Some(heading_dest_id(h.chunk_id)),
            indent,
            pages,
            leaders,
        );
    }
    blocks
}
