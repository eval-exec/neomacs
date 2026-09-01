#[cfg(test)]
mod conformance;
mod contract;
mod gap;
mod piece_tree;
mod rope;
mod treap;

use std::fmt;

use super::ImplementedBufferTextBackendKind;
use crate::buffer::position::{
    CharPos0, EmacsBytePos, EmacsByteRange, TextPositionHint, TextPositionLookup,
};
#[cfg(test)]
use crate::buffer::text::TextBackendDebugLayout;
use crate::buffer::text::{
    BufferTextBytesSnapshot, GapCompatState, TextEditRange, TextExtent, TextMetrics,
    TextReplacement,
};
use contract::PhysicalTextBackend;
use gap::GapTextBackend;
use piece_tree::PieceTreeTextBackend;
use rope::RopeTextBackend;

macro_rules! dispatch_backend_ref {
    ($backend:expr, $storage:ident => $body:expr) => {
        match $backend {
            TextBackend::Gap($storage) => $body,
            TextBackend::PieceTree($storage) => $body,
            TextBackend::Rope($storage) => $body,
        }
    };
}

macro_rules! dispatch_backend_mut {
    ($backend:expr, $storage:ident => $body:expr) => {
        match $backend {
            TextBackend::Gap($storage) => $body,
            TextBackend::PieceTree($storage) => $body,
            TextBackend::Rope($storage) => $body,
        }
    };
}

/// Physical storage for buffer text.
///
/// This enum is intentionally private to the buffer module.  `BufferText`
/// owns GNU-visible text semantics such as markers, text properties, ticks,
/// narrowing interactions, and cache invalidation.  Concrete backends own only
/// byte storage and backend-local lookup hints.
#[derive(Clone)]
pub(in crate::buffer) enum TextBackend {
    Gap(GapTextBackend),
    PieceTree(PieceTreeTextBackend),
    Rope(RopeTextBackend),
}

impl TextBackend {
    pub(in crate::buffer) fn new(kind: ImplementedBufferTextBackendKind) -> Self {
        match kind {
            ImplementedBufferTextBackendKind::GAP_BUFFER => Self::Gap(GapTextBackend::new()),
            ImplementedBufferTextBackendKind::PIECE_TREE => {
                Self::PieceTree(PieceTreeTextBackend::new())
            }
            ImplementedBufferTextBackendKind::ROPE => Self::Rope(RopeTextBackend::new()),
        }
    }

    pub(in crate::buffer) fn from_str(text: &str, kind: ImplementedBufferTextBackendKind) -> Self {
        match kind {
            ImplementedBufferTextBackendKind::GAP_BUFFER => {
                Self::Gap(GapTextBackend::from_str(text))
            }
            ImplementedBufferTextBackendKind::PIECE_TREE => {
                Self::PieceTree(PieceTreeTextBackend::from_str(text))
            }
            ImplementedBufferTextBackendKind::ROPE => Self::Rope(RopeTextBackend::from_str(text)),
        }
    }

    pub(in crate::buffer) fn from_emacs_bytes(
        bytes: &[u8],
        multibyte: bool,
        kind: ImplementedBufferTextBackendKind,
    ) -> Self {
        match kind {
            ImplementedBufferTextBackendKind::GAP_BUFFER => {
                Self::Gap(GapTextBackend::from_emacs_bytes(bytes, multibyte))
            }
            ImplementedBufferTextBackendKind::PIECE_TREE => {
                Self::PieceTree(PieceTreeTextBackend::from_emacs_bytes(bytes, multibyte))
            }
            ImplementedBufferTextBackendKind::ROPE => {
                Self::Rope(RopeTextBackend::from_emacs_bytes(bytes, multibyte))
            }
        }
    }

    pub(in crate::buffer) fn from_snapshot(
        snapshot: BufferTextBytesSnapshot,
        kind: ImplementedBufferTextBackendKind,
    ) -> Self {
        match kind {
            ImplementedBufferTextBackendKind::GAP_BUFFER => {
                Self::Gap(GapTextBackend::from_snapshot(snapshot))
            }
            ImplementedBufferTextBackendKind::PIECE_TREE => {
                Self::PieceTree(PieceTreeTextBackend::from_snapshot(snapshot))
            }
            ImplementedBufferTextBackendKind::ROPE => {
                Self::Rope(RopeTextBackend::from_snapshot(snapshot))
            }
        }
    }

    pub(in crate::buffer) fn from_snapshot_with_gap_compat_state(
        snapshot: BufferTextBytesSnapshot,
        kind: ImplementedBufferTextBackendKind,
        gap_state: GapCompatState,
    ) -> Self {
        match kind {
            ImplementedBufferTextBackendKind::GAP_BUFFER => Self::Gap(
                GapTextBackend::from_snapshot_with_gap_compat_state(snapshot, gap_state),
            ),
            ImplementedBufferTextBackendKind::PIECE_TREE => {
                Self::PieceTree(PieceTreeTextBackend::from_snapshot(snapshot))
            }
            ImplementedBufferTextBackendKind::ROPE => {
                Self::Rope(RopeTextBackend::from_snapshot(snapshot))
            }
        }
    }

    pub(in crate::buffer) fn kind(&self) -> ImplementedBufferTextBackendKind {
        match self {
            Self::Gap(_) => ImplementedBufferTextBackendKind::GAP_BUFFER,
            Self::PieceTree(_) => ImplementedBufferTextBackendKind::PIECE_TREE,
            Self::Rope(_) => ImplementedBufferTextBackendKind::ROPE,
        }
    }

    #[cfg(test)]
    pub(in crate::buffer) fn debug_layout(&self) -> TextBackendDebugLayout {
        dispatch_backend_ref!(self, storage => PhysicalTextBackend::debug_layout(storage))
    }

    pub(in crate::buffer) fn metrics(&self) -> TextMetrics {
        dispatch_backend_ref!(self, storage => PhysicalTextBackend::metrics(storage))
    }

    pub(in crate::buffer) fn storage_position_hint(&self) -> TextPositionHint {
        dispatch_backend_ref!(self, storage => PhysicalTextBackend::storage_position_hint(storage))
    }

    pub(in crate::buffer) fn position_lookup(&self) -> TextPositionLookup {
        dispatch_backend_ref!(self, storage => PhysicalTextBackend::position_lookup(storage))
    }

    pub(in crate::buffer) fn real_gap_compat_state(&self) -> Option<GapCompatState> {
        match self {
            Self::Gap(storage) => Some(storage.real_gap_compat_state()),
            Self::PieceTree(_) | Self::Rope(_) => None,
        }
    }

    pub(in crate::buffer) fn is_multibyte(&self) -> bool {
        dispatch_backend_ref!(self, storage => PhysicalTextBackend::is_multibyte(storage))
    }

    pub(in crate::buffer) fn set_multibyte(&mut self, multibyte: bool) {
        dispatch_backend_mut!(self, storage => PhysicalTextBackend::set_multibyte(storage, multibyte));
    }

    pub(in crate::buffer) fn byte_at_emacs_byte_pos(&self, pos: EmacsBytePos) -> u8 {
        dispatch_backend_ref!(self, storage => PhysicalTextBackend::byte_at_emacs_byte_pos(storage, pos))
    }

    pub(in crate::buffer) fn emacs_byte_at_pos(&self, pos: EmacsBytePos) -> Option<u8> {
        dispatch_backend_ref!(self, storage => PhysicalTextBackend::emacs_byte_at_pos(storage, pos))
    }

    pub(in crate::buffer) fn char_at_emacs_byte_pos(&self, pos: EmacsBytePos) -> Option<char> {
        dispatch_backend_ref!(self, storage => PhysicalTextBackend::char_at_emacs_byte_pos(storage, pos))
    }

    pub(in crate::buffer) fn char_code_at_emacs_byte_pos(&self, pos: EmacsBytePos) -> Option<u32> {
        dispatch_backend_ref!(self, storage => PhysicalTextBackend::char_code_at_emacs_byte_pos(storage, pos))
    }

    pub(in crate::buffer) fn contiguous_window_at(
        &self,
        pos: usize,
    ) -> Option<(usize, *const u8, usize)> {
        dispatch_backend_ref!(self, storage => PhysicalTextBackend::contiguous_window_at(storage, pos))
    }

    pub(in crate::buffer) fn emacs_byte_pos_to_char_pos(&self, byte_pos: EmacsBytePos) -> CharPos0 {
        dispatch_backend_ref!(self, storage => PhysicalTextBackend::emacs_byte_pos_to_char_pos(storage, byte_pos))
    }

    pub(in crate::buffer) fn char_pos_to_emacs_byte_pos(&self, char_pos: CharPos0) -> EmacsBytePos {
        dispatch_backend_ref!(self, storage => PhysicalTextBackend::char_pos_to_emacs_byte_pos(storage, char_pos))
    }

    pub(in crate::buffer) fn text_emacs_byte_range(&self, range: EmacsByteRange) -> String {
        dispatch_backend_ref!(self, storage => PhysicalTextBackend::text_emacs_byte_range(storage, range))
    }

    pub(in crate::buffer) fn copy_emacs_byte_range_to(
        &self,
        range: EmacsByteRange,
        out: &mut Vec<u8>,
    ) {
        dispatch_backend_ref!(self, storage => PhysicalTextBackend::copy_emacs_byte_range_to(storage, range, out));
    }

    pub(in crate::buffer) fn for_each_emacs_byte_range_chunk<E>(
        &self,
        range: EmacsByteRange,
        f: impl FnMut(&[u8]) -> Result<(), E>,
    ) -> Result<(), E> {
        dispatch_backend_ref!(self, storage => PhysicalTextBackend::for_each_emacs_byte_range_chunk(storage, range, f))
    }

    pub(in crate::buffer) fn has_contiguous_emacs_byte_range(&self, range: EmacsByteRange) -> bool {
        dispatch_backend_ref!(self, storage => PhysicalTextBackend::has_contiguous_emacs_byte_range(storage, range))
    }

    /// Best-effort: arrange for `range` to be borrowable as a single
    /// contiguous slice, returning whether it now is.  The gap backend
    /// moves its gap out of the range (GNU `move_gap`); chunked backends
    /// (piece tree, rope) cannot and just report their existing
    /// contiguity.  Content and logical positions are unaffected.
    pub(in crate::buffer) fn try_make_emacs_byte_range_contiguous(
        &mut self,
        range: EmacsByteRange,
    ) -> bool {
        match self {
            TextBackend::Gap(gap) => {
                gap.make_emacs_byte_range_contiguous(range);
                true
            }
            _ => self.has_contiguous_emacs_byte_range(range),
        }
    }

    pub(in crate::buffer) fn with_contiguous_emacs_byte_range<R>(
        &self,
        range: EmacsByteRange,
        f: impl FnOnce(&[u8]) -> R,
    ) -> Option<R> {
        dispatch_backend_ref!(self, storage => PhysicalTextBackend::with_contiguous_emacs_byte_range(storage, range, f))
    }

    pub(in crate::buffer) fn insert_measured_emacs_bytes(
        &mut self,
        pos: EmacsBytePos,
        bytes: &[u8],
        extent: TextExtent,
    ) {
        dispatch_backend_mut!(self, storage => PhysicalTextBackend::insert_measured_emacs_bytes(storage, pos, bytes, extent));
    }

    pub(in crate::buffer) fn delete_measured_range(&mut self, range: TextEditRange) {
        dispatch_backend_mut!(self, storage => PhysicalTextBackend::delete_measured_range(storage, range));
    }

    pub(in crate::buffer) fn replace_measured_range(
        &mut self,
        replacement: TextReplacement,
        bytes: &[u8],
    ) {
        dispatch_backend_mut!(self, storage => PhysicalTextBackend::replace_measured_range(storage, replacement, bytes));
    }

    pub(in crate::buffer) fn replace_same_len_measured_range(
        &mut self,
        replacement: TextReplacement,
        bytes: &[u8],
    ) {
        dispatch_backend_mut!(self, storage => PhysicalTextBackend::replace_same_len_measured_range(storage, replacement, bytes));
    }

    pub(in crate::buffer) fn dump_text(&self) -> Vec<u8> {
        dispatch_backend_ref!(self, storage => PhysicalTextBackend::dump_text(storage))
    }

    pub(in crate::buffer) fn snapshot(&self) -> BufferTextBytesSnapshot {
        BufferTextBytesSnapshot::new(self.dump_text(), self.is_multibyte())
    }
}

impl fmt::Display for TextBackend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        dispatch_backend_ref!(self, storage => storage.fmt(f))
    }
}
