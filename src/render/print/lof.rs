//! Print IR expansion for sealed [`TextRole::Lof`] / [`TextRole::Lot`] (THI-395).

use ariadnes_weave::PrintBlock;

use crate::catalog::chunk::TextHeader;
use crate::render::floats::{FloatCandidate, FloatListKind, float_dest_id, select_float_entries};

use super::{push_list_nav_entry, title_paragraph};

/// Expand a sealed LOF / LOT marker into print IR blocks.
///
/// Default look: [`PrintBlock::TocEntry`] lines with `Figure N.` / `Table N.`
/// prefixes from float **titles** (`source=title`, default; untitled omitted).
/// Optional `source=caption`. Page digits weave-resolve from `f-*` / `t-*`.
#[must_use]
pub(super) fn expand_float_list_print(
    header: &TextHeader,
    candidates: &[FloatCandidate],
    kind: FloatListKind,
) -> Vec<PrintBlock> {
    let entries = select_float_entries(candidates, header.float_list_source_or_default());
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
        push_list_nav_entry(
            &mut blocks,
            format!("{} {}. {}", kind.noun(), entry.number, entry.text),
            Some(float_dest_id(kind, entry.chunk_id)),
            0,
            pages,
            leaders,
        );
    }
    blocks
}
