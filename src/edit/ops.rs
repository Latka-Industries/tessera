//! Typed mutation operations for `tes apply --ops`.

use serde::Deserialize;

use crate::catalog::chunk::{TextHeader, TextRole};
use crate::error::{Result, TesError};

use super::ContentBlock;

/// Closed set of agent-safe document mutations.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum TesOp {
    /// Replace catalog title.
    SetTitle {
        /// New title.
        title: String,
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

/// Apply typed ops onto an in-memory Tessprek projection.
///
/// # Errors
///
/// Returns [`TesError::EditOp`] when an op references a missing chunk or is invalid.
pub fn apply_ops_to_blocks(
    blocks: &mut Vec<ContentBlock>,
    title: &mut String,
    ops: &[TesOp],
) -> Result<()> {
    for op in ops {
        match op {
            TesOp::SetTitle { title: next } => {
                *title = next.clone();
            }
            TesOp::SetText {
                chunk_id,
                body,
                role,
                level,
                class,
            } => {
                let block = blocks
                    .iter_mut()
                    .find(|b| b.chunk_id() == Some(*chunk_id))
                    .ok_or_else(|| TesError::EditOp {
                        message: format!("set_text: chunk {chunk_id} not found"),
                    })?;
                match block {
                    ContentBlock::Text {
                        header, body: b, ..
                    } => {
                        if let Some(role) = role {
                            header.role = *role;
                        }
                        if let Some(level) = *level {
                            header.level = Some(level.clamp(1, 6));
                        }
                        if let Some(class) = class {
                            header.classes = class.clone();
                        }
                        *b = body.clone();
                    }
                    _ => {
                        return Err(TesError::EditOp {
                            message: format!("set_text: chunk {chunk_id} is not text"),
                        });
                    }
                }
            }
            TesOp::AppendParagraph { body, class } => {
                let mut header = TextHeader::paragraph();
                if let Some(class) = class {
                    header.classes = class.clone();
                }
                blocks.push(ContentBlock::Text {
                    chunk_id: None,
                    header,
                    body: body.clone(),
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
        }
    }
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
