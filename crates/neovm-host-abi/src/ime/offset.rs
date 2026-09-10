//! Input-method offsets are not editor positions or UTF-8 byte counts.

/// Nonnegative UTF-16 code-unit offset, as supplied by platform input methods.
/// Construction does not validate an offset against any particular snapshot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImeUtf16Offset(u32);

impl ImeUtf16Offset {
    /// Reject platform sentinel values such as -1, rather than clamping them.
    pub fn new(units: i32) -> Option<Self> {
        Some(Self(units.try_into().ok()?))
    }

    pub(super) fn byte_offset(self, text: &str) -> Option<usize> {
        let mut remaining = self.0;
        for (byte, character) in text.char_indices() {
            if remaining == 0 {
                return Some(byte);
            }
            remaining = remaining.checked_sub(character.len_utf16() as u32)?;
        }
        (remaining == 0).then_some(text.len())
    }
}
