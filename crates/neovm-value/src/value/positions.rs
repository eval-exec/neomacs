//! Value-layer position vocabulary: character positions, character lengths
//! and character ranges. The buffer-dependent conversions stay in
//! `crate::buffer::position` as inherent impls.

/// 0-based internal character position (first character = 0).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct CharPos0(pub(crate) usize);

/// Count of logical Emacs characters.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct CharLen(pub(crate) usize);

/// Half-open internal character range `[start, end)`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CharRange {
    start: CharPos0,
    end: CharPos0,
}

/// 1-based Lisp character position (first character = 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct LispCharPos1(pub(crate) i64);

impl CharPos0 {
    pub const ZERO: Self = Self(0);

    pub const fn new(pos: usize) -> Self {
        Self(pos)
    }

    pub const fn get(self) -> usize {
        self.0
    }

    /// Convert to a 1-based Lisp position.
    pub fn to_lisp(self) -> LispCharPos1 {
        LispCharPos1(self.0 as i64 + 1)
    }

    /// Convert from a 1-based Lisp position (clamped to 0).
    pub fn from_lisp(p: LispCharPos1) -> Self {
        Self((p.0 - 1).max(0) as usize)
    }

    pub const fn add_len(self, len: CharLen) -> Self {
        Self(self.0 + len.get())
    }

    pub const fn saturating_sub_len(self, len: CharLen) -> Self {
        Self(self.0.saturating_sub(len.get()))
    }

    pub const fn saturating_offset_from(self, start: Self) -> CharLen {
        CharLen::new(self.0.saturating_sub(start.0))
    }
}

impl CharLen {
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

impl CharRange {
    pub const fn new(start: CharPos0, end: CharPos0) -> Self {
        Self { start, end }
    }

    pub const fn from_start_len(start: CharPos0, len: CharLen) -> Self {
        Self {
            start,
            end: start.add_len(len),
        }
    }

    #[cfg(any(test, feature = "test-support"))]
    pub const fn from_usize(start: usize, end: usize) -> Self {
        Self {
            start: CharPos0::new(start),
            end: CharPos0::new(end),
        }
    }

    pub const fn start(self) -> CharPos0 {
        self.start
    }

    pub const fn end(self) -> CharPos0 {
        self.end
    }

    pub fn start_lisp(self) -> LispCharPos1 {
        self.start.to_lisp()
    }

    pub fn end_lisp(self) -> LispCharPos1 {
        self.end.to_lisp()
    }

    pub const fn len(self) -> CharLen {
        CharLen::new(self.end.get().saturating_sub(self.start.get()))
    }

    pub const fn is_empty(self) -> bool {
        self.start.get() >= self.end.get()
    }
}

impl From<usize> for CharPos0 {
    fn from(pos: usize) -> Self {
        Self(pos)
    }
}

impl From<CharPos0> for usize {
    fn from(p: CharPos0) -> Self {
        p.0
    }
}

impl From<usize> for CharLen {
    fn from(len: usize) -> Self {
        Self(len)
    }
}

impl From<CharLen> for usize {
    fn from(len: CharLen) -> Self {
        len.0
    }
}

pub(crate) use crate::heap_types::byte_positions::{EmacsByteDelta, apply_signed_delta};
pub use crate::heap_types::byte_positions::{
    EmacsByteLen, EmacsBytePos, EmacsBytePositionView, EmacsByteRange, LispBytePos1,
};

impl LispCharPos1 {
    pub const ONE: Self = Self(1);

    pub const fn new(pos: i64) -> Self {
        Self(pos)
    }

    pub fn from_one_based_usize(pos: usize) -> Self {
        Self(i64::try_from(pos.max(1)).expect("Lisp character position fits i64"))
    }

    pub fn to_one_based_usize(self) -> usize {
        usize::try_from(self.as_i64().max(1)).expect("Lisp character position fits usize")
    }

    /// Convert to 0-based internal char position.
    pub fn to_char_pos(self) -> CharPos0 {
        CharPos0::from_lisp(self)
    }

    /// The raw i64 value.
    pub fn as_i64(self) -> i64 {
        self.0
    }
}
