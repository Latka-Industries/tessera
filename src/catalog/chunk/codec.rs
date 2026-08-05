//! Wire framing for text (and shared u32-prefixed) chunk payloads.

use argus::{LeReader, LeWriter};

use crate::error::{Result, TesError};

use super::text::{TEXT_HEADER_MAX_BYTES, TextHeader};

/// Frame `u32 LE length(head) | head | tail` (text headers, attachment meta, …).
///
/// # Panics
///
/// Panics if `head.len()` does not fit in `u32` (not reachable for Tessera
/// payload sizes).
#[must_use]
pub fn encode_u32_prefixed(head: &[u8], tail: &[u8]) -> Vec<u8> {
    let len = u32::try_from(head.len()).expect("prefixed head exceeds u32::MAX");
    let mut out = Vec::with_capacity(4 + head.len() + tail.len());
    let mut len_buf = [0u8; 4];
    {
        let mut w = LeWriter::new(&mut len_buf);
        w.put_u32(len);
    }
    out.extend_from_slice(&len_buf);
    out.extend_from_slice(head);
    out.extend_from_slice(tail);
    out
}

/// Split a [`encode_u32_prefixed`] buffer into `(head, tail)`.
///
/// # Errors
///
/// Returns [`TesError::BufferTooSmall`] when the buffer is truncated.
pub fn split_u32_prefixed<'a>(
    bytes: &'a [u8],
    structure: &'static str,
) -> Result<(&'a [u8], &'a [u8])> {
    let mut r = LeReader::require(bytes, structure, 4)?;
    let head_len = r.take_u32() as usize;
    let rest = &bytes[4..];
    if rest.len() < head_len {
        return Err(TesError::BufferTooSmall {
            structure,
            need: head_len,
            got: rest.len(),
        });
    }
    Ok((&rest[..head_len], &rest[head_len..]))
}

fn ensure_text_header_size(len: usize) -> Result<()> {
    if len > TEXT_HEADER_MAX_BYTES {
        Err(TesError::TextHeaderTooLarge {
            len,
            limit: TEXT_HEADER_MAX_BYTES,
        })
    } else {
        Ok(())
    }
}

/// Encode a text chunk payload: `u32 header_len | header JSON | UTF-8 body`.
///
/// # Errors
///
/// Returns validation errors from [`TextHeader::validate`], [`TesError::Json`]
/// if the header cannot be serialized, or [`TesError::TextHeaderTooLarge`] if
/// it exceeds [`TEXT_HEADER_MAX_BYTES`].
pub fn encode_text_payload(header: &TextHeader, body: &str) -> Result<Vec<u8>> {
    header.validate(body)?;
    let header_bytes = serde_json::to_vec(header)?;
    ensure_text_header_size(header_bytes.len())?;
    Ok(encode_u32_prefixed(&header_bytes, body.as_bytes()))
}

/// Decode a text chunk payload into `(header, body)`.
///
/// # Errors
///
/// Returns [`TesError::BufferTooSmall`] if the buffer is truncated,
/// [`TesError::TextHeaderTooLarge`] if the header exceeds
/// [`TEXT_HEADER_MAX_BYTES`], [`TesError::Json`] for a bad header,
/// [`TesError::InvalidUtf8`] if the body is not UTF-8, or validation errors
/// from [`TextHeader::validate`].
pub fn decode_text_payload(bytes: &[u8]) -> Result<(TextHeader, String)> {
    let (header_bytes, body_bytes) = split_u32_prefixed(bytes, "TextChunkPayload")?;
    ensure_text_header_size(header_bytes.len())?;
    let header: TextHeader = serde_json::from_slice(header_bytes)?;
    let body = std::str::from_utf8(body_bytes)
        .map_err(|_| TesError::InvalidUtf8 {
            structure: "TextChunkPayload.body",
        })?
        .to_owned();
    header.validate(&body)?;
    Ok((header, body))
}
