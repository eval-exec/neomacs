//! GNU's character-category word-wrap rules, independent of row production.

use crate::buffer::Buffer;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WordWrapMode {
    #[default]
    Whitespace,
    Category,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WordWrapDirection {
    #[default]
    Forward,
    Reversed,
}

/// Only the three categories consumed by GNU's char_can_wrap_* predicates.
/// This projection owns no Lisp objects and can cross a worker boundary.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WordWrapCategories(u8);

impl WordWrapCategories {
    pub(crate) fn from_bits(bits: u128) -> Self {
        Self(
            u8::from(bits & (1 << b'|') != 0)
                | (u8::from(bits & (1 << b'>') != 0) << 1)
                | (u8::from(bits & (1 << b'<') != 0) << 2),
        )
    }

    pub fn for_buffer(buffer: &Buffer, code: u32) -> Self {
        Self::from_bits(
            crate::emacs_core::category::character_category_bits(buffer, code).unwrap_or(0),
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WordWrapOpportunity {
    before: bool,
    after: bool,
}

impl WordWrapOpportunity {
    pub const fn before(self) -> bool {
        self.before
    }

    pub const fn after(self) -> bool {
        self.after
    }
}

impl WordWrapMode {
    /// GNU xdisp.c:577–617. A reversed glyph row swaps the kinsoku ends.
    pub fn opportunity(
        self,
        code: u32,
        categories: WordWrapCategories,
        direction: WordWrapDirection,
    ) -> WordWrapOpportunity {
        let whitespace = matches!(code, 0x20 | 0x09);
        let (not_at_start, not_at_end) = match direction {
            WordWrapDirection::Forward => (2, 4),
            WordWrapDirection::Reversed => (4, 2),
        };
        WordWrapOpportunity {
            before: !whitespace && (self == Self::Whitespace || categories.0 & not_at_start == 0),
            after: whitespace
                || (self == Self::Category
                    && categories.0 & 1 != 0
                    && categories.0 & not_at_end == 0),
        }
    }
}
