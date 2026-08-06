//! Reading-order Tessprek blocks ([`ContentBlock`]).

use crate::catalog::OutboundLink;
use crate::catalog::chunk::{CitePayload, TextHeader, TextRole};
use crate::catalog::media::FigureRef;
use crate::catalog::slide::SlidePayload;
use crate::io::{cite, face};

/// One reading-order block in a Tessprek projection.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)] // CitePayload carries optional BibEntry.
pub enum ContentBlock {
    /// Text chunk.
    Text {
        /// Optional stable id from the source projection.
        chunk_id: Option<u64>,
        /// Semantic header.
        header: TextHeader,
        /// UTF-8 body.
        body: String,
        /// Outbound links over [`Self::Text::body`] (Tessprek / markdown).
        pending_links: Vec<OutboundLink>,
        /// Inline `\cite{key}` spans over [`Self::Text::body`] (Tessprek).
        pending_cites: Vec<cite::PendingCite>,
        /// Inline `\face{id}{text}` spans over [`Self::Text::body`] (Tessprek).
        pending_faces: Vec<face::PendingFace>,
    },
    /// Figure chunk referencing an image payload.
    Figure {
        /// Optional stable id from the source projection.
        chunk_id: Option<u64>,
        /// Figure metadata + alt.
        figure: FigureRef,
    },
    /// Cite chunk (Tessprek `\cite` / `\quote` / `\ref` via [`crate::io::cite::classify_cite`]).
    Cite {
        /// Optional stable id from the source projection.
        chunk_id: Option<u64>,
        /// Cite payload.
        cite: CitePayload,
    },
    /// Slide chunk with named region refs.
    Slide {
        /// Optional stable id from the source projection.
        chunk_id: Option<u64>,
        /// Slide payload.
        slide: SlidePayload,
    },
    /// Inert attachment chunk (metadata in Tessprek; bytes from source or media bag).
    Attachment {
        /// Source chunk id, or a temporary id resolved via [`EditMediaBag`].
        chunk_id: Option<u64>,
        /// Safe basename.
        filename: String,
        /// IANA media type.
        media_type: String,
        /// Optional caption.
        caption: Option<String>,
        /// Declared integrity hash (checked against bytes on write).
        sha256: String,
    },
}

impl ContentBlock {
    /// Projected chunk id, when known.
    #[must_use]
    pub fn chunk_id(&self) -> Option<u64> {
        match self {
            Self::Text { chunk_id, .. }
            | Self::Figure { chunk_id, .. }
            | Self::Cite { chunk_id, .. }
            | Self::Slide { chunk_id, .. }
            | Self::Attachment { chunk_id, .. } => *chunk_id,
        }
    }

    /// Whether this block is a list-item text chunk.
    #[must_use]
    pub fn is_list_item(&self) -> bool {
        matches!(
            self,
            Self::Text { header, .. } if header.role == TextRole::ListItem
        )
    }
}
