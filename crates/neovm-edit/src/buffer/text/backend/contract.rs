use std::fmt;

use crate::buffer::position::{
    CharPos0, EmacsBytePos, EmacsByteRange, TextPositionHint, TextPositionLookup,
};
#[cfg(test)]
use crate::buffer::text::TextBackendDebugLayout;
use crate::buffer::text::{TextEditRange, TextExtent, TextMetrics, TextReplacement};

use super::gap::GapTextBackend;
use super::piece_tree::PieceTreeTextBackend;
use super::rope::RopeTextBackend;

macro_rules! impl_physical_text_backend {
    (
        $backend:ty,
        debug_layout = $debug_layout:expr,
        position_lookup = $position_lookup:expr,
        storage_position_hint = $storage_position_hint:expr
    ) => {
        impl PhysicalTextBackend for $backend {
            fn metrics(&self) -> TextMetrics {
                <$backend>::metrics(self)
            }

            #[cfg(test)]
            fn debug_layout(&self) -> TextBackendDebugLayout {
                ($debug_layout)(self)
            }

            fn position_lookup(&self) -> TextPositionLookup {
                ($position_lookup)(self)
            }

            fn storage_position_hint(&self) -> TextPositionHint {
                ($storage_position_hint)(self)
            }

            fn is_multibyte(&self) -> bool {
                <$backend>::is_multibyte(self)
            }

            fn set_multibyte(&mut self, multibyte: bool) {
                <$backend>::set_multibyte(self, multibyte);
            }

            fn byte_at_emacs_byte_pos(&self, pos: EmacsBytePos) -> u8 {
                <$backend>::byte_at_emacs_byte_pos(self, pos)
            }

            fn emacs_byte_at_pos(&self, pos: EmacsBytePos) -> Option<u8> {
                <$backend>::emacs_byte_at_pos(self, pos)
            }

            fn char_at_emacs_byte_pos(&self, pos: EmacsBytePos) -> Option<char> {
                <$backend>::char_at_emacs_byte_pos(self, pos)
            }

            fn char_code_at_emacs_byte_pos(&self, pos: EmacsBytePos) -> Option<u32> {
                <$backend>::char_code_at_emacs_byte_pos(self, pos)
            }

            fn contiguous_window_at(&self, pos: usize) -> Option<(usize, *const u8, usize)> {
                <$backend>::contiguous_window_at(self, pos)
            }

            fn emacs_byte_pos_to_char_pos(&self, byte_pos: EmacsBytePos) -> CharPos0 {
                <$backend>::emacs_byte_pos_to_char_pos(self, byte_pos)
            }

            fn char_pos_to_emacs_byte_pos(&self, char_pos: CharPos0) -> EmacsBytePos {
                <$backend>::char_pos_to_emacs_byte_pos(self, char_pos)
            }

            fn text_emacs_byte_range(&self, range: EmacsByteRange) -> String {
                <$backend>::text_emacs_byte_range(self, range)
            }

            fn copy_emacs_byte_range_to(&self, range: EmacsByteRange, out: &mut Vec<u8>) {
                <$backend>::copy_emacs_byte_range_to(self, range, out);
            }

            fn for_each_emacs_byte_range_chunk<E, F>(
                &self,
                range: EmacsByteRange,
                f: F,
            ) -> Result<(), E>
            where
                F: FnMut(&[u8]) -> Result<(), E>,
            {
                <$backend>::for_each_emacs_byte_range_chunk(self, range, f)
            }

            fn has_contiguous_emacs_byte_range(&self, range: EmacsByteRange) -> bool {
                <$backend>::has_contiguous_emacs_byte_range(self, range)
            }

            fn with_contiguous_emacs_byte_range<R, F>(
                &self,
                range: EmacsByteRange,
                f: F,
            ) -> Option<R>
            where
                F: FnOnce(&[u8]) -> R,
            {
                <$backend>::with_contiguous_emacs_byte_range(self, range, f)
            }

            fn insert_measured_emacs_bytes(
                &mut self,
                pos: EmacsBytePos,
                bytes: &[u8],
                extent: TextExtent,
            ) {
                <$backend>::insert_measured_emacs_bytes(self, pos, bytes, extent);
            }

            fn delete_measured_range(&mut self, range: TextEditRange) {
                <$backend>::delete_measured_range(self, range);
            }

            fn replace_measured_range(&mut self, replacement: TextReplacement, bytes: &[u8]) {
                <$backend>::replace_measured_range(self, replacement, bytes);
            }

            fn replace_same_len_measured_range(
                &mut self,
                replacement: TextReplacement,
                bytes: &[u8],
            ) {
                <$backend>::replace_same_len_measured_range(self, replacement, bytes);
            }

            fn dump_text(&self) -> Vec<u8> {
                <$backend>::dump_text(self)
            }
        }
    };
}

pub(super) trait PhysicalTextBackend: fmt::Display {
    fn metrics(&self) -> TextMetrics;

    #[cfg(test)]
    fn debug_layout(&self) -> TextBackendDebugLayout;

    fn position_lookup(&self) -> TextPositionLookup;
    fn storage_position_hint(&self) -> TextPositionHint;
    fn is_multibyte(&self) -> bool;
    fn set_multibyte(&mut self, multibyte: bool);
    fn byte_at_emacs_byte_pos(&self, pos: EmacsBytePos) -> u8;
    fn emacs_byte_at_pos(&self, pos: EmacsBytePos) -> Option<u8>;
    fn char_at_emacs_byte_pos(&self, pos: EmacsBytePos) -> Option<char>;
    fn char_code_at_emacs_byte_pos(&self, pos: EmacsBytePos) -> Option<u32>;

    /// The contiguous physical window containing logical byte `pos`, as
    /// `(logical_start, base_ptr, len)`. Backends without cheap contiguous
    /// windows return `None`; callers must fall back to the per-byte
    /// accessors. The pointer is valid only until the next text mutation —
    /// see `GapBuffer::contiguous_window_at`.
    fn contiguous_window_at(&self, pos: usize) -> Option<(usize, *const u8, usize)>;
    fn emacs_byte_pos_to_char_pos(&self, byte_pos: EmacsBytePos) -> CharPos0;
    fn char_pos_to_emacs_byte_pos(&self, char_pos: CharPos0) -> EmacsBytePos;
    fn text_emacs_byte_range(&self, range: EmacsByteRange) -> String;
    fn copy_emacs_byte_range_to(&self, range: EmacsByteRange, out: &mut Vec<u8>);
    fn for_each_emacs_byte_range_chunk<E, F>(&self, range: EmacsByteRange, f: F) -> Result<(), E>
    where
        F: FnMut(&[u8]) -> Result<(), E>;
    fn has_contiguous_emacs_byte_range(&self, range: EmacsByteRange) -> bool;
    fn with_contiguous_emacs_byte_range<R, F>(&self, range: EmacsByteRange, f: F) -> Option<R>
    where
        F: FnOnce(&[u8]) -> R;
    fn insert_measured_emacs_bytes(&mut self, pos: EmacsBytePos, bytes: &[u8], extent: TextExtent);
    fn delete_measured_range(&mut self, range: TextEditRange);
    fn replace_measured_range(&mut self, replacement: TextReplacement, bytes: &[u8]);
    fn replace_same_len_measured_range(&mut self, replacement: TextReplacement, bytes: &[u8]);
    fn dump_text(&self) -> Vec<u8>;
}

impl_physical_text_backend!(
    GapTextBackend,
    debug_layout = |backend: &GapTextBackend| TextBackendDebugLayout::Gap(
        GapTextBackend::debug_layout(backend)
    ),
    position_lookup = |_backend: &GapTextBackend| TextPositionLookup::AnchorScan,
    storage_position_hint =
        |backend: &GapTextBackend| GapTextBackend::storage_position_hint(backend)
);

impl_physical_text_backend!(
    PieceTreeTextBackend,
    debug_layout = |backend: &PieceTreeTextBackend| PieceTreeTextBackend::debug_layout(backend),
    position_lookup = |_backend: &PieceTreeTextBackend| TextPositionLookup::BackendIndex,
    storage_position_hint = |_backend: &PieceTreeTextBackend| TextPositionHint::none()
);

impl_physical_text_backend!(
    RopeTextBackend,
    debug_layout = |backend: &RopeTextBackend| RopeTextBackend::debug_layout(backend),
    position_lookup = |_backend: &RopeTextBackend| TextPositionLookup::BackendIndex,
    storage_position_hint = |_backend: &RopeTextBackend| TextPositionHint::none()
);
