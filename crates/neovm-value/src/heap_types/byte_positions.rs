//! Logical Emacs byte-position vocabulary; buffer storage and authority stay upper.

use crate::value::positions::{CharPos0, LispCharPos1};

/// 0-based logical Emacs byte position.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct EmacsBytePos(usize);

/// Count of logical Emacs bytes.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct EmacsByteLen(usize);

/// Signed change in logical Emacs bytes.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(transparent)]
#[doc(hidden)]
pub struct EmacsByteDelta(isize);

/// 1-based Lisp byte position (first byte = 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct LispBytePos1(i64);

/// Half-open logical Emacs byte range `[start, end)`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EmacsByteRange {
    start: EmacsBytePos,
    end: EmacsBytePos,
}

impl EmacsBytePos {
    pub const ZERO: Self = Self(0);

    pub const fn new(pos: usize) -> Self {
        Self(pos)
    }

    pub const fn get(self) -> usize {
        self.0
    }

    /// Convert to a 1-based Lisp position.
    /// Requires the buffer for byte-to-char conversion.
    pub fn to_lisp(self, text: &impl EmacsBytePositionView) -> LispCharPos1 {
        text.byte_to_char(self).to_lisp()
    }

    /// Convert to a 1-based Lisp byte position.
    pub const fn to_lisp_byte_pos(self) -> LispBytePos1 {
        LispBytePos1(self.0 as i64 + 1)
    }

    /// Convert from a 1-based Lisp position.
    /// Requires the buffer for char-to-byte conversion.
    pub fn from_lisp(p: LispCharPos1, text: &impl EmacsBytePositionView) -> Self {
        text.char_to_byte(CharPos0::from_lisp(p))
    }

    pub const fn add_len(self, len: EmacsByteLen) -> Self {
        Self(self.0 + len.get())
    }

    pub const fn saturating_sub_len(self, len: EmacsByteLen) -> Self {
        Self(self.0.saturating_sub(len.get()))
    }

    pub const fn saturating_offset_from(self, start: Self) -> EmacsByteLen {
        EmacsByteLen::new(self.0.saturating_sub(start.0))
    }
}

impl EmacsByteLen {
    pub const ZERO: Self = Self(0);

    pub const fn new(len: usize) -> Self {
        Self(len)
    }

    pub const fn get(self) -> usize {
        self.0
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub const fn add_len(self, len: Self) -> Self {
        Self(self.0 + len.0)
    }

    pub const fn saturating_sub(self, len: Self) -> Self {
        Self(self.0.saturating_sub(len.0))
    }
}

impl EmacsByteDelta {
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(crate) const ZERO: Self = Self(0);

    #[doc(hidden)]
    pub fn insertion(len: EmacsByteLen) -> Self {
        Self(len.get() as isize)
    }

    #[doc(hidden)]
    pub fn deletion(len: EmacsByteLen) -> Self {
        Self(-(len.get() as isize))
    }

    #[doc(hidden)]
    pub fn replacement(old_len: EmacsByteLen, new_len: EmacsByteLen) -> Self {
        Self(new_len.get() as isize - old_len.get() as isize)
    }

    #[inline(always)]
    #[doc(hidden)]
    pub fn apply_to_pos(self, pos: EmacsBytePos) -> EmacsBytePos {
        if self.is_zero() {
            return pos;
        }
        EmacsBytePos::new(apply_signed_delta(pos.get(), self.0))
    }

    pub(crate) const fn is_zero(self) -> bool {
        self.0 == 0
    }

    #[inline(always)]
    pub(crate) fn combine(self, other: Self) -> Self {
        if self.is_zero() {
            return other;
        }
        if other.is_zero() {
            return self;
        }
        Self(
            self.0
                .checked_add(other.0)
                .expect("buffer text edit delta overflow"),
        )
    }

    pub(crate) fn apply_to_range(self, range: EmacsByteRange) -> EmacsByteRange {
        EmacsByteRange::new(
            self.apply_to_pos(range.start()),
            self.apply_to_pos(range.end()),
        )
    }
}

#[doc(hidden)]
pub fn apply_signed_delta(value: usize, delta: isize) -> usize {
    if delta >= 0 {
        value
            .checked_add(delta as usize)
            .expect("buffer text edit position overflow")
    } else {
        value
            .checked_sub(delta.unsigned_abs())
            .expect("buffer text edit position underflow")
    }
}

impl EmacsByteRange {
    pub const EMPTY: Self = Self {
        start: EmacsBytePos::ZERO,
        end: EmacsBytePos::ZERO,
    };

    pub const fn new(start: EmacsBytePos, end: EmacsBytePos) -> Self {
        Self { start, end }
    }

    pub fn ordered(start: EmacsBytePos, end: EmacsBytePos) -> Self {
        Self::new(start.min(end), start.max(end))
    }

    pub const fn from_start_len(start: EmacsBytePos, len: EmacsByteLen) -> Self {
        Self {
            start,
            end: start.add_len(len),
        }
    }

    /// Convenience `usize`-based constructor, primarily for tests. Wraps
    /// the typed [`EmacsByteRange::new`]. Not `#[cfg(test)]`-gated because
    /// downstream crates' test builds (e.g. neomacs-layout-engine) compile
    /// neovm-core without `cfg(test)`, so a test-gated item is invisible
    /// to them. Prefer the typed constructors in production code.
    pub const fn from_usize(start: usize, end: usize) -> Self {
        Self {
            start: EmacsBytePos::new(start),
            end: EmacsBytePos::new(end),
        }
    }

    pub const fn start(self) -> EmacsBytePos {
        self.start
    }

    pub const fn end(self) -> EmacsBytePos {
        self.end
    }

    pub const fn len(self) -> EmacsByteLen {
        EmacsByteLen::new(self.end.get().saturating_sub(self.start.get()))
    }

    pub const fn is_empty(self) -> bool {
        self.start.get() >= self.end.get()
    }
}

impl LispBytePos1 {
    pub const fn new(pos: i64) -> Self {
        Self(pos)
    }

    /// Convert to a 0-based internal Emacs byte position.
    pub fn to_emacs_byte_pos(self) -> EmacsBytePos {
        EmacsBytePos::new((self.0 - 1).max(0) as usize)
    }

    pub fn as_i64(self) -> i64 {
        self.0
    }
}

impl From<usize> for EmacsBytePos {
    fn from(pos: usize) -> Self {
        Self(pos)
    }
}

impl From<EmacsBytePos> for usize {
    fn from(p: EmacsBytePos) -> Self {
        p.0
    }
}

impl From<usize> for EmacsByteLen {
    fn from(len: usize) -> Self {
        Self(len)
    }
}

impl From<EmacsByteLen> for usize {
    fn from(len: EmacsByteLen) -> Self {
        len.0
    }
}

/// A call-local coordinate view. Implementations perform no callback or publication.
/// This vocabulary grants no buffer ownership or mutation authority.
pub trait EmacsBytePositionView {
    fn byte_to_char(&self, position: EmacsBytePos) -> CharPos0;
    fn char_to_byte(&self, position: CharPos0) -> EmacsBytePos;
}
