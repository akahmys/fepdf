//! Big-endian integers read out of a font program: `None` where the program ends before
//! the integer does.
//!
//! A font program is the file's to say anything in, and every offset in one is a number
//! it chose. Three modules carried their own copy of these two; one reads past the end
//! of nothing.

/// The `uint16` at `at`.
pub(crate) fn read_u16(data: &[u8], at: usize) -> Option<u16> {
    data.get(at..at.checked_add(2)?)?.try_into().ok().map(u16::from_be_bytes)
}

/// The `uint32` at `at`.
pub(crate) fn read_u32(data: &[u8], at: usize) -> Option<u32> {
    data.get(at..at.checked_add(4)?)?.try_into().ok().map(u32::from_be_bytes)
}
