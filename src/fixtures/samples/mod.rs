//! Browse / demo `.tes` samples (not byte-golden).
//!
//! These are for exploring Tessprek chunk shapes in Neovim / CLI. Regenerate with
//! `cargo run --example gen_sample_fixtures`. Do not assert on-disk bytes in CI.

mod block_captions;
mod common;
mod field_notes;
mod figure_align;
mod hyphen_dense;
mod manuscript;
mod showcase;
mod studio_brief;
mod text_roles;

pub use block_captions::encode_block_captions;
pub use field_notes::encode_field_notes;
pub use figure_align::encode_figure_align;
pub use hyphen_dense::encode_hyphen_dense;
pub use manuscript::encode_manuscript_chapters;
pub use showcase::encode_tessprek_showcase;
pub use studio_brief::encode_studio_brief;
pub use text_roles::encode_text_roles;

use std::fs;
use std::path::Path;

use crate::error::Result;

/// Write every sample under `dir` (typically `fixtures/samples`).
///
/// # Errors
///
/// Returns I/O errors from creating the directory or writing files.
pub fn write_all(dir: &Path) -> Result<()> {
    fs::create_dir_all(dir)?;
    let files = [
        ("tessprek_showcase.tes", encode_tessprek_showcase()),
        ("text_roles.tes", encode_text_roles()),
        ("field_notes.tes", encode_field_notes()),
        ("studio_brief.tes", encode_studio_brief()),
        ("block_captions.tes", encode_block_captions()),
        ("figure_align.tes", encode_figure_align()),
        ("manuscript_chapters.tes", encode_manuscript_chapters()),
        ("hyphen_dense.tes", encode_hyphen_dense()),
    ];
    for (name, bytes) in files {
        fs::write(dir.join(name), bytes)?;
    }
    Ok(())
}
