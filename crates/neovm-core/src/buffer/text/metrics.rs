use crate::buffer::position::{CharLen, CharPos0, EmacsByteLen, EmacsBytePos};

use super::TextExtent;

/// Backend-neutral text extent in GNU Emacs coordinate spaces.
///
/// `chars` and `emacs_bytes` are lengths. Concrete backends may have a
/// different physical storage byte coordinate, but that must not leak through
/// this type.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct TextMetrics {
    chars: CharLen,
    emacs_bytes: EmacsByteLen,
}

impl TextMetrics {
    pub const ZERO: Self = Self {
        chars: CharLen::ZERO,
        emacs_bytes: EmacsByteLen::ZERO,
    };

    #[cfg(test)]
    pub(in crate::buffer) const fn from_usize(chars: usize, emacs_bytes: usize) -> Self {
        Self {
            chars: CharLen::new(chars),
            emacs_bytes: EmacsByteLen::new(emacs_bytes),
        }
    }

    pub const fn from_lengths(chars: CharLen, emacs_bytes: EmacsByteLen) -> Self {
        Self { chars, emacs_bytes }
    }

    pub const fn from_extent(extent: TextExtent) -> Self {
        Self {
            chars: extent.chars(),
            emacs_bytes: extent.emacs_bytes(),
        }
    }

    pub const fn add_metrics(self, other: Self) -> Self {
        Self {
            chars: self.chars.add_len(other.chars),
            emacs_bytes: self.emacs_bytes.add_len(other.emacs_bytes),
        }
    }

    pub const fn add_extent(self, extent: TextExtent) -> Self {
        Self {
            chars: self.chars.add_len(extent.chars()),
            emacs_bytes: self.emacs_bytes.add_len(extent.emacs_bytes()),
        }
    }

    pub const fn char_len(self) -> CharLen {
        self.chars
    }

    pub const fn emacs_byte_len(self) -> EmacsByteLen {
        self.emacs_bytes
    }

    pub(in crate::buffer) const fn chars_usize(self) -> usize {
        self.chars.get()
    }

    pub(in crate::buffer) const fn emacs_bytes_usize(self) -> usize {
        self.emacs_bytes.get()
    }

    pub const fn char_end(self) -> CharPos0 {
        CharPos0::ZERO.add_len(self.chars)
    }

    pub const fn emacs_byte_end(self) -> EmacsBytePos {
        EmacsBytePos::ZERO.add_len(self.emacs_bytes)
    }

    pub const fn is_empty(self) -> bool {
        self.chars.get() == 0 && self.emacs_bytes.get() == 0
    }
}
