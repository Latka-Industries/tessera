//! Injected image / attachment payloads for [`crate::edit::edit_write_with_media`].

use crate::catalog::media::{AttachmentPayload, ImagePayload};
use crate::error::{Result, TesError};

/// New image / attachment payloads injected during [`edit_write`].
///
/// Tessprek (or Fluid) may reference temporary chunk ids that are not present in
/// the source `.tes`. Those ids are resolved from this bag when compiling.
#[derive(Debug, Clone, Default)]
pub struct EditMediaBag {
    /// Temporary image chunk id → payload (figure `image=` / `media:N`).
    pub images: Vec<(u64, ImagePayload)>,
    /// Temporary attachment chunk id → payload (attachment `chunk=`).
    pub attachments: Vec<(u64, AttachmentPayload)>,
}

impl EditMediaBag {
    /// Whether the bag carries any payloads.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.images.is_empty() && self.attachments.is_empty()
    }

    pub(super) fn image_map(&self) -> Result<std::collections::HashMap<u64, &ImagePayload>> {
        unique_id_map(&self.images, "image")
    }

    pub(super) fn attachment_map(
        &self,
    ) -> Result<std::collections::HashMap<u64, &AttachmentPayload>> {
        unique_id_map(&self.attachments, "attachment")
    }
}

fn unique_id_map<'a, T>(
    entries: &'a [(u64, T)],
    kind: &str,
) -> Result<std::collections::HashMap<u64, &'a T>> {
    let mut map = std::collections::HashMap::with_capacity(entries.len());
    for (id, payload) in entries {
        if map.insert(*id, payload).is_some() {
            return Err(TesError::EditOp {
                message: format!("duplicate media bag {kind} id {id}"),
            });
        }
    }
    Ok(map)
}
