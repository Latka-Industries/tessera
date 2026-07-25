//! Little-endian wire primitives and alignment helpers.
//!
//! All Tessera on-disk integers are little-endian (see `docs/layout_v0.md`).
//! These helpers keep the fixed-layout encoders/decoders free of repetitive
//! slice arithmetic and bounds handling.

/// Round `n` up to the next multiple of 8.
///
/// Catalog and index regions are 8-byte aligned by the reference writer.
#[inline]
#[must_use]
pub const fn align8(n: u64) -> u64 {
    (n + 7) & !7
}

/// Cursor that writes little-endian primitives into a mutable byte slice.
///
/// The cursor panics on overflow; callers size their buffers to the exact
/// fixed structure length, so an overflow indicates a layout bug rather than
/// runtime input error.
pub struct LeWriter<'a> {
    buf: &'a mut [u8],
    pos: usize,
}

impl<'a> LeWriter<'a> {
    /// Create a writer over `buf`, starting at offset 0.
    #[inline]
    pub fn new(buf: &'a mut [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    /// Current write offset.
    #[inline]
    #[must_use]
    pub fn position(&self) -> usize {
        self.pos
    }

    /// Write raw bytes verbatim.
    #[inline]
    pub fn put_bytes(&mut self, bytes: &[u8]) {
        self.buf[self.pos..self.pos + bytes.len()].copy_from_slice(bytes);
        self.pos += bytes.len();
    }

    /// Write a `u32` in little-endian order.
    #[inline]
    pub fn put_u32(&mut self, v: u32) {
        self.put_bytes(&v.to_le_bytes());
    }

    /// Write a `u64` in little-endian order.
    #[inline]
    pub fn put_u64(&mut self, v: u64) {
        self.put_bytes(&v.to_le_bytes());
    }

    /// Write `count` zero bytes (reserved padding).
    #[inline]
    pub fn put_zeros(&mut self, count: usize) {
        self.buf[self.pos..self.pos + count].fill(0);
        self.pos += count;
    }
}

/// Cursor that reads little-endian primitives from a byte slice.
///
/// Length is validated once by the caller (via [`LeReader::require`]); the
/// typed readers then advance without re-checking, since fixed structures are
/// decoded from a slice already known to be large enough.
pub struct LeReader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> LeReader<'a> {
    /// Create a reader over `buf`, starting at offset 0.
    #[inline]
    #[must_use]
    pub fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    /// Ensure `buf` holds at least `need` bytes for `structure`.
    #[inline]
    pub fn require(
        buf: &'a [u8],
        structure: &'static str,
        need: usize,
    ) -> crate::error::Result<Self> {
        if buf.len() < need {
            return Err(crate::error::TesError::BufferTooSmall {
                structure,
                need,
                got: buf.len(),
            });
        }
        Ok(Self::new(buf))
    }

    /// Current read offset.
    #[inline]
    #[must_use]
    pub fn position(&self) -> usize {
        self.pos
    }

    /// Read a fixed 4-byte array (e.g. a magic tag).
    #[inline]
    pub fn take_4(&mut self) -> [u8; 4] {
        let mut out = [0u8; 4];
        out.copy_from_slice(&self.buf[self.pos..self.pos + 4]);
        self.pos += 4;
        out
    }

    /// Read a fixed 16-byte array (e.g. a UUID).
    #[inline]
    pub fn take_16(&mut self) -> [u8; 16] {
        let mut out = [0u8; 16];
        out.copy_from_slice(&self.buf[self.pos..self.pos + 16]);
        self.pos += 16;
        out
    }

    /// Read a little-endian `u32`.
    #[inline]
    pub fn take_u32(&mut self) -> u32 {
        let mut b = [0u8; 4];
        b.copy_from_slice(&self.buf[self.pos..self.pos + 4]);
        self.pos += 4;
        u32::from_le_bytes(b)
    }

    /// Read a little-endian `u64`.
    #[inline]
    pub fn take_u64(&mut self) -> u64 {
        let mut b = [0u8; 8];
        b.copy_from_slice(&self.buf[self.pos..self.pos + 8]);
        self.pos += 8;
        u64::from_le_bytes(b)
    }

    /// Skip `count` bytes (reserved fields).
    #[inline]
    pub fn skip(&mut self, count: usize) {
        self.pos += count;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn align8_rounds_up() {
        assert_eq!(align8(0), 0);
        assert_eq!(align8(1), 8);
        assert_eq!(align8(8), 8);
        assert_eq!(align8(9), 16);
        assert_eq!(align8(63), 64);
    }

    #[test]
    fn writer_reader_round_trip() {
        let mut buf = [0u8; 24];
        let mut w = LeWriter::new(&mut buf);
        w.put_bytes(b"TESS");
        w.put_u32(0xDEAD_BEEF);
        w.put_u64(0x0102_0304_0506_0708);
        w.put_zeros(8);
        assert_eq!(w.position(), 24);

        let mut r = LeReader::new(&buf);
        assert_eq!(&r.take_4(), b"TESS");
        assert_eq!(r.take_u32(), 0xDEAD_BEEF);
        assert_eq!(r.take_u64(), 0x0102_0304_0506_0708);
        r.skip(8);
        assert_eq!(r.position(), 24);
    }

    #[test]
    fn require_rejects_short_buffer() {
        let buf = [0u8; 4];
        let result = LeReader::require(&buf, "thing", 8);
        assert!(matches!(
            result,
            Err(crate::error::TesError::BufferTooSmall {
                need: 8,
                got: 4,
                ..
            })
        ));
    }
}
