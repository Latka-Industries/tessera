//! Image media payloads and contextual figure references.
//!
//! Image bytes live in non-reading-order [`super::ChunkType::Image`] chunks.
//! Each use in reading order is a [`super::ChunkType::Figure`] JSON payload that
//! points at an image chunk with alt text, optional caption, and placement.

use serde::{Deserialize, Serialize};

use crate::error::{Result, TesError};
use argus::{LeReader, LeWriter};

/// Soft upper bound on a single image payload (32 MiB).
pub const IMAGE_MAX_BYTES: usize = 32 * 1024 * 1024;

/// Soft upper bound on declared width or height.
pub const IMAGE_MAX_DIMENSION: u32 = 16_384;

/// Soft upper bound on `width_px * height_px`.
pub const IMAGE_MAX_PIXELS: u64 = 50_000_000;

/// Soft upper bound on MIME / alt / caption string lengths.
pub const IMAGE_STRING_MAX: usize = 1024;

/// Reusable image bytes (chunk type `2`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImagePayload {
    /// IANA media type, e.g. `image/jpeg`.
    pub media_type: String,
    /// Intrinsic width in pixels (0 = unknown).
    pub width_px: u32,
    /// Intrinsic height in pixels (0 = unknown).
    pub height_px: u32,
    /// Raw image bytes (png/jpeg/…); never executed.
    pub data: Vec<u8>,
}

impl ImagePayload {
    /// Validate resource limits without encoding.
    ///
    /// # Errors
    ///
    /// Returns [`TesError::InvalidImage`] if MIME, dimensions, or data violate
    /// the soft resource limits.
    pub fn validate(&self) -> Result<()> {
        if self.media_type.is_empty() || self.media_type.len() > IMAGE_STRING_MAX {
            return Err(TesError::InvalidImage {
                message: format!(
                    "media_type length {} out of range 1..={IMAGE_STRING_MAX}",
                    self.media_type.len()
                ),
            });
        }
        if self.media_type.contains(['\0', '\n', '\r']) {
            return Err(TesError::InvalidImage {
                message: "media_type must be a single-line token".into(),
            });
        }
        if !self.media_type.starts_with("image/") {
            return Err(TesError::InvalidImage {
                message: format!(
                    "media_type must start with 'image/', got '{}'",
                    self.media_type
                ),
            });
        }
        if self.width_px > IMAGE_MAX_DIMENSION || self.height_px > IMAGE_MAX_DIMENSION {
            return Err(TesError::InvalidImage {
                message: format!(
                    "dimensions {}x{} exceed {IMAGE_MAX_DIMENSION}",
                    self.width_px, self.height_px
                ),
            });
        }
        let pixels = u64::from(self.width_px).saturating_mul(u64::from(self.height_px));
        if pixels > IMAGE_MAX_PIXELS {
            return Err(TesError::InvalidImage {
                message: format!("pixel count {pixels} exceeds {IMAGE_MAX_PIXELS}"),
            });
        }
        if self.data.is_empty() {
            return Err(TesError::InvalidImage {
                message: "image data must be non-empty".into(),
            });
        }
        if self.data.len() > IMAGE_MAX_BYTES {
            return Err(TesError::InvalidImage {
                message: format!(
                    "image data {} bytes exceeds {IMAGE_MAX_BYTES}",
                    self.data.len()
                ),
            });
        }
        Ok(())
    }

    /// Encode: `u32 mime_len | mime | u32 w | u32 h | u64 data_len | data`.
    ///
    /// # Errors
    ///
    /// Returns validation errors from [`Self::validate`], or
    /// [`TesError::InvalidImage`] if the MIME length does not fit in `u32`.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let mime = self.media_type.as_bytes();
        let mime_len = u32::try_from(mime.len()).map_err(|_| TesError::InvalidImage {
            message: "media_type too long for u32".into(),
        })?;
        let data_len = self.data.len() as u64;
        let mut out = Vec::with_capacity(4 + mime.len() + 16 + self.data.len());
        let mut len_buf = [0u8; 4];
        {
            let mut w = LeWriter::new(&mut len_buf);
            w.put_u32(mime_len);
        }
        out.extend_from_slice(&len_buf);
        out.extend_from_slice(mime);
        let mut dim = [0u8; 16];
        {
            let mut w = LeWriter::new(&mut dim);
            w.put_u32(self.width_px);
            w.put_u32(self.height_px);
            w.put_u64(data_len);
        }
        out.extend_from_slice(&dim);
        out.extend_from_slice(&self.data);
        Ok(out)
    }

    /// Decode an image payload.
    ///
    /// # Errors
    ///
    /// Returns [`TesError::BufferTooSmall`] / [`TesError::InvalidUtf8`] for a
    /// truncated or non-UTF-8 MIME, [`TesError::InvalidImage`] for trailing
    /// bytes or failed validation.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let mut r = LeReader::require(bytes, "ImagePayload", 4)?;
        let mime_len = r.take_u32() as usize;
        let rest = &bytes[4..];
        if rest.len() < mime_len {
            return Err(TesError::BufferTooSmall {
                structure: "ImagePayload.media_type",
                need: mime_len,
                got: rest.len(),
            });
        }
        let media_type = std::str::from_utf8(&rest[..mime_len])
            .map_err(|_| TesError::InvalidUtf8 {
                structure: "ImagePayload.media_type",
            })?
            .to_owned();
        let after_mime = &rest[mime_len..];
        let mut r2 = LeReader::require(after_mime, "ImagePayload.dims", 16)?;
        let width_px = r2.take_u32();
        let height_px = r2.take_u32();
        let data_len = r2.take_u64() as usize;
        let data_bytes = &after_mime[16..];
        if data_bytes.len() < data_len {
            return Err(TesError::BufferTooSmall {
                structure: "ImagePayload.data",
                need: data_len,
                got: data_bytes.len(),
            });
        }
        if data_bytes.len() != data_len {
            return Err(TesError::InvalidImage {
                message: format!(
                    "trailing bytes after image data: got {}, expected {data_len}",
                    data_bytes.len()
                ),
            });
        }
        let payload = Self {
            media_type,
            width_px,
            height_px,
            data: data_bytes.to_vec(),
        };
        payload.validate()?;
        Ok(payload)
    }
}

/// Semantic placement for a figure use (theme maps to CSS).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ImagePlacement {
    /// Block in normal flow.
    #[default]
    Flow,
    /// Edge-to-edge within the content column.
    FullWidth,
    /// Float toward the start (inline-start) side.
    FloatStart,
    /// Float toward the end (inline-end) side.
    FloatEnd,
    /// Inline with surrounding text.
    Inline,
    /// Named template/slide region.
    Region {
        /// Region name from the template pack.
        name: String,
    },
    /// Decorative / background (still requires alt for a11y verify).
    Background,
}

impl ImagePlacement {
    /// Stable `snake_case` name for HTML `data-placement`.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Flow => "flow",
            Self::FullWidth => "full_width",
            Self::FloatStart => "float_start",
            Self::FloatEnd => "float_end",
            Self::Inline => "inline",
            Self::Region { .. } => "region",
            Self::Background => "background",
        }
    }
}

/// One contextual use of an image chunk (chunk type `7`, reading-order).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FigureRef {
    /// Target [`ImagePayload`] chunk id in this file.
    pub image_chunk_id: u64,
    /// Required alternative text.
    pub alt_text: String,
    /// Optional caption shown with the figure.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caption: Option<String>,
    /// Semantic placement hint for themes.
    #[serde(default)]
    pub placement: ImagePlacement,
}

impl FigureRef {
    /// Validate alt text and string bounds.
    ///
    /// # Errors
    ///
    /// Returns [`TesError::InvalidFigure`] if `image_chunk_id`, alt text,
    /// caption, or region name fails validation.
    pub fn validate(&self) -> Result<()> {
        if self.image_chunk_id == 0 {
            return Err(TesError::InvalidFigure {
                message: "image_chunk_id must be non-zero".into(),
            });
        }
        let alt = self.alt_text.trim();
        if alt.is_empty() {
            return Err(TesError::InvalidFigure {
                message: "alt_text is required".into(),
            });
        }
        if self.alt_text.len() > IMAGE_STRING_MAX {
            return Err(TesError::InvalidFigure {
                message: format!("alt_text exceeds {IMAGE_STRING_MAX} bytes"),
            });
        }
        if let Some(caption) = &self.caption
            && caption.len() > IMAGE_STRING_MAX
        {
            return Err(TesError::InvalidFigure {
                message: format!("caption exceeds {IMAGE_STRING_MAX} bytes"),
            });
        }
        if let ImagePlacement::Region { name } = &self.placement
            && (name.is_empty() || name.len() > IMAGE_STRING_MAX)
        {
            return Err(TesError::InvalidFigure {
                message: "region name must be non-empty and within limits".into(),
            });
        }
        Ok(())
    }

    /// Serialize as UTF-8 JSON.
    ///
    /// # Errors
    ///
    /// Returns validation errors from [`Self::validate`], or [`TesError::Json`]
    /// if serialization fails.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        self.validate()?;
        Ok(serde_json::to_vec(self)?)
    }

    /// Parse a figure payload from UTF-8 JSON.
    ///
    /// # Errors
    ///
    /// Returns [`TesError::Json`] on malformed JSON, or validation errors from
    /// [`Self::validate`].
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let figure: Self = serde_json::from_slice(bytes)?;
        figure.validate()?;
        Ok(figure)
    }
}

/// Standard Base64 (RFC 4648) for data-URI HTML embeds.
pub fn base64_encode(data: &[u8]) -> String {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = u32::from(chunk[0]);
        let b1 = chunk.get(1).copied().map_or(0, u32::from);
        let b2 = chunk.get(2).copied().map_or(0, u32::from);
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[((n >> 18) & 0x3F) as usize] as char);
        out.push(TABLE[((n >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            out.push(TABLE[((n >> 6) & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(TABLE[(n & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

/// Decode standard Base64 (RFC 4648), ignoring whitespace.
///
/// # Errors
///
/// Returns an error string when the input is truncated or contains invalid chars.
pub fn base64_decode(input: &str) -> std::result::Result<Vec<u8>, String> {
    fn val(c: u8) -> std::result::Result<u8, String> {
        match c {
            b'A'..=b'Z' => Ok(c - b'A'),
            b'a'..=b'z' => Ok(c - b'a' + 26),
            b'0'..=b'9' => Ok(c - b'0' + 52),
            b'+' => Ok(62),
            b'/' => Ok(63),
            other => Err(format!("invalid base64 byte {other}")),
        }
    }

    let filtered: Vec<u8> = input.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    if !filtered.len().is_multiple_of(4) {
        return Err("base64 length not a multiple of 4".into());
    }
    let mut out = Vec::with_capacity(filtered.len() / 4 * 3);
    for chunk in filtered.chunks_exact(4) {
        let pad = usize::from(chunk[2] == b'=') + usize::from(chunk[3] == b'=');
        let n0 = val(chunk[0])?;
        let n1 = val(chunk[1])?;
        let n2 = if chunk[2] == b'=' { 0 } else { val(chunk[2])? };
        let n3 = if chunk[3] == b'=' { 0 } else { val(chunk[3])? };
        let n =
            (u32::from(n0) << 18) | (u32::from(n1) << 12) | (u32::from(n2) << 6) | u32::from(n3);
        out.push(((n >> 16) & 0xff) as u8);
        if pad < 2 {
            out.push(((n >> 8) & 0xff) as u8);
        }
        if pad < 1 {
            out.push((n & 0xff) as u8);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_payload_round_trip() {
        let img = ImagePayload {
            media_type: "image/png".into(),
            width_px: 2,
            height_px: 2,
            data: vec![1, 2, 3, 4],
        };
        let bytes = img.to_bytes().unwrap();
        let back = ImagePayload::from_bytes(&bytes).unwrap();
        assert_eq!(back, img);
    }

    #[test]
    fn figure_ref_round_trip_and_requires_alt() {
        let figure = FigureRef {
            image_chunk_id: 3,
            alt_text: "Forest trail".into(),
            caption: Some("Morning light".into()),
            placement: ImagePlacement::FullWidth,
        };
        let bytes = figure.to_bytes().unwrap();
        let back = FigureRef::from_bytes(&bytes).unwrap();
        assert_eq!(back, figure);

        let bad = FigureRef {
            image_chunk_id: 3,
            alt_text: "   ".into(),
            caption: None,
            placement: ImagePlacement::Flow,
        };
        assert!(matches!(
            bad.to_bytes(),
            Err(TesError::InvalidFigure { .. })
        ));
    }

    #[test]
    fn base64_known_vector() {
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
    }
}
