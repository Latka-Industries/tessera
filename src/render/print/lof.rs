//! Print IR expansion for sealed [`TextRole::Lof`] / [`TextRole::Lot`] (THI-395).

use ariadnes_weave::{InlineStyle, PrintBlock, TextRun};

use crate::catalog::chunk::TextHeader;
use crate::render::floats::{FloatEntry, FloatListKind, float_dest_id};

use super::title_paragraph;

/// Expand a sealed LOF / LOT marker into print IR blocks.
///
/// Default look: [`PrintBlock::TocEntry`] lines with `Figure N.` / `Table N.`
/// prefixes, optional dotted leaders, optional page column (weave-resolved when
/// `page_label` is `None`), and `f-{chunk_id}` / `t-{chunk_id}` destinations.
#[must_use]
pub(super) fn expand_float_list_print(
    header: &TextHeader,
    entries: &[FloatEntry],
    kind: FloatListKind,
) -> Vec<PrintBlock> {
    let mut blocks = Vec::new();
    if let Some(title) = header.title.as_deref().filter(|s| !s.is_empty()) {
        blocks.push(title_paragraph(title));
    }
    if entries.is_empty() {
        return blocks;
    }

    let pages = header.toc_pages_or_default();
    let leaders = header.toc_leaders_or_default();

    for entry in entries {
        let dest_id = Some(float_dest_id(kind, entry.chunk_id));
        let title_text = format!("{} {}. {}", kind.noun(), entry.number, entry.text);
        let mut run = TextRun::plain(title_text);
        run.style = InlineStyle {
            link: true,
            ..InlineStyle::default()
        };
        let page_label = if pages { None } else { Some(String::new()) };
        blocks.push(PrintBlock::toc_entry_leaders(
            vec![run],
            page_label,
            dest_id,
            0,
            leaders,
        ));
    }
    blocks
}
