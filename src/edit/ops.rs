//! Typed mutation operations for `tes apply --ops`.

use serde::{Deserialize, Serialize};

use crate::catalog::chunk::{TextHeader, TextRole};
use crate::catalog::document::DocumentCatalog;
use crate::error::{Result, TesError};

use super::ContentBlock;

/// Closed set of agent-safe document mutations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum TesOp {
    /// Replace catalog title.
    SetTitle {
        /// New title.
        title: String,
    },
    /// Replace catalog aliases (full list; `[]` clears).
    SetAliases {
        /// New alias list.
        aliases: Vec<String>,
    },
    /// Replace catalog tags (full list; `[]` clears).
    SetTags {
        /// New tag list.
        tags: Vec<String>,
    },
    /// Replace or clear catalog slug (`null` clears).
    SetSlug {
        /// New slug, or `null` to clear.
        slug: Option<String>,
    },
    /// Replace or clear catalog category (`null` clears).
    SetCategory {
        /// New category, or `null` to clear.
        category: Option<String>,
    },
    /// Replace or clear catalog section path (`null` clears).
    SetSection {
        /// New section path under category (e.g. `Books/Authors`), or `null` to clear.
        section: Option<String>,
    },
    /// Replace a text chunk body (and optional header fields).
    SetText {
        /// Existing or projected chunk id.
        chunk_id: u64,
        /// Replacement body.
        body: String,
        /// Optional role override.
        #[serde(default)]
        role: Option<TextRole>,
        /// Optional heading level.
        #[serde(default)]
        level: Option<u32>,
        /// Optional class list replacement.
        #[serde(default)]
        class: Option<Vec<String>>,
    },
    /// Append a paragraph at the end of reading order.
    AppendParagraph {
        /// Paragraph body.
        body: String,
        /// Optional classes.
        #[serde(default)]
        class: Option<Vec<String>>,
    },
    /// Delete a reading-order chunk by id.
    DeleteChunk {
        /// Chunk id to remove.
        chunk_id: u64,
    },
}

/// Mutable catalog fields carried through [`apply_ops_to_blocks`] into compile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogPatch {
    /// Display title.
    pub title: String,
    /// Alternate display / wikilink names.
    pub aliases: Vec<String>,
    /// Freeform topical tags.
    pub tags: Vec<String>,
    /// Optional vault-unique human handle.
    pub slug: Option<String>,
    /// Optional primary bucket.
    pub category: Option<String>,
    /// Optional ordered path under category.
    pub section: Option<String>,
}

impl Default for CatalogPatch {
    fn default() -> Self {
        Self {
            title: "Untitled".into(),
            aliases: Vec::new(),
            tags: Vec::new(),
            slug: None,
            category: None,
            section: None,
        }
    }
}

impl CatalogPatch {
    /// Seed a patch from an existing catalog (or untitled defaults).
    #[must_use]
    pub fn from_catalog(catalog: Option<&DocumentCatalog>) -> Self {
        catalog.map_or_else(Self::default, |c| Self {
            title: c.title.clone(),
            aliases: c.aliases.clone(),
            tags: c.tags.clone(),
            slug: c.slug.clone(),
            category: c.category.clone(),
            section: c.section.clone(),
        })
    }

    /// Apply this patch onto a catalog being compiled for write-back.
    pub fn apply_to(&self, catalog: &mut DocumentCatalog) {
        catalog.title.clone_from(&self.title);
        catalog.aliases.clone_from(&self.aliases);
        catalog.tags.clone_from(&self.tags);
        catalog.slug.clone_from(&self.slug);
        catalog.category.clone_from(&self.category);
        catalog.section.clone_from(&self.section);
    }

    /// Apply a catalog-field op. Returns `true` when `op` mutated this patch.
    fn apply_catalog_op(&mut self, op: &TesOp) -> bool {
        match op {
            TesOp::SetTitle { title } => {
                self.title.clone_from(title);
                true
            }
            TesOp::SetAliases { aliases } => {
                self.aliases.clone_from(aliases);
                true
            }
            TesOp::SetTags { tags } => {
                self.tags.clone_from(tags);
                true
            }
            TesOp::SetSlug { slug } => {
                self.slug.clone_from(slug);
                true
            }
            TesOp::SetCategory { category } => {
                self.category.clone_from(category);
                true
            }
            TesOp::SetSection { section } => {
                self.section.clone_from(section);
                true
            }
            TesOp::SetText { .. } | TesOp::AppendParagraph { .. } | TesOp::DeleteChunk { .. } => {
                false
            }
        }
    }
}

/// Apply typed ops onto an in-memory Tessprek projection.
///
/// # Errors
///
/// Returns [`TesError::EditOp`] when an op references a missing chunk or is invalid.
pub fn apply_ops_to_blocks(
    blocks: &mut Vec<ContentBlock>,
    catalog: &mut CatalogPatch,
    ops: &[TesOp],
) -> Result<()> {
    for op in ops {
        if catalog.apply_catalog_op(op) {
            continue;
        }
        match op {
            TesOp::SetText {
                chunk_id,
                body,
                role,
                level,
                class,
            } => apply_set_text(blocks, *chunk_id, body, *role, *level, class.as_deref())?,
            TesOp::AppendParagraph { body, class } => {
                let mut header = TextHeader::paragraph();
                if let Some(class) = class {
                    header.classes.clone_from(class);
                }
                blocks.push(ContentBlock::Text {
                    chunk_id: None,
                    header,
                    body: body.clone(),
                    pending_links: Vec::new(),
                    pending_cites: Vec::new(),
                    pending_faces: Vec::new(),
                });
            }
            TesOp::DeleteChunk { chunk_id } => {
                let before = blocks.len();
                blocks.retain(|b| b.chunk_id() != Some(*chunk_id));
                if blocks.len() == before {
                    return Err(TesError::EditOp {
                        message: format!("delete_chunk: chunk {chunk_id} not found"),
                    });
                }
            }
            TesOp::SetTitle { .. }
            | TesOp::SetAliases { .. }
            | TesOp::SetTags { .. }
            | TesOp::SetSlug { .. }
            | TesOp::SetCategory { .. }
            | TesOp::SetSection { .. } => {}
        }
    }
    Ok(())
}

fn apply_set_text(
    blocks: &mut [ContentBlock],
    chunk_id: u64,
    body: &str,
    role: Option<TextRole>,
    level: Option<u32>,
    class: Option<&[String]>,
) -> Result<()> {
    let block = blocks
        .iter_mut()
        .find(|b| b.chunk_id() == Some(chunk_id))
        .ok_or_else(|| TesError::EditOp {
            message: format!("set_text: chunk {chunk_id} not found"),
        })?;
    let ContentBlock::Text {
        header, body: b, ..
    } = block
    else {
        return Err(TesError::EditOp {
            message: format!("set_text: chunk {chunk_id} is not text"),
        });
    };
    if let Some(role) = role {
        header.role = role;
    }
    if let Some(level) = level {
        header.level = Some(level.clamp(1, 6));
    }
    if let Some(class) = class {
        header.classes = class.to_vec();
    }
    body.clone_into(b);
    Ok(())
}

/// Parse a JSON array of [`TesOp`].
///
/// # Errors
///
/// Returns [`TesError::Json`] on malformed JSON.
pub fn parse_ops_json(json: &str) -> Result<Vec<TesOp>> {
    Ok(serde_json::from_str(json)?)
}
