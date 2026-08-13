//! Print IR expansion for sealed [`TextRole::Toc`] (THI-390 / THI-390 defaults).

use ariadnes_weave::{InlineStyle, PrintBlock, TextRun};

use crate::catalog::chunk::TextHeader;
use crate::render::toc::{TocHeading, filter_headings, section_number_labels};

use super::title_paragraph;

/// Expand a sealed TOC marker into print IR blocks.
///
/// Default look: [`PrintBlock::TocEntry`] lines with section numbers, optional
/// page column (weave-resolved when `page_label` is `None`), and `h-{chunk_id}`
/// destinations matching heading `dest_id`s.
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
    let labels = if header.toc_sections_or_default() {
        section_number_labels(&owned)
    } else {
        vec![String::new(); owned.len()]
    };
    let min_level = owned.iter().map(|h| h.level).min().unwrap_or(1);
    let pages = header.toc_pages_or_default();

    for (i, h) in owned.iter().enumerate() {
        let dest_id = Some(format!("h-{}", h.chunk_id));
        let title_text = match labels.get(i).map(String::as_str).filter(|s| !s.is_empty()) {
            Some(num) => format!("{num} {}", h.text),
            None => h.text.clone(),
        };
        let mut run = TextRun::plain(title_text);
        run.style = InlineStyle {
            link: true,
            ..InlineStyle::default()
        };
        // `None` → weave resolves page digits from `dest_id`.
        // `Some("")` → no page column (pages explicitly off).
        let page_label = if pages { None } else { Some(String::new()) };
        let indent = h.level.saturating_sub(min_level);
        blocks.push(PrintBlock::toc_entry(
            vec![run],
            page_label,
            dest_id,
            indent,
        ));
    }
    blocks
}
