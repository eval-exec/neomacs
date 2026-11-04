use crate::buffer::position::{
    CharDelta, CharLen, CharPos0, CharRange, EmacsByteDelta, EmacsByteLen, EmacsBytePos,
    EmacsByteRange, TextPositionAnchor,
};

/// Logical size of inserted or deleted buffer text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextExtent {
    chars: CharLen,
    emacs_bytes: EmacsByteLen,
}

/// Signed logical size change produced by a structural text edit.
///
/// GNU edit code updates char and byte coordinates together (`PT` with
/// `PT_BYTE`, marker `charpos` with `bytepos`, and so on).  Keeping the signed
/// delta typed prevents replacement paths from shifting only one coordinate
/// space when the byte and character lengths differ.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::buffer) struct TextExtentDelta {
    chars: CharDelta,
    emacs_bytes: EmacsByteDelta,
}

/// Insertion point plus the text size known by the caller.
///
/// GNU `insert_1_both` receives character and byte lengths separately.  This
/// type keeps that contract explicit at the Rust boundary.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextInsertion {
    byte_pos: EmacsBytePos,
    char_pos: CharPos0,
    extent: TextExtent,
}

/// Replacement range plus the logical size of the inserted text.
///
/// GNU `replace_range` keeps the deleted range and inserted size together
/// while it updates the gap, markers, intervals, undo, and redisplay state.
/// Carrying the same measured shape here prevents individual frontends or
/// backend paths from re-deriving byte and character counts differently.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextReplacement {
    old_range: TextEditRange,
    new_extent: TextExtent,
}

/// A measured text change for modification hooks and edit notifications.
///
/// GNU `signal_before_change` receives the old range, while
/// `signal_after_change` receives the same start, the inserted end in the new
/// buffer, and the old character length.  This type keeps those values tied to
/// the measured edit instead of passing raw byte triples through callers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextChange {
    old_range: TextEditRange,
    new_extent: TextExtent,
    after_start_byte: EmacsBytePos,
    after_new_extent: TextExtent,
    after_old_char_len: CharLen,
}

/// Two non-overlapping edit ranges that will be transposed.
///
/// GNU `transpose-regions` receives character positions from Lisp, computes
/// byte positions, and then keeps both coordinate spaces through the rest of
/// the operation. This type makes that paired range shape explicit so marker,
/// property, undo, and byte-storage movement cannot accidentally use different
/// region boundaries.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextTransposition {
    first: TextEditRange,
    second: TextEditRange,
}

/// Half-open edit range with both byte and character coordinates.
///
/// GNU `del_range_both` carries `from`, `from_byte`, `to`, and `to_byte`.
/// Keeping the same shape here avoids recomputing one coordinate space from the
/// other and makes byte/character mixups visible in the type signature.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextEditRange {
    byte_range: EmacsByteRange,
    char_start: CharPos0,
    char_end: CharPos0,
}

impl TextExtent {
    pub const ZERO: Self = Self {
        chars: CharLen::ZERO,
        emacs_bytes: EmacsByteLen::ZERO,
    };

    pub const fn new(chars: CharLen, emacs_bytes: EmacsByteLen) -> Self {
        Self { chars, emacs_bytes }
    }

    #[cfg(test)]
    pub(in crate::buffer) const fn from_usize(chars: usize, emacs_bytes: usize) -> Self {
        Self {
            chars: CharLen::new(chars),
            emacs_bytes: EmacsByteLen::new(emacs_bytes),
        }
    }

    pub fn from_emacs_bytes(bytes: &[u8], multibyte: bool) -> Self {
        Self::new(
            super::emacs_char_count_bytes(bytes, multibyte),
            EmacsByteLen::new(bytes.len()),
        )
    }

    pub const fn chars(self) -> CharLen {
        self.chars
    }

    pub const fn emacs_bytes(self) -> EmacsByteLen {
        self.emacs_bytes
    }

    pub const fn is_empty(self) -> bool {
        self.chars.is_empty() && self.emacs_bytes.is_empty()
    }
}

impl TextExtentDelta {
    pub(in crate::buffer) fn insertion(extent: TextExtent) -> Self {
        Self {
            chars: CharDelta::insertion(extent.chars()),
            emacs_bytes: EmacsByteDelta::insertion(extent.emacs_bytes()),
        }
    }

    pub(in crate::buffer) fn deletion(extent: TextExtent) -> Self {
        Self {
            chars: CharDelta::deletion(extent.chars()),
            emacs_bytes: EmacsByteDelta::deletion(extent.emacs_bytes()),
        }
    }

    pub(in crate::buffer) fn replacement(old_extent: TextExtent, new_extent: TextExtent) -> Self {
        Self {
            chars: CharDelta::replacement(old_extent.chars(), new_extent.chars()),
            emacs_bytes: EmacsByteDelta::replacement(
                old_extent.emacs_bytes(),
                new_extent.emacs_bytes(),
            ),
        }
    }

    pub(in crate::buffer) fn apply_to_anchor(
        self,
        position: TextPositionAnchor,
    ) -> TextPositionAnchor {
        TextPositionAnchor::new(
            self.chars.apply_to_pos(position.char_pos()),
            self.emacs_bytes.apply_to_pos(position.emacs_byte_pos()),
        )
    }
}

impl TextInsertion {
    pub const fn new(byte_pos: EmacsBytePos, char_pos: CharPos0, extent: TextExtent) -> Self {
        Self {
            byte_pos,
            char_pos,
            extent,
        }
    }

    pub const fn at_anchor(anchor: TextPositionAnchor, extent: TextExtent) -> Self {
        Self {
            byte_pos: anchor.emacs_byte_pos(),
            char_pos: anchor.char_pos(),
            extent,
        }
    }

    #[cfg(test)]
    pub const fn from_usize(
        byte_pos: usize,
        char_pos: usize,
        chars: usize,
        emacs_bytes: usize,
    ) -> Self {
        Self {
            byte_pos: EmacsBytePos::new(byte_pos),
            char_pos: CharPos0::new(char_pos),
            extent: TextExtent::from_usize(chars, emacs_bytes),
        }
    }

    pub const fn byte_pos(self) -> EmacsBytePos {
        self.byte_pos
    }

    pub const fn char_pos(self) -> CharPos0 {
        self.char_pos
    }

    pub const fn start_anchor(self) -> TextPositionAnchor {
        TextPositionAnchor::new(self.char_pos, self.byte_pos)
    }

    pub const fn byte_end(self) -> EmacsBytePos {
        self.byte_pos.add_len(self.extent.emacs_bytes())
    }

    pub const fn char_end(self) -> CharPos0 {
        self.char_pos.add_len(self.extent.chars())
    }

    pub const fn end_anchor(self) -> TextPositionAnchor {
        TextPositionAnchor::new(self.char_end(), self.byte_end())
    }

    pub const fn extent(self) -> TextExtent {
        self.extent
    }
}

impl TextReplacement {
    pub const fn new(old_range: TextEditRange, new_extent: TextExtent) -> Self {
        Self {
            old_range,
            new_extent,
        }
    }

    pub const fn old_range(self) -> TextEditRange {
        self.old_range
    }

    pub const fn new_extent(self) -> TextExtent {
        self.new_extent
    }

    pub const fn byte_start(self) -> EmacsBytePos {
        self.old_range.byte_start()
    }

    pub const fn old_start_anchor(self) -> TextPositionAnchor {
        self.old_range.start_anchor()
    }

    pub const fn old_end_anchor(self) -> TextPositionAnchor {
        self.old_range.end_anchor()
    }

    pub const fn old_byte_len(self) -> EmacsByteLen {
        self.old_range.byte_len()
    }

    pub const fn old_char_len(self) -> CharLen {
        self.old_range.char_len()
    }

    pub const fn new_byte_len(self) -> EmacsByteLen {
        self.new_extent.emacs_bytes()
    }

    pub const fn new_char_len(self) -> CharLen {
        self.new_extent.chars()
    }

    pub const fn changed_chars(self) -> CharLen {
        let old = self.old_char_len().get();
        let new = self.new_char_len().get();
        CharLen::new(if old > new { old } else { new })
    }

    #[cfg(test)]
    pub const fn changed_chars_usize(self) -> usize {
        self.changed_chars().get()
    }
}

impl TextChange {
    pub const fn new(old_range: TextEditRange, new_extent: TextExtent) -> Self {
        Self {
            old_range,
            new_extent,
            after_start_byte: old_range.byte_start(),
            after_new_extent: new_extent,
            after_old_char_len: old_range.char_len(),
        }
    }

    pub const fn insertion(insertion: TextInsertion) -> Self {
        Self::new(
            TextEditRange::empty_at(insertion.byte_pos(), insertion.char_pos()),
            insertion.extent(),
        )
    }

    pub const fn deletion(range: TextEditRange) -> Self {
        Self::new(range, TextExtent::ZERO)
    }

    pub const fn unchanged_extent(range: TextEditRange) -> Self {
        Self::new(range, range.extent())
    }

    /// GNU `subst-char-in-region` runs `before-change-functions` over
    /// `[first-changed, original-end)`, but reports `after-change-functions`
    /// over `[first-changed, last-changed)`.  Keep that two-span shape explicit
    /// instead of widening the after-change notification.
    pub const fn unchanged_extent_with_after_range(
        before_range: TextEditRange,
        after_range: TextEditRange,
    ) -> Self {
        Self {
            old_range: before_range,
            new_extent: before_range.extent(),
            after_start_byte: after_range.byte_start(),
            after_new_extent: after_range.extent(),
            after_old_char_len: after_range.char_len(),
        }
    }

    pub const fn replacement(replacement: TextReplacement) -> Self {
        Self::new(replacement.old_range(), replacement.new_extent())
    }

    pub const fn old_range(self) -> TextEditRange {
        self.old_range
    }

    pub const fn new_extent(self) -> TextExtent {
        self.new_extent
    }

    pub const fn before_byte_range(self) -> EmacsByteRange {
        self.old_range.byte_range()
    }

    pub const fn before_start_byte(self) -> EmacsBytePos {
        self.old_range.byte_start()
    }

    pub const fn before_end_byte(self) -> EmacsBytePos {
        self.old_range.byte_end()
    }

    pub const fn after_start_byte(self) -> EmacsBytePos {
        self.after_start_byte
    }

    pub const fn after_end_byte(self) -> EmacsBytePos {
        self.after_start_byte
            .add_len(self.after_new_extent.emacs_bytes())
    }

    pub const fn after_byte_range(self) -> EmacsByteRange {
        EmacsByteRange::new(self.after_start_byte(), self.after_end_byte())
    }

    pub const fn old_char_len(self) -> CharLen {
        self.after_old_char_len
    }
}

impl TextTransposition {
    pub const fn new(first: TextEditRange, second: TextEditRange) -> Self {
        Self { first, second }
    }

    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub const fn from_usize(
        start1_byte: usize,
        end1_byte: usize,
        start1_char: usize,
        end1_char: usize,
        start2_byte: usize,
        end2_byte: usize,
        start2_char: usize,
        end2_char: usize,
    ) -> Self {
        Self {
            first: TextEditRange::from_usize(start1_byte, end1_byte, start1_char, end1_char),
            second: TextEditRange::from_usize(start2_byte, end2_byte, start2_char, end2_char),
        }
    }

    pub const fn first(self) -> TextEditRange {
        self.first
    }

    pub const fn second(self) -> TextEditRange {
        self.second
    }

    pub const fn byte_span(self) -> EmacsByteRange {
        EmacsByteRange::new(self.first.byte_start(), self.second.byte_end())
    }

    pub const fn char_span(self) -> CharRange {
        CharRange::new(self.first.char_start(), self.second.char_end())
    }

    pub const fn span_edit_range(self) -> TextEditRange {
        TextEditRange::from_start_end(self.first.start_anchor(), self.second.end_anchor())
    }

    pub const fn middle_byte_range(self) -> EmacsByteRange {
        EmacsByteRange::new(self.first.byte_end(), self.second.byte_start())
    }

    pub const fn middle_char_range(self) -> CharRange {
        CharRange::new(self.first.char_end(), self.second.char_start())
    }

    pub const fn second_destination_char_start(self) -> CharPos0 {
        self.first.char_start()
    }

    pub const fn middle_destination_char_start(self) -> CharPos0 {
        self.first.char_start().add_len(self.second_char_len())
    }

    pub const fn first_destination_char_start(self) -> CharPos0 {
        self.second
            .char_end()
            .saturating_sub_len(self.first_char_len())
    }

    pub const fn first_char_len(self) -> CharLen {
        self.first.char_len()
    }

    pub const fn second_char_len(self) -> CharLen {
        self.second.char_len()
    }

    pub const fn changed_chars(self) -> CharLen {
        self.char_span().len()
    }

    pub const fn same_char_len(self) -> bool {
        self.first_char_len().get() == self.second_char_len().get()
    }

    pub const fn adjacent(self) -> bool {
        self.first.byte_end().get() == self.second.byte_start().get()
    }

    pub const fn transpose_byte_pos(self, pos: EmacsBytePos) -> EmacsBytePos {
        EmacsBytePos::new(transpose_half_open_position(
            pos.get(),
            self.first.byte_start().get(),
            self.first.byte_end().get(),
            self.second.byte_start().get(),
            self.second.byte_end().get(),
        ))
    }

    pub const fn transpose_char_pos(self, pos: CharPos0) -> CharPos0 {
        CharPos0::new(transpose_half_open_position(
            pos.get(),
            self.first.char_start().get(),
            self.first.char_end().get(),
            self.second.char_start().get(),
            self.second.char_end().get(),
        ))
    }

    pub const fn transpose_anchor(self, anchor: TextPositionAnchor) -> TextPositionAnchor {
        TextPositionAnchor::new(
            self.transpose_char_pos(anchor.char_pos()),
            self.transpose_byte_pos(anchor.emacs_byte_pos()),
        )
    }
}

impl TextEditRange {
    pub const fn new(byte_range: EmacsByteRange, char_start: CharPos0, char_end: CharPos0) -> Self {
        Self {
            byte_range,
            char_start,
            char_end,
        }
    }

    pub const fn empty_at(byte_pos: EmacsBytePos, char_pos: CharPos0) -> Self {
        Self::new(EmacsByteRange::new(byte_pos, byte_pos), char_pos, char_pos)
    }

    pub const fn from_start_end(start: TextPositionAnchor, end: TextPositionAnchor) -> Self {
        Self {
            byte_range: EmacsByteRange::new(start.emacs_byte_pos(), end.emacs_byte_pos()),
            char_start: start.char_pos(),
            char_end: end.char_pos(),
        }
    }

    pub const fn from_start_extent(
        byte_start: EmacsBytePos,
        char_start: CharPos0,
        extent: TextExtent,
    ) -> Self {
        Self {
            byte_range: EmacsByteRange::from_start_len(byte_start, extent.emacs_bytes()),
            char_start,
            char_end: char_start.add_len(extent.chars()),
        }
    }

    #[cfg(test)]
    pub const fn from_usize(
        byte_start: usize,
        byte_end: usize,
        char_start: usize,
        char_end: usize,
    ) -> Self {
        Self {
            byte_range: EmacsByteRange::new(
                EmacsBytePos::new(byte_start),
                EmacsBytePos::new(byte_end),
            ),
            char_start: CharPos0::new(char_start),
            char_end: CharPos0::new(char_end),
        }
    }

    pub const fn byte_range(self) -> EmacsByteRange {
        self.byte_range
    }

    pub const fn byte_start(self) -> EmacsBytePos {
        self.byte_range.start()
    }

    pub const fn start_anchor(self) -> TextPositionAnchor {
        TextPositionAnchor::new(self.char_start, self.byte_range.start())
    }

    pub const fn byte_end(self) -> EmacsBytePos {
        self.byte_range.end()
    }

    pub const fn end_anchor(self) -> TextPositionAnchor {
        TextPositionAnchor::new(self.char_end, self.byte_range.end())
    }

    pub const fn char_start(self) -> CharPos0 {
        self.char_start
    }

    pub const fn char_end(self) -> CharPos0 {
        self.char_end
    }

    pub const fn char_range(self) -> CharRange {
        CharRange::new(self.char_start, self.char_end)
    }

    pub const fn byte_len(self) -> EmacsByteLen {
        self.byte_range.len()
    }

    pub(in crate::buffer) fn byte_index_range_relative_to(
        self,
        base: Self,
    ) -> std::ops::Range<usize> {
        debug_assert!(
            self.byte_start() >= base.byte_start() && self.byte_end() <= base.byte_end(),
            "relative byte index range must be contained in its base range"
        );
        let base_start = base.byte_start().get();
        self.byte_start().get() - base_start..self.byte_end().get() - base_start
    }

    pub(in crate::buffer) const fn subrange_from_start_offsets(
        self,
        byte_offset: EmacsByteLen,
        char_offset: CharLen,
        extent: TextExtent,
    ) -> Self {
        Self::from_start_extent(
            self.byte_start().add_len(byte_offset),
            self.char_start().add_len(char_offset),
            extent,
        )
    }

    pub const fn char_len(self) -> CharLen {
        CharLen::new(self.char_end.get().saturating_sub(self.char_start.get()))
    }

    pub const fn extent(self) -> TextExtent {
        TextExtent::new(self.char_len(), self.byte_len())
    }

    pub const fn is_empty(self) -> bool {
        self.byte_range.is_empty()
    }
}

const fn transpose_half_open_position(
    pos: usize,
    start1: usize,
    end1: usize,
    start2: usize,
    end2: usize,
) -> usize {
    if pos < start1 || pos >= end2 {
        pos
    } else if pos < end1 {
        pos + (end2 - end1)
    } else if pos < start2 {
        let diff = (end2 - start2) as isize - (end1 - start1) as isize;
        (pos as isize + diff) as usize
    } else {
        pos - (start2 - start1)
    }
}

#[cfg(test)]
#[path = "edit/tests/edit_test.rs"]
mod tests;
