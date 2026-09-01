use std::fmt;

use crate::buffer::gap_buffer::GapBuffer;
use crate::buffer::position::{
    CharLen, CharPos0, EmacsByteLen, EmacsBytePos, EmacsByteRange, TextPositionAnchor,
    TextPositionHint,
};
#[cfg(test)]
use crate::buffer::text::GapDebugLayout;
use crate::buffer::text::{
    BufferTextBytesSnapshot, GapCompatState, TextEditRange, TextExtent, TextMetrics,
    TextReplacement,
};

#[derive(Clone)]
pub(in crate::buffer) struct GapTextBackend {
    gap: GapBuffer,
}

impl GapTextBackend {
    pub(in crate::buffer) fn new() -> Self {
        Self {
            gap: GapBuffer::new(),
        }
    }

    pub(in crate::buffer) fn from_str(text: &str) -> Self {
        Self {
            gap: GapBuffer::from_str(text),
        }
    }

    pub(in crate::buffer) fn from_emacs_bytes(bytes: &[u8], multibyte: bool) -> Self {
        Self {
            gap: GapBuffer::from_emacs_bytes(bytes, multibyte),
        }
    }

    pub(in crate::buffer) fn from_snapshot(snapshot: BufferTextBytesSnapshot) -> Self {
        let (text, multibyte) = snapshot.into_parts();
        Self {
            gap: GapBuffer::from_dump(text, multibyte),
        }
    }

    pub(in crate::buffer) fn from_snapshot_with_gap_compat_state(
        snapshot: BufferTextBytesSnapshot,
        gap_state: GapCompatState,
    ) -> Self {
        let (text, multibyte) = snapshot.into_parts();
        Self {
            gap: GapBuffer::from_emacs_bytes_with_gap_compat_state(&text, multibyte, gap_state),
        }
    }

    fn len(&self) -> usize {
        self.gap.len()
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(in crate::buffer) fn is_empty(&self) -> bool {
        self.gap.is_empty()
    }

    pub(in crate::buffer) fn is_multibyte(&self) -> bool {
        self.gap.is_multibyte()
    }

    pub(in crate::buffer) fn set_multibyte(&mut self, multibyte: bool) {
        self.gap.set_multibyte(multibyte);
    }

    fn char_count(&self) -> usize {
        self.gap.char_count()
    }

    fn gpt(&self) -> usize {
        self.gap.gpt()
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn z(&self) -> usize {
        self.gap.z()
    }

    fn gpt_byte(&self) -> usize {
        self.gap.gpt_byte()
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn z_byte(&self) -> usize {
        self.gap.z_byte()
    }

    fn gap_size(&self) -> usize {
        self.gap.gap_size()
    }

    #[cfg(test)]
    pub(in crate::buffer) fn debug_layout(&self) -> GapDebugLayout {
        GapDebugLayout {
            gpt: CharPos0::new(self.gpt()),
            z: CharPos0::new(self.z()),
            gpt_byte: EmacsBytePos::new(self.gpt_byte()),
            z_byte: EmacsBytePos::new(self.z_byte()),
            gap_byte_len: EmacsByteLen::new(self.gap_size()),
        }
    }

    pub(in crate::buffer) fn metrics(&self) -> TextMetrics {
        TextMetrics::from_lengths(
            CharLen::new(self.char_count()),
            EmacsByteLen::new(self.len()),
        )
    }

    pub(in crate::buffer) fn storage_position_hint(&self) -> TextPositionHint {
        TextPositionHint::from_anchor(TextPositionAnchor::new(
            CharPos0::new(self.gpt()),
            EmacsBytePos::new(self.gpt_byte()),
        ))
    }

    pub(in crate::buffer) fn real_gap_compat_state(&self) -> GapCompatState {
        GapCompatState::new(
            CharPos0::new(self.gpt()),
            EmacsByteLen::new(self.gap_size()),
        )
    }

    pub(in crate::buffer) fn byte_at_emacs_byte_pos(&self, pos: EmacsBytePos) -> u8 {
        self.gap.byte_at_emacs_byte_pos(pos)
    }

    pub(in crate::buffer) fn emacs_byte_at_pos(&self, pos: EmacsBytePos) -> Option<u8> {
        self.gap.emacs_byte_at_pos(pos)
    }

    pub(in crate::buffer) fn char_at_emacs_byte_pos(&self, pos: EmacsBytePos) -> Option<char> {
        self.gap.char_at_emacs_byte_pos(pos)
    }

    pub(in crate::buffer) fn char_code_at_emacs_byte_pos(&self, pos: EmacsBytePos) -> Option<u32> {
        self.gap.char_code_at_emacs_byte_pos(pos)
    }

    pub(in crate::buffer) fn contiguous_window_at(
        &self,
        pos: usize,
    ) -> Option<(usize, *const u8, usize)> {
        self.gap.contiguous_window_at(pos)
    }

    pub(in crate::buffer) fn emacs_byte_pos_to_char_pos(&self, byte_pos: EmacsBytePos) -> CharPos0 {
        self.gap.emacs_byte_pos_to_char_pos(byte_pos)
    }

    pub(in crate::buffer) fn char_pos_to_emacs_byte_pos(&self, char_pos: CharPos0) -> EmacsBytePos {
        self.gap.char_pos_to_emacs_byte_pos(char_pos)
    }

    pub(in crate::buffer) fn text_emacs_byte_range(&self, range: EmacsByteRange) -> String {
        self.gap.text_emacs_byte_range(range)
    }

    pub(in crate::buffer) fn copy_emacs_byte_range_to(
        &self,
        range: EmacsByteRange,
        out: &mut Vec<u8>,
    ) {
        self.gap.copy_emacs_byte_range_to(range, out);
    }

    pub(in crate::buffer) fn for_each_emacs_byte_range_chunk<E>(
        &self,
        range: EmacsByteRange,
        f: impl FnMut(&[u8]) -> Result<(), E>,
    ) -> Result<(), E> {
        self.gap.for_each_emacs_byte_range_chunk(range, f)
    }

    pub(in crate::buffer) fn has_contiguous_emacs_byte_range(&self, range: EmacsByteRange) -> bool {
        self.gap.has_contiguous_emacs_byte_range(range)
    }

    pub(in crate::buffer) fn make_emacs_byte_range_contiguous(&mut self, range: EmacsByteRange) {
        self.gap.make_emacs_byte_range_contiguous(range);
    }

    pub(in crate::buffer) fn with_contiguous_emacs_byte_range<R>(
        &self,
        range: EmacsByteRange,
        f: impl FnOnce(&[u8]) -> R,
    ) -> Option<R> {
        self.gap.with_contiguous_emacs_byte_range(range, f)
    }

    pub(in crate::buffer) fn insert_measured_emacs_bytes(
        &mut self,
        pos: EmacsBytePos,
        bytes: &[u8],
        extent: TextExtent,
    ) {
        self.gap.insert_measured_emacs_bytes(pos, bytes, extent);
    }

    pub(in crate::buffer) fn delete_measured_range(&mut self, range: TextEditRange) {
        self.gap.delete_measured_range(range);
    }

    pub(in crate::buffer) fn replace_measured_range(
        &mut self,
        replacement: TextReplacement,
        bytes: &[u8],
    ) {
        self.gap.replace_measured_range(replacement, bytes);
    }

    pub(in crate::buffer) fn replace_same_len_measured_range(
        &mut self,
        replacement: TextReplacement,
        bytes: &[u8],
    ) {
        self.gap.replace_same_len_measured_range(replacement, bytes);
    }

    pub(in crate::buffer) fn dump_text(&self) -> Vec<u8> {
        self.gap.dump_text()
    }
}

impl fmt::Display for GapTextBackend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.gap.fmt(f)
    }
}

impl fmt::Debug for GapTextBackend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GapTextBackend")
            .field("len", &self.len())
            .field("chars", &self.char_count())
            .finish()
    }
}
