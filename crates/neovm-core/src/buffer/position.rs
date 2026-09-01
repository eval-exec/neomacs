//! Position types for distinguishing buffer coordinate spaces.
//!
//! GNU Emacs uses 1-based positions everywhere (BEG=1, first char is at
//! position 1).  NeoMacs stores positions 0-based internally but exposes
//! them 1-based to Lisp.  These wrappers make the distinction type-safe
//! at the boundary so the compiler catches accidental mixing.

/// 0-based internal character position (first character = 0).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct CharPos0(usize);

/// 0-based logical Emacs byte position.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct EmacsBytePos(usize);

/// Count of logical Emacs characters.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct CharLen(usize);

/// Count of logical Emacs bytes.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct EmacsByteLen(usize);

/// Signed change in logical Emacs characters.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub(in crate::buffer) struct CharDelta(isize);

/// Signed change in logical Emacs bytes.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub(in crate::buffer) struct EmacsByteDelta(isize);

/// Half-open internal character range `[start, end)`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CharRange {
    start: CharPos0,
    end: CharPos0,
}

/// 1-based Lisp character position (first character = 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct LispCharPos1(i64);

/// Inclusive Lisp character-position range `[BEG, Z]` for a full buffer.
///
/// Unlike [`AccessibleCharRange`], this range deliberately ignores narrowing.
/// It includes `Z`, because Lisp positions may name the insertion boundary
/// immediately after the final character.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FullBufferLispCharRange {
    z: LispCharPos1,
}

/// 1-based Lisp byte position (first byte = 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct LispBytePos1(i64);

/// Display column coordinate. This is not a buffer character position.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct DisplayColumn(usize);

/// Half-open logical Emacs byte range `[start, end)`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EmacsByteRange {
    start: EmacsBytePos,
    end: EmacsBytePos,
}

/// Accessible logical Emacs byte range `[BEGV_BYTE, ZV_BYTE)`.
///
/// This is still an Emacs byte range, but carrying the narrowing meaning in
/// the type keeps higher-level motion/search code from reaching directly into
/// raw buffer fields for `begv_byte` and `zv_byte`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AccessibleEmacsByteRange {
    range: EmacsByteRange,
}

/// Accessible internal character range `[BEGV, ZV)`.
///
/// GNU syntax and parse code carries both character and byte positions. This
/// companion to `AccessibleEmacsByteRange` keeps those character bounds
/// explicit at call sites that must not scan outside the narrowed region.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AccessibleCharRange {
    range: CharRange,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextPositionAnchor {
    char_pos: CharPos0,
    emacs_byte_pos: EmacsBytePos,
}

/// Bracketing anchors used for char<->byte conversion.
///
/// GNU Emacs' `marker.c` conversion code keeps the nearest known `(char,
/// byte)` pair below and above the target, considering point, gap, narrowing,
/// the last conversion, and markers.  This type keeps that paired state
/// explicit so callers cannot update one coordinate without the other.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextPositionBounds {
    below: TextPositionAnchor,
    above: TextPositionAnchor,
}

/// Backend-local known position that can help char<->byte conversion.
///
/// This is a storage hint, not editor state.  GNU's gap position is one known
/// `(char, byte)` pair among many; keeping it behind this wrapper avoids
/// exposing gap-shaped APIs to the generic conversion logic.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct TextPositionHint {
    anchor: Option<TextPositionAnchor>,
}

/// How a physical backend wants generic text code to translate positions.
///
/// GNU's gap buffer benefits from the marker.c-style anchor scan because the
/// gap and marker chain provide nearby `(char, byte)` pairs. Indexed storage
/// such as piece trees and ropes should answer from their own subtree metrics
/// instead of forcing the semantic layer to scan chunks linearly.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum TextPositionLookup {
    #[default]
    AnchorScan,
    BackendIndex,
}

impl TextPositionAnchor {
    pub const ZERO: Self = Self {
        char_pos: CharPos0::ZERO,
        emacs_byte_pos: EmacsBytePos::ZERO,
    };

    pub const fn new(char_pos: CharPos0, emacs_byte_pos: EmacsBytePos) -> Self {
        Self {
            char_pos,
            emacs_byte_pos,
        }
    }

    #[cfg(test)]
    pub(in crate::buffer) const fn from_usize(char_pos: usize, emacs_byte_pos: usize) -> Self {
        Self {
            char_pos: CharPos0::new(char_pos),
            emacs_byte_pos: EmacsBytePos::new(emacs_byte_pos),
        }
    }

    pub const fn char_pos(self) -> CharPos0 {
        self.char_pos
    }

    pub const fn emacs_byte_pos(self) -> EmacsBytePos {
        self.emacs_byte_pos
    }

    pub(in crate::buffer) const fn char_pos_usize(self) -> usize {
        self.char_pos.get()
    }

    pub(in crate::buffer) const fn emacs_byte_pos_usize(self) -> usize {
        self.emacs_byte_pos.get()
    }
}

impl TextPositionHint {
    pub const fn none() -> Self {
        Self { anchor: None }
    }

    pub const fn from_anchor(anchor: TextPositionAnchor) -> Self {
        Self {
            anchor: Some(anchor),
        }
    }

    pub fn consider_char_anchor(self, bounds: &mut TextPositionBounds, target: CharPos0) {
        if let Some(anchor) = self.anchor {
            bounds.consider_char_anchor(target, anchor);
        }
    }

    pub fn consider_byte_anchor(self, bounds: &mut TextPositionBounds, target: EmacsBytePos) {
        if let Some(anchor) = self.anchor {
            bounds.consider_byte_anchor(target, anchor);
        }
    }
}

impl TextPositionLookup {
    pub const fn uses_backend_index(self) -> bool {
        matches!(self, Self::BackendIndex)
    }
}

impl TextPositionBounds {
    pub const fn new(above: TextPositionAnchor) -> Self {
        Self {
            below: TextPositionAnchor::new(CharPos0::ZERO, EmacsBytePos::ZERO),
            above,
        }
    }

    pub const fn below(self) -> TextPositionAnchor {
        self.below
    }

    pub const fn above(self) -> TextPositionAnchor {
        self.above
    }

    pub fn consider_char_anchor(&mut self, target: CharPos0, anchor: TextPositionAnchor) {
        if anchor.char_pos() <= target && anchor.char_pos() > self.below.char_pos() {
            self.below = anchor;
        }
        if anchor.char_pos() >= target && anchor.char_pos() < self.above.char_pos() {
            self.above = anchor;
        }
    }

    pub fn consider_byte_anchor(&mut self, target: EmacsBytePos, anchor: TextPositionAnchor) {
        if anchor.emacs_byte_pos() <= target
            && anchor.emacs_byte_pos() > self.below.emacs_byte_pos()
        {
            self.below = anchor;
        }
        if anchor.emacs_byte_pos() >= target
            && anchor.emacs_byte_pos() < self.above.emacs_byte_pos()
        {
            self.above = anchor;
        }
    }

    /// Whether every character between the two bracketing anchors occupies a
    /// single byte.
    ///
    /// GNU's `CONSIDER` (marker.c) checks exactly this each time the brackets
    /// narrow: "If at any point we can tell that the space between those two
    /// best approximations is all single-byte, we interpolate the result
    /// immediately." Equal char and byte spans can only happen when every
    /// character in between is one byte, so a position inside the span
    /// converts by ARITHMETIC and needs no scan.
    ///
    /// This matters far beyond the whole-buffer `SCHARS == SBYTES` fast path.
    /// Real Emacs sources are multibyte for the sake of a handful of characters
    /// -- curly quotes in docstrings -- so the global check fails while nearly
    /// every local span is still pure ASCII.
    const fn span_is_single_byte(self) -> bool {
        let chars = self.above.char_pos().get() - self.below.char_pos().get();
        let bytes = self.above.emacs_byte_pos().get() - self.below.emacs_byte_pos().get();
        chars == bytes
    }

    /// The byte position of `target` by interpolation, when the bracketing
    /// span is all single-byte (see [`Self::span_is_single_byte`]).
    pub fn interpolate_char(self, target: CharPos0) -> Option<EmacsBytePos> {
        if !self.span_is_single_byte() {
            return None;
        }
        let offset = target.get() - self.below.char_pos().get();
        Some(EmacsBytePos::new(
            self.below.emacs_byte_pos().get() + offset,
        ))
    }

    /// The char position of `target` by interpolation, when the bracketing
    /// span is all single-byte (see [`Self::span_is_single_byte`]).
    pub fn interpolate_byte(self, target: EmacsBytePos) -> Option<CharPos0> {
        if !self.span_is_single_byte() {
            return None;
        }
        let offset = target.get() - self.below.emacs_byte_pos().get();
        Some(CharPos0::new(self.below.char_pos().get() + offset))
    }

    pub fn char_below_distance(self, target: CharPos0) -> CharLen {
        target.saturating_offset_from(self.below.char_pos())
    }

    pub fn char_above_distance(self, target: CharPos0) -> CharLen {
        self.above.char_pos().saturating_offset_from(target)
    }

    pub fn byte_below_distance(self, target: EmacsBytePos) -> EmacsByteLen {
        target.saturating_offset_from(self.below.emacs_byte_pos())
    }

    pub fn byte_above_distance(self, target: EmacsBytePos) -> EmacsByteLen {
        self.above.emacs_byte_pos().saturating_offset_from(target)
    }

    pub fn char_target_is_near(self, target: CharPos0, distance: CharLen) -> bool {
        self.char_above_distance(target) < distance || self.char_below_distance(target) < distance
    }

    pub fn byte_target_is_near(self, target: EmacsBytePos, distance: EmacsByteLen) -> bool {
        self.byte_above_distance(target) < distance || self.byte_below_distance(target) < distance
    }

    pub fn nearest_char_anchor(self, target: CharPos0) -> TextPositionAnchor {
        if self.char_below_distance(target) <= self.char_above_distance(target) {
            self.below
        } else {
            self.above
        }
    }

    pub fn nearest_byte_anchor(self, target: EmacsBytePos) -> TextPositionAnchor {
        if self.byte_below_distance(target) <= self.byte_above_distance(target) {
            self.below
        } else {
            self.above
        }
    }

    pub fn min_char_walk(self, target: CharPos0) -> CharLen {
        self.char_below_distance(target)
            .min(self.char_above_distance(target))
    }

    pub fn min_byte_walk(self, target: EmacsBytePos) -> EmacsByteLen {
        self.byte_below_distance(target)
            .min(self.byte_above_distance(target))
    }
}

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
    pub fn to_lisp(self, text: &super::BufferText) -> LispCharPos1 {
        text.emacs_byte_pos_to_char_pos(self).to_lisp()
    }

    /// Convert to a 1-based Lisp byte position.
    pub const fn to_lisp_byte_pos(self) -> LispBytePos1 {
        LispBytePos1(self.0 as i64 + 1)
    }

    /// Convert from a 1-based Lisp position.
    /// Requires the buffer for char-to-byte conversion.
    pub fn from_lisp(p: LispCharPos1, text: &super::BufferText) -> Self {
        text.char_pos_to_emacs_byte_pos(CharPos0::from_lisp(p))
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

    #[cfg(test)]
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

impl CharDelta {
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(in crate::buffer) const ZERO: Self = Self(0);

    pub(in crate::buffer) fn insertion(len: CharLen) -> Self {
        Self(len.get() as isize)
    }

    pub(in crate::buffer) fn deletion(len: CharLen) -> Self {
        Self(-(len.get() as isize))
    }

    pub(in crate::buffer) fn replacement(old_len: CharLen, new_len: CharLen) -> Self {
        Self(new_len.get() as isize - old_len.get() as isize)
    }

    pub(in crate::buffer) fn apply_to_pos(self, pos: CharPos0) -> CharPos0 {
        CharPos0::new(apply_signed_delta(pos.get(), self.0))
    }
}

impl EmacsByteDelta {
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(in crate::buffer) const ZERO: Self = Self(0);

    pub(in crate::buffer) fn insertion(len: EmacsByteLen) -> Self {
        Self(len.get() as isize)
    }

    pub(in crate::buffer) fn deletion(len: EmacsByteLen) -> Self {
        Self(-(len.get() as isize))
    }

    pub(in crate::buffer) fn replacement(old_len: EmacsByteLen, new_len: EmacsByteLen) -> Self {
        Self(new_len.get() as isize - old_len.get() as isize)
    }

    #[inline(always)]
    pub(in crate::buffer) fn apply_to_pos(self, pos: EmacsBytePos) -> EmacsBytePos {
        if self.is_zero() {
            return pos;
        }
        EmacsBytePos::new(apply_signed_delta(pos.get(), self.0))
    }

    pub(in crate::buffer) const fn is_zero(self) -> bool {
        self.0 == 0
    }

    #[inline(always)]
    pub(in crate::buffer) fn combine(self, other: Self) -> Self {
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

    pub(in crate::buffer) fn apply_to_range(self, range: EmacsByteRange) -> EmacsByteRange {
        EmacsByteRange::new(
            self.apply_to_pos(range.start()),
            self.apply_to_pos(range.end()),
        )
    }
}

fn apply_signed_delta(value: usize, delta: isize) -> usize {
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

impl AccessibleEmacsByteRange {
    pub const fn new(range: EmacsByteRange) -> Self {
        Self { range }
    }

    pub const fn range(self) -> EmacsByteRange {
        self.range
    }

    pub const fn start(self) -> EmacsBytePos {
        self.range.start()
    }

    pub const fn end(self) -> EmacsBytePos {
        self.range.end()
    }

    pub fn contains(self, pos: EmacsBytePos) -> bool {
        self.start() <= pos && pos < self.end()
    }

    pub fn contains_preceding_char_boundary(self, pos: EmacsBytePos) -> bool {
        self.start() < pos && pos <= self.end()
    }

    pub fn clamp(self, pos: EmacsBytePos) -> EmacsBytePos {
        pos.clamp(self.start(), self.end())
    }
}

impl AccessibleCharRange {
    pub const fn new(range: CharRange) -> Self {
        Self { range }
    }

    pub const fn range(self) -> CharRange {
        self.range
    }

    pub const fn start(self) -> CharPos0 {
        self.range.start()
    }

    pub const fn end(self) -> CharPos0 {
        self.range.end()
    }

    pub fn start_lisp(self) -> LispCharPos1 {
        self.range.start_lisp()
    }

    pub fn end_lisp(self) -> LispCharPos1 {
        self.range.end_lisp()
    }

    pub const fn len(self) -> CharLen {
        self.range.len()
    }

    pub const fn is_empty(self) -> bool {
        self.range.is_empty()
    }

    pub fn contains(self, pos: CharPos0) -> bool {
        self.start() <= pos && pos < self.end()
    }

    pub fn contains_boundary(self, pos: CharPos0) -> bool {
        self.start() <= pos && pos <= self.end()
    }

    pub fn clamp(self, pos: CharPos0) -> CharPos0 {
        pos.clamp(self.start(), self.end())
    }
}

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

    /// Convert to 0-based internal byte position.
    pub fn to_byte_pos(self, text: &super::BufferText) -> EmacsBytePos {
        EmacsBytePos::from_lisp(self, text)
    }

    /// The raw i64 value.
    pub fn as_i64(self) -> i64 {
        self.0
    }
}

impl FullBufferLispCharRange {
    pub const fn new(z: LispCharPos1) -> Self {
        Self { z }
    }

    pub const fn beg(self) -> LispCharPos1 {
        LispCharPos1::ONE
    }

    pub const fn z(self) -> LispCharPos1 {
        self.z
    }

    pub fn contains(self, pos: LispCharPos1) -> bool {
        self.beg() <= pos && pos <= self.z()
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

impl DisplayColumn {
    pub const ZERO: Self = Self(0);

    pub const fn new(pos: usize) -> Self {
        Self(pos)
    }

    pub const fn get(self) -> usize {
        self.0
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_position_bounds_keep_char_and_byte_anchor_pairs_together() {
        let mut bounds = TextPositionBounds::new(TextPositionAnchor::from_usize(20, 40));

        bounds.consider_char_anchor(CharPos0::new(10), TextPositionAnchor::from_usize(5, 7));
        bounds.consider_char_anchor(CharPos0::new(10), TextPositionAnchor::from_usize(15, 27));
        bounds.consider_char_anchor(CharPos0::new(10), TextPositionAnchor::from_usize(12, 24));

        assert_eq!(bounds.below(), TextPositionAnchor::from_usize(5, 7));
        assert_eq!(bounds.above(), TextPositionAnchor::from_usize(12, 24));
        assert_eq!(
            bounds.nearest_char_anchor(CharPos0::new(10)),
            TextPositionAnchor::from_usize(12, 24)
        );

        let mut byte_bounds = TextPositionBounds::new(TextPositionAnchor::from_usize(20, 40));
        byte_bounds.consider_byte_anchor(
            EmacsBytePos::new(30),
            TextPositionAnchor::from_usize(11, 29),
        );
        byte_bounds.consider_byte_anchor(
            EmacsBytePos::new(30),
            TextPositionAnchor::from_usize(13, 33),
        );

        assert_eq!(byte_bounds.below(), TextPositionAnchor::from_usize(11, 29));
        assert_eq!(byte_bounds.above(), TextPositionAnchor::from_usize(13, 33));
        assert_eq!(
            byte_bounds.byte_below_distance(EmacsBytePos::new(30)),
            EmacsByteLen::new(1)
        );
        assert_eq!(
            byte_bounds.byte_above_distance(EmacsBytePos::new(30)),
            EmacsByteLen::new(3)
        );
        assert_eq!(
            byte_bounds.nearest_byte_anchor(EmacsBytePos::new(30)),
            TextPositionAnchor::from_usize(11, 29)
        );
    }

    #[test]
    fn text_position_hint_contributes_backend_anchor_without_exposing_storage_shape() {
        let hint = TextPositionHint::from_anchor(TextPositionAnchor::from_usize(8, 12));

        let mut char_bounds = TextPositionBounds::new(TextPositionAnchor::from_usize(20, 40));
        hint.consider_char_anchor(&mut char_bounds, CharPos0::new(10));
        assert_eq!(char_bounds.below(), TextPositionAnchor::from_usize(8, 12));

        let mut byte_bounds = TextPositionBounds::new(TextPositionAnchor::from_usize(20, 40));
        hint.consider_byte_anchor(&mut byte_bounds, EmacsBytePos::new(10));
        assert_eq!(byte_bounds.above(), TextPositionAnchor::from_usize(8, 12));
    }
}

// Transitional aliases for older call sites. New code should use the explicit
// coordinate-space names above.
pub type CharPos = CharPos0;
pub type BytePos = EmacsBytePos;
pub type LispPos = LispCharPos1;
