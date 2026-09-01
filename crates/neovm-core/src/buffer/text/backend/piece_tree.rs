use std::fmt;

use crate::buffer::position::{CharLen, CharPos0, EmacsByteLen, EmacsBytePos, EmacsByteRange};
#[cfg(test)]
use crate::buffer::text::TextBackendDebugLayout;
use crate::buffer::text::{
    BufferTextBytesSnapshot, TextEditRange, TextExtent, TextMetrics, TextReplacement,
    emacs_byte_to_char_in_slice, emacs_char_count_bytes, emacs_char_to_byte_in_slice,
};

use super::treap::{TreapPriority, TreapSerial};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PieceSource {
    Original,
    Add,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
struct SourceBytePos(usize);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
struct PieceByteOffset(usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Piece {
    source: PieceSource,
    start: SourceBytePos,
    extent: TextExtent,
}

impl SourceBytePos {
    const ZERO: Self = Self(0);

    const fn new(pos: usize) -> Self {
        Self(pos)
    }

    const fn get(self) -> usize {
        self.0
    }

    const fn add_emacs_bytes(self, len: EmacsByteLen) -> Self {
        Self(self.0 + len.get())
    }
}

impl PieceByteOffset {
    const fn new(pos: usize) -> Self {
        Self(pos)
    }

    const fn get(self) -> usize {
        self.0
    }
}

impl Piece {
    const fn new(source: PieceSource, start: SourceBytePos, extent: TextExtent) -> Self {
        Self {
            source,
            start,
            extent,
        }
    }

    const fn emacs_byte_len(self) -> EmacsByteLen {
        self.extent.emacs_bytes()
    }

    const fn char_len(self) -> CharLen {
        self.extent.chars()
    }

    const fn len_usize(self) -> usize {
        self.emacs_byte_len().get()
    }

    const fn char_len_usize(self) -> usize {
        self.char_len().get()
    }

    const fn source_start(self) -> SourceBytePos {
        self.start
    }

    const fn source_end(self) -> SourceBytePos {
        self.start.add_emacs_bytes(self.emacs_byte_len())
    }

    const fn is_empty(self) -> bool {
        self.emacs_byte_len().is_empty()
    }

    const fn metrics(self) -> TextMetrics {
        TextMetrics::from_extent(self.extent)
    }

    fn split_at_emacs_byte(
        self,
        local_byte: PieceByteOffset,
        chars_before: CharLen,
    ) -> (Self, Self) {
        let local_byte = local_byte.get();
        debug_assert!(local_byte > 0 && local_byte < self.len_usize());
        (
            Self::new(
                self.source,
                self.start,
                TextExtent::new(chars_before, EmacsByteLen::new(local_byte)),
            ),
            Self::new(
                self.source,
                self.start.add_emacs_bytes(EmacsByteLen::new(local_byte)),
                TextExtent::new(
                    CharLen::new(self.char_len_usize() - chars_before.get()),
                    EmacsByteLen::new(self.len_usize() - local_byte),
                ),
            ),
        )
    }
}

#[derive(Clone)]
struct PieceNode {
    piece: Piece,
    priority: TreapPriority,
    metrics: TextMetrics,
    left: Option<Box<PieceNode>>,
    right: Option<Box<PieceNode>>,
}

impl PieceNode {
    fn new(piece: Piece, priority: TreapPriority) -> Box<Self> {
        Box::new(Self {
            piece,
            priority,
            metrics: piece_metrics(piece),
            left: None,
            right: None,
        })
    }

    fn refresh(&mut self) {
        let left = node_metrics(&self.left);
        let right = node_metrics(&self.right);
        self.metrics = left.add_extent(self.piece.extent).add_metrics(right);
    }
}

#[derive(Clone)]
pub(in crate::buffer) struct PieceTreeTextBackend {
    original: Vec<u8>,
    add: Vec<u8>,
    multibyte: bool,
    root: Option<Box<PieceNode>>,
    next_piece_serial: TreapSerial,
}

impl PieceTreeTextBackend {
    pub(in crate::buffer) fn new() -> Self {
        Self {
            original: Vec::new(),
            add: Vec::new(),
            multibyte: true,
            root: None,
            next_piece_serial: TreapSerial::FIRST,
        }
    }

    pub(in crate::buffer) fn from_str(text: &str) -> Self {
        let decoded = crate::buffer::text::storage_string_to_emacs_buffer_bytes(text);
        Self::from_emacs_bytes(decoded.bytes(), decoded.multibyte())
    }

    pub(in crate::buffer) fn from_emacs_bytes(bytes: &[u8], multibyte: bool) -> Self {
        let mut backend = Self {
            original: bytes.to_vec(),
            add: Vec::new(),
            multibyte,
            root: None,
            next_piece_serial: TreapSerial::FIRST,
        };
        backend.root = backend.node_for_piece(Piece::new(
            PieceSource::Original,
            SourceBytePos::ZERO,
            TextExtent::from_emacs_bytes(bytes, multibyte),
        ));
        backend
    }

    pub(in crate::buffer) fn from_snapshot(snapshot: BufferTextBytesSnapshot) -> Self {
        Self::from_emacs_bytes(snapshot.bytes(), snapshot.is_multibyte())
    }

    #[cfg(test)]
    pub(in crate::buffer) fn debug_layout(&self) -> TextBackendDebugLayout {
        TextBackendDebugLayout::PieceTree(self.metrics())
    }

    fn len(&self) -> usize {
        self.metrics().emacs_bytes_usize()
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(in crate::buffer) fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub(in crate::buffer) fn is_multibyte(&self) -> bool {
        self.multibyte
    }

    pub(in crate::buffer) fn set_multibyte(&mut self, multibyte: bool) {
        if self.multibyte == multibyte {
            return;
        }
        let bytes = self.dump_text();
        self.rebuild_from_bytes(bytes, multibyte);
    }

    pub(in crate::buffer) fn byte_at_emacs_byte_pos(&self, pos: EmacsBytePos) -> u8 {
        let pos = pos.get();
        assert!(
            pos < self.len(),
            "byte_at: position {pos} out of range (len {})",
            self.len()
        );
        self.contiguous_slice(pos, pos + 1).expect("single byte")[0]
    }

    pub(in crate::buffer) fn emacs_byte_at_pos(&self, pos: EmacsBytePos) -> Option<u8> {
        (pos.get() < self.len()).then(|| self.byte_at_emacs_byte_pos(pos))
    }

    pub(in crate::buffer) fn char_at_emacs_byte_pos(&self, pos: EmacsBytePos) -> Option<char> {
        self.char_code_at_emacs_byte_pos(pos)
            .and_then(char::from_u32)
    }

    pub(in crate::buffer) fn char_code_at_emacs_byte_pos(&self, pos: EmacsBytePos) -> Option<u32> {
        let pos_usize = pos.get();
        if pos_usize >= self.len() {
            return None;
        }
        self.emacs_byte_pos_to_char_pos(pos);
        if !self.multibyte {
            return Some(self.byte_at_emacs_byte_pos(pos) as u32);
        }

        let mut tmp = [0u8; crate::emacs_core::emacs_char::MAX_MULTIBYTE_LENGTH];
        let available = (self.len() - pos_usize).min(tmp.len());
        let mut written = 0;
        self.for_each_emacs_byte_range_chunk(
            EmacsByteRange::from_start_len(pos, EmacsByteLen::new(available)),
            |chunk| {
                let take = (available - written).min(chunk.len());
                tmp[written..written + take].copy_from_slice(&chunk[..take]);
                written += take;
                Ok::<(), ()>(())
            },
        )
        .expect("infallible chunk copy");
        Some(crate::emacs_core::emacs_char::string_char(&tmp[..written]).0)
    }

    /// No cheap contiguous window: storage is chunked. Callers fall back to
    /// the per-byte accessors.
    pub(in crate::buffer) fn contiguous_window_at(
        &self,
        _pos: usize,
    ) -> Option<(usize, *const u8, usize)> {
        None
    }

    pub(in crate::buffer) fn emacs_byte_pos_to_char_pos(&self, byte_pos: EmacsBytePos) -> CharPos0 {
        let byte_pos = byte_pos.get();
        assert!(
            byte_pos <= self.len(),
            "byte_to_char: byte_pos ({byte_pos}) > len ({})",
            self.len()
        );
        CharPos0::new(self.byte_to_char_in_node(&self.root, byte_pos))
    }

    pub(in crate::buffer) fn char_pos_to_emacs_byte_pos(&self, char_pos: CharPos0) -> EmacsBytePos {
        let char_pos = char_pos.get();
        let metrics = self.metrics();
        if char_pos >= metrics.chars_usize() {
            if char_pos > metrics.chars_usize() {
                tracing::debug!(
                    "piece tree char_to_byte: char_pos ({char_pos}) exceeds char_count ({}), clamping",
                    metrics.chars_usize()
                );
            }
            return EmacsBytePos::new(metrics.emacs_bytes_usize());
        }
        EmacsBytePos::new(self.char_to_byte_in_node(&self.root, char_pos))
    }

    pub(in crate::buffer) fn text_emacs_byte_range(&self, range: EmacsByteRange) -> String {
        let start = range.start().get();
        let end = range.end().get();
        assert!(start <= end, "text_range: start ({start}) > end ({end})");
        assert!(
            end <= self.len(),
            "text_range: end ({end}) > len ({})",
            self.len()
        );
        let mut out = Vec::with_capacity(end - start);
        self.copy_emacs_byte_range_to(range, &mut out);
        crate::emacs_core::emacs_char::emacs_bytes_to_lossy_string(&out, self.multibyte)
    }

    pub(in crate::buffer) fn copy_emacs_byte_range_to(
        &self,
        range: EmacsByteRange,
        out: &mut Vec<u8>,
    ) {
        let start = range.start().get();
        let end = range.end().get();
        assert!(
            start <= end,
            "copy_emacs_bytes_to: start ({start}) > end ({end})"
        );
        assert!(
            end <= self.len(),
            "copy_emacs_bytes_to: end ({end}) > emacs len ({})",
            self.len()
        );
        out.clear();
        // One spare slot: these bytes usually become a LispString payload,
        // whose constructor appends a trailing NUL — an exact-capacity Vec
        // would realloc and re-copy there.
        out.reserve(end - start + 1);
        self.for_each_emacs_byte_range_chunk(range, |chunk| {
            out.extend_from_slice(chunk);
            Ok::<(), ()>(())
        })
        .expect("infallible byte copy");
    }

    pub(in crate::buffer) fn for_each_emacs_byte_range_chunk<E>(
        &self,
        range: EmacsByteRange,
        mut f: impl FnMut(&[u8]) -> Result<(), E>,
    ) -> Result<(), E> {
        let start = range.start().get();
        let end = range.end().get();
        assert!(
            start <= end,
            "for_each_emacs_byte_chunk: start ({start}) > end ({end})"
        );
        assert!(
            end <= self.len(),
            "for_each_emacs_byte_chunk: end ({end}) > emacs len ({})",
            self.len()
        );
        self.for_each_range(&self.root, start, end, &mut f)
    }

    pub(in crate::buffer) fn has_contiguous_emacs_byte_range(&self, range: EmacsByteRange) -> bool {
        let start = range.start().get();
        let end = range.end().get();
        assert!(
            start <= end,
            "has_contiguous_emacs_bytes: start ({start}) > end ({end})"
        );
        assert!(
            end <= self.len(),
            "has_contiguous_emacs_bytes: end ({end}) > emacs len ({})",
            self.len()
        );
        start == end || self.contiguous_slice(start, end).is_some()
    }

    pub(in crate::buffer) fn with_contiguous_emacs_byte_range<R>(
        &self,
        range: EmacsByteRange,
        f: impl FnOnce(&[u8]) -> R,
    ) -> Option<R> {
        let start = range.start().get();
        let end = range.end().get();
        assert!(
            start <= end,
            "with_contiguous_emacs_bytes: start ({start}) > end ({end})"
        );
        assert!(
            end <= self.len(),
            "with_contiguous_emacs_bytes: end ({end}) > emacs len ({})",
            self.len()
        );
        if start == end {
            return Some(f(&[]));
        }
        self.contiguous_slice(start, end).map(f)
    }

    pub(in crate::buffer) fn insert_measured_emacs_bytes(
        &mut self,
        pos: EmacsBytePos,
        bytes: &[u8],
        extent: TextExtent,
    ) {
        let byte_pos = pos;
        let pos = byte_pos.get();
        assert!(
            pos <= self.len(),
            "insert_emacs_bytes_both: position {pos} out of range (len {})",
            self.len()
        );
        if bytes.is_empty() {
            return;
        }
        debug_assert_eq!(
            extent.emacs_bytes().get(),
            bytes.len(),
            "insert_emacs_bytes_both: caller-supplied byte count mismatches actual"
        );
        debug_assert_eq!(
            extent.chars(),
            emacs_char_count_bytes(bytes, self.multibyte),
            "insert_emacs_bytes_both: caller-supplied nchars mismatches actual"
        );
        self.emacs_byte_pos_to_char_pos(byte_pos);

        let add_start = self.add.len();
        self.add.extend_from_slice(bytes);
        let piece = self.node_for_piece(Piece::new(
            PieceSource::Add,
            SourceBytePos::new(add_start),
            extent,
        ));
        let root = self.root.take();
        let (left, right) = self.split_at_byte(root, pos);
        self.root = Self::merge(Self::merge(left, piece), right);
    }

    pub(in crate::buffer) fn delete_measured_range(&mut self, range: TextEditRange) {
        let start = range.byte_start().get();
        let end = range.byte_end().get();
        let nchars = range.char_len().get();
        assert!(
            start <= end,
            "delete_range_both: start ({start}) > end ({end})"
        );
        assert!(
            end <= self.len(),
            "delete_range_both: end ({end}) > len ({})",
            self.len()
        );
        if start == end {
            return;
        }
        debug_assert_eq!(
            nchars,
            self.emacs_byte_pos_to_char_pos(range.byte_end()).get()
                - self.emacs_byte_pos_to_char_pos(range.byte_start()).get(),
            "delete_range_both: caller-supplied nchars mismatches actual"
        );

        let root = self.root.take();
        let (left, rest) = self.split_at_byte(root, start);
        let (_deleted, right) = self.split_at_byte(rest, end - start);
        self.root = Self::merge(left, right);
    }

    pub(in crate::buffer) fn replace_measured_range(
        &mut self,
        replacement: TextReplacement,
        bytes: &[u8],
    ) {
        let old_range = replacement.old_range();
        if old_range.is_empty() {
            self.insert_measured_emacs_bytes(
                replacement.byte_start(),
                bytes,
                replacement.new_extent(),
            );
            return;
        }
        if bytes.is_empty() {
            self.delete_measured_range(old_range);
            return;
        }
        self.delete_measured_range(old_range);
        self.insert_measured_emacs_bytes(replacement.byte_start(), bytes, replacement.new_extent());
    }

    pub(in crate::buffer) fn replace_same_len_measured_range(
        &mut self,
        replacement: TextReplacement,
        bytes: &[u8],
    ) {
        let old_range = replacement.old_range();
        let start = old_range.byte_start().get();
        let end = old_range.byte_end().get();
        assert!(
            start <= end,
            "replace_same_len_range: start ({start}) > end ({end})"
        );
        assert!(
            end <= self.len(),
            "replace_same_len_range: end ({end}) > len ({})",
            self.len()
        );
        assert_eq!(
            bytes.len(),
            end - start,
            "replace_same_len_range: replacement Emacs-byte length ({}) must match replaced length ({})",
            bytes.len(),
            end - start
        );
        assert_eq!(
            replacement.new_byte_len().get(),
            bytes.len(),
            "replace_same_len_range: measured new byte length ({}) mismatches replacement bytes ({})",
            replacement.new_byte_len().get(),
            bytes.len()
        );
        if start == end {
            return;
        }

        debug_assert_eq!(
            old_range.char_start(),
            self.emacs_byte_pos_to_char_pos(old_range.byte_start()),
            "replace_same_len_range: measured start char mismatches storage"
        );
        debug_assert_eq!(
            old_range.char_end(),
            self.emacs_byte_pos_to_char_pos(old_range.byte_end()),
            "replace_same_len_range: measured end char mismatches storage"
        );
        debug_assert_eq!(
            replacement.new_char_len().get(),
            TextExtent::from_emacs_bytes(bytes, self.multibyte)
                .chars()
                .get(),
            "replace_same_len_range: measured new char count mismatches replacement bytes"
        );
        self.replace_measured_range(replacement, bytes);
    }

    pub(in crate::buffer) fn dump_text(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.len());
        self.copy_emacs_byte_range_to(
            EmacsByteRange::new(EmacsBytePos::ZERO, self.metrics().emacs_byte_end()),
            &mut out,
        );
        out
    }

    pub(in crate::buffer) fn metrics(&self) -> TextMetrics {
        node_metrics(&self.root)
    }

    fn rebuild_from_bytes(&mut self, bytes: Vec<u8>, multibyte: bool) {
        self.original = bytes;
        self.add.clear();
        self.multibyte = multibyte;
        self.root = None;
        self.next_piece_serial = TreapSerial::FIRST;
        self.root = self.node_for_piece(Piece::new(
            PieceSource::Original,
            SourceBytePos::ZERO,
            TextExtent::from_emacs_bytes(&self.original, multibyte),
        ));
    }

    fn next_priority(&mut self) -> TreapPriority {
        self.next_piece_serial.next_priority()
    }

    fn node_for_piece(&mut self, piece: Piece) -> Option<Box<PieceNode>> {
        (!piece.is_empty()).then(|| PieceNode::new(piece, self.next_priority()))
    }

    fn split_at_byte(
        &mut self,
        tree: Option<Box<PieceNode>>,
        byte_pos: usize,
    ) -> (Option<Box<PieceNode>>, Option<Box<PieceNode>>) {
        let Some(mut node) = tree else {
            assert_eq!(byte_pos, 0, "split_at_byte: byte_pos out of empty tree");
            return (None, None);
        };

        assert!(
            byte_pos <= node.metrics.emacs_bytes_usize(),
            "split_at_byte: byte_pos ({byte_pos}) > subtree len ({})",
            node.metrics.emacs_bytes_usize()
        );

        let left_metrics = node_metrics(&node.left);
        let piece_start = left_metrics.emacs_bytes_usize();
        let piece_end = piece_start + node.piece.len_usize();

        if byte_pos < piece_start {
            let (left, right_of_left) = self.split_at_byte(node.left.take(), byte_pos);
            node.left = right_of_left;
            node.refresh();
            return (left, Some(node));
        }

        if byte_pos > piece_end {
            let (left_of_right, right) =
                self.split_at_byte(node.right.take(), byte_pos - piece_end);
            node.right = left_of_right;
            node.refresh();
            return (Some(node), right);
        }

        let local = byte_pos - piece_start;
        if local == 0 {
            let left = node.left.take();
            node.refresh();
            return (left, Some(node));
        }
        if local == node.piece.len_usize() {
            let right = node.right.take();
            node.refresh();
            return (Some(node), right);
        }

        let (left_piece, right_piece) = self.split_piece(node.piece, PieceByteOffset::new(local));
        let left_tree = Self::merge(node.left.take(), self.node_for_piece(left_piece));
        let right_tree = Self::merge(self.node_for_piece(right_piece), node.right.take());
        (left_tree, right_tree)
    }

    fn split_piece(&self, piece: Piece, local_byte: PieceByteOffset) -> (Piece, Piece) {
        debug_assert!(local_byte.get() > 0 && local_byte.get() < piece.len_usize());
        let chars_before = CharLen::new(self.piece_byte_to_char(piece, local_byte.get()));
        piece.split_at_emacs_byte(local_byte, chars_before)
    }

    fn merge(
        left: Option<Box<PieceNode>>,
        right: Option<Box<PieceNode>>,
    ) -> Option<Box<PieceNode>> {
        match (left, right) {
            (None, right) => right,
            (left, None) => left,
            (Some(mut left), Some(mut right)) => {
                if left.priority >= right.priority {
                    left.right = Self::merge(left.right.take(), Some(right));
                    left.refresh();
                    Some(left)
                } else {
                    right.left = Self::merge(Some(left), right.left.take());
                    right.refresh();
                    Some(right)
                }
            }
        }
    }

    fn piece_slice(&self, piece: Piece) -> &[u8] {
        let start = piece.source_start().get();
        let end = piece.source_end().get();
        match piece.source {
            PieceSource::Original => &self.original[start..end],
            PieceSource::Add => &self.add[start..end],
        }
    }

    fn piece_byte_to_char(&self, piece: Piece, byte_pos: usize) -> usize {
        let slice = self.piece_slice(piece);
        emacs_byte_to_char_in_slice(slice, byte_pos, self.multibyte, "piece tree byte boundary")
    }

    fn byte_to_char_in_node(&self, tree: &Option<Box<PieceNode>>, byte_pos: usize) -> usize {
        let Some(node) = tree.as_ref() else {
            return 0;
        };

        let left = node_metrics(&node.left);
        if byte_pos <= left.emacs_bytes_usize() {
            return self.byte_to_char_in_node(&node.left, byte_pos);
        }

        let after_left = byte_pos - left.emacs_bytes_usize();
        if after_left <= node.piece.len_usize() {
            return left.chars_usize() + self.piece_byte_to_char(node.piece, after_left);
        }

        left.chars_usize()
            + node.piece.char_len_usize()
            + self.byte_to_char_in_node(&node.right, after_left - node.piece.len_usize())
    }

    fn char_to_byte_in_node(&self, tree: &Option<Box<PieceNode>>, char_pos: usize) -> usize {
        let Some(node) = tree.as_ref() else {
            return 0;
        };

        let left = node_metrics(&node.left);
        if char_pos <= left.chars_usize() {
            return self.char_to_byte_in_node(&node.left, char_pos);
        }

        let after_left = char_pos - left.chars_usize();
        if after_left <= node.piece.char_len_usize() {
            return left.emacs_bytes_usize()
                + emacs_char_to_byte_in_slice(
                    self.piece_slice(node.piece),
                    after_left,
                    self.multibyte,
                );
        }

        left.emacs_bytes_usize()
            + node.piece.len_usize()
            + self.char_to_byte_in_node(&node.right, after_left - node.piece.char_len_usize())
    }

    fn for_each_range<E>(
        &self,
        tree: &Option<Box<PieceNode>>,
        start: usize,
        end: usize,
        f: &mut impl FnMut(&[u8]) -> Result<(), E>,
    ) -> Result<(), E> {
        if start >= end {
            return Ok(());
        }
        let Some(node) = tree.as_ref() else {
            return Ok(());
        };

        let left = node_metrics(&node.left);
        if start < left.emacs_bytes_usize() {
            self.for_each_range(&node.left, start, end.min(left.emacs_bytes_usize()), f)?;
        }

        let piece_start = left.emacs_bytes_usize();
        let piece_end = piece_start + node.piece.len_usize();
        if start < piece_end && end > piece_start {
            let local_start = start.max(piece_start) - piece_start;
            let local_end = end.min(piece_end) - piece_start;
            f(&self.piece_slice(node.piece)[local_start..local_end])?;
        }

        if end > piece_end {
            self.for_each_range(
                &node.right,
                start.saturating_sub(piece_end),
                end - piece_end,
                f,
            )?;
        }

        Ok(())
    }

    fn contiguous_slice(&self, start: usize, end: usize) -> Option<&[u8]> {
        if start == end {
            return Some(&[]);
        }
        self.contiguous_slice_in_node(&self.root, start, end)
    }

    fn contiguous_slice_in_node(
        &self,
        tree: &Option<Box<PieceNode>>,
        start: usize,
        end: usize,
    ) -> Option<&[u8]> {
        let node = tree.as_ref()?;
        let left = node_metrics(&node.left);
        if end <= left.emacs_bytes_usize() {
            return self.contiguous_slice_in_node(&node.left, start, end);
        }

        let piece_start = left.emacs_bytes_usize();
        let piece_end = piece_start + node.piece.len_usize();
        if start >= piece_end {
            return self.contiguous_slice_in_node(&node.right, start - piece_end, end - piece_end);
        }

        if start >= piece_start && end <= piece_end {
            let local_start = start - piece_start;
            let local_end = end - piece_start;
            return Some(&self.piece_slice(node.piece)[local_start..local_end]);
        }

        None
    }
}

impl fmt::Display for PieceTreeTextBackend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text_emacs_byte_range(EmacsByteRange::new(
            EmacsBytePos::ZERO,
            self.metrics().emacs_byte_end(),
        )))
    }
}

impl fmt::Debug for PieceTreeTextBackend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PieceTreeTextBackend")
            .field("bytes", &self.len())
            .field("chars", &self.metrics().chars_usize())
            .field("multibyte", &self.multibyte)
            .finish()
    }
}

fn node_metrics(node: &Option<Box<PieceNode>>) -> TextMetrics {
    node.as_ref().map(|node| node.metrics).unwrap_or_default()
}

fn piece_metrics(piece: Piece) -> TextMetrics {
    piece.metrics()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::gap_buffer::GapBuffer;
    use proptest::prelude::*;

    fn byte_range(start: usize, end: usize) -> EmacsByteRange {
        assert!(start <= end);
        EmacsByteRange::from_start_len(EmacsBytePos::new(start), EmacsByteLen::new(end - start))
    }

    fn assert_matches_gap(piece: &PieceTreeTextBackend, gap: &GapBuffer) {
        assert_eq!(piece.len(), gap.len());
        assert_eq!(piece.metrics().chars_usize(), gap.char_count());
        assert_eq!(piece.to_string(), gap.to_string());

        let mut piece_bytes = Vec::new();
        let mut gap_bytes = Vec::new();
        copy_piece_bytes(piece, 0, piece.len(), &mut piece_bytes);
        gap.copy_emacs_byte_range_to(byte_range(0, gap.len()), &mut gap_bytes);
        assert_eq!(piece_bytes, gap_bytes);

        for byte_pos in 0..piece.len() {
            assert_eq!(piece_byte_at(piece, byte_pos), gap_byte_at(gap, byte_pos));
            assert_eq!(
                piece_emacs_byte_at(piece, byte_pos),
                gap_emacs_byte_at(gap, byte_pos)
            );
        }
        assert_eq!(piece_emacs_byte_at(piece, piece.len()), None);
        assert_eq!(gap_emacs_byte_at(gap, gap.len()), None);

        for char_pos in 0..=piece.metrics().chars_usize() {
            let piece_byte = piece_char_to_byte(piece, char_pos);
            let gap_byte = gap_char_to_byte(gap, char_pos);
            assert_eq!(piece_byte, gap_byte, "char_to_byte({char_pos})");
            assert_eq!(piece_byte_to_char(piece, piece_byte), char_pos);
            assert_eq!(gap_byte_to_char(gap, gap_byte), char_pos);
            if char_pos < piece.metrics().chars_usize() {
                assert_eq!(
                    piece_char_code_at(piece, piece_byte),
                    gap_char_code_at(gap, gap_byte)
                );
            }
        }
    }

    fn gap_byte_to_char(gap: &GapBuffer, byte_pos: usize) -> usize {
        gap.emacs_byte_pos_to_char_pos(EmacsBytePos::new(byte_pos))
            .get()
    }

    fn gap_char_to_byte(gap: &GapBuffer, char_pos: usize) -> usize {
        gap.char_pos_to_emacs_byte_pos(CharPos0::new(char_pos))
            .get()
    }

    fn gap_byte_at(gap: &GapBuffer, byte_pos: usize) -> u8 {
        gap.byte_at_emacs_byte_pos(EmacsBytePos::new(byte_pos))
    }

    fn gap_emacs_byte_at(gap: &GapBuffer, byte_pos: usize) -> Option<u8> {
        gap.emacs_byte_at_pos(EmacsBytePos::new(byte_pos))
    }

    fn gap_char_code_at(gap: &GapBuffer, byte_pos: usize) -> Option<u32> {
        gap.char_code_at_emacs_byte_pos(EmacsBytePos::new(byte_pos))
    }

    fn insert_gap_str(gap: &mut GapBuffer, byte_pos: usize, text: &str) {
        gap.insert_storage_string_at_emacs_byte_pos(EmacsBytePos::new(byte_pos), text);
    }

    fn insert_gap_bytes_both(gap: &mut GapBuffer, byte_pos: usize, bytes: &[u8], nchars: usize) {
        gap.insert_emacs_bytes_at_emacs_byte_pos_with_char_len(
            EmacsBytePos::new(byte_pos),
            bytes,
            crate::buffer::CharLen::new(nchars),
        );
    }

    fn delete_gap_range_both(gap: &mut GapBuffer, start: usize, end: usize, nchars: usize) {
        gap.delete_emacs_byte_range_with_char_len(
            byte_range(start, end),
            crate::buffer::CharLen::new(nchars),
        );
    }

    fn replace_gap_same_len(gap: &mut GapBuffer, start: usize, end: usize, replacement: &[u8]) {
        gap.replace_same_len_emacs_byte_range(byte_range(start, end), replacement);
    }

    fn sample_insert(seed: u8) -> &'static str {
        match seed % 8 {
            0 => "a",
            1 => "XYZ",
            2 => "é",
            3 => "日本",
            4 => "\n",
            5 => "🙂",
            6 => "ßΩ",
            _ => "end",
        }
    }

    fn replacement_bytes_for_len(len: usize, seed: u8) -> Option<Vec<u8>> {
        let candidates = ["Q", "z", "\n", "é", "ß", "日", "界", "🙂", "🚀"];
        let matches: Vec<Vec<u8>> = candidates
            .iter()
            .map(|candidate| {
                crate::emacs_core::string_escape::storage_string_to_buffer_bytes(candidate, true)
            })
            .filter(|bytes| bytes.len() == len)
            .collect();
        (!matches.is_empty()).then(|| matches[seed as usize % matches.len()].clone())
    }

    fn sample_unibyte_insert(seed: u8) -> Vec<u8> {
        match seed % 7 {
            0 => vec![b'a'],
            1 => vec![0xFF],
            2 => vec![b'\n'],
            3 => vec![0x80, b'Z'],
            4 => vec![b'X', b'Y', b'Z'],
            5 => vec![0, 1, 2],
            _ => vec![seed, seed.wrapping_add(1)],
        }
    }

    fn piece_byte_to_char(piece: &PieceTreeTextBackend, byte_pos: usize) -> usize {
        piece
            .emacs_byte_pos_to_char_pos(EmacsBytePos::new(byte_pos))
            .get()
    }

    fn piece_char_to_byte(piece: &PieceTreeTextBackend, char_pos: usize) -> usize {
        piece
            .char_pos_to_emacs_byte_pos(CharPos0::new(char_pos))
            .get()
    }

    fn piece_byte_at(piece: &PieceTreeTextBackend, byte_pos: usize) -> u8 {
        piece.byte_at_emacs_byte_pos(EmacsBytePos::new(byte_pos))
    }

    fn piece_emacs_byte_at(piece: &PieceTreeTextBackend, byte_pos: usize) -> Option<u8> {
        piece.emacs_byte_at_pos(EmacsBytePos::new(byte_pos))
    }

    fn piece_char_code_at(piece: &PieceTreeTextBackend, byte_pos: usize) -> Option<u32> {
        piece.char_code_at_emacs_byte_pos(EmacsBytePos::new(byte_pos))
    }

    fn copy_piece_bytes(piece: &PieceTreeTextBackend, start: usize, end: usize, out: &mut Vec<u8>) {
        piece.copy_emacs_byte_range_to(byte_range(start, end), out);
    }

    fn insert_piece_str(piece: &mut PieceTreeTextBackend, byte_pos: usize, text: &str) {
        let bytes =
            crate::emacs_core::string_escape::storage_string_to_buffer_bytes(text, piece.multibyte);
        let extent = TextExtent::from_emacs_bytes(&bytes, piece.multibyte);
        piece.insert_measured_emacs_bytes(EmacsBytePos::new(byte_pos), &bytes, extent);
    }

    fn insert_piece_bytes_both(
        piece: &mut PieceTreeTextBackend,
        byte_pos: usize,
        bytes: &[u8],
        nchars: usize,
    ) {
        piece.insert_measured_emacs_bytes(
            EmacsBytePos::new(byte_pos),
            bytes,
            TextExtent::from_usize(nchars, bytes.len()),
        );
    }

    fn delete_piece_range_both(
        piece: &mut PieceTreeTextBackend,
        start: usize,
        end: usize,
        nchars: usize,
    ) {
        let start_char = piece_byte_to_char(piece, start);
        piece.delete_measured_range(TextEditRange::from_usize(
            start,
            end,
            start_char,
            start_char + nchars,
        ));
    }

    fn replace_piece_same_len(
        piece: &mut PieceTreeTextBackend,
        start: usize,
        end: usize,
        replacement: &[u8],
    ) {
        let range = byte_range(start, end);
        let edit_range = TextEditRange::new(
            range,
            piece.emacs_byte_pos_to_char_pos(range.start()),
            piece.emacs_byte_pos_to_char_pos(range.end()),
        );
        piece.replace_same_len_measured_range(
            TextReplacement::new(
                edit_range,
                TextExtent::from_emacs_bytes(replacement, piece.multibyte),
            ),
            replacement,
        );
    }

    #[test]
    fn piece_tree_reports_metrics_and_layout() {
        let backend = PieceTreeTextBackend::from_str("éz");
        assert_eq!(
            backend.debug_layout(),
            TextBackendDebugLayout::PieceTree(TextMetrics::from_usize(2, 3))
        );
        assert_eq!(piece_char_to_byte(&backend, 1), "é".len());
        assert_eq!(piece_byte_to_char(&backend, "é".len()), 1);
    }

    #[test]
    fn piece_tree_insert_delete_and_replace_match_gap_buffer() {
        let mut piece = PieceTreeTextBackend::from_str("abécd日本");
        let mut gap = GapBuffer::from_str("abécd日本");
        assert_matches_gap(&piece, &gap);

        let pos = piece_char_to_byte(&piece, 2);
        insert_piece_str(&mut piece, pos, "XYZ");
        insert_gap_str(&mut gap, pos, "XYZ");
        assert_matches_gap(&piece, &gap);

        let start = piece_char_to_byte(&piece, 1);
        let end = piece_char_to_byte(&piece, 5);
        let nchars = piece_byte_to_char(&piece, end) - piece_byte_to_char(&piece, start);
        delete_piece_range_both(&mut piece, start, end, nchars);
        delete_gap_range_both(&mut gap, start, end, nchars);
        assert_matches_gap(&piece, &gap);

        let start = piece_char_to_byte(&piece, 1);
        let end = piece_char_to_byte(&piece, 2);
        replace_piece_same_len(&mut piece, start, end, "ß".as_bytes());
        replace_gap_same_len(&mut gap, start, end, "ß".as_bytes());
        assert_matches_gap(&piece, &gap);
    }

    #[test]
    fn piece_tree_visits_piece_chunks_without_coalescing() {
        let mut backend = PieceTreeTextBackend::from_str("abcdef");
        insert_piece_str(&mut backend, 3, "XY");
        delete_piece_range_both(&mut backend, 4, 5, 1);

        let mut chunks = Vec::new();
        backend
            .for_each_emacs_byte_range_chunk(byte_range(1, 7), |chunk| {
                chunks.push(chunk.to_vec());
                Ok::<(), ()>(())
            })
            .unwrap();
        assert_eq!(chunks, vec![b"bc".to_vec(), b"X".to_vec(), b"def".to_vec()]);
    }

    #[test]
    fn piece_tree_unibyte_raw_bytes_round_trip() {
        let raw = vec![0xFF, b'A', 0x80];
        let mut backend = PieceTreeTextBackend::from_emacs_bytes(&raw, false);
        insert_piece_bytes_both(&mut backend, 1, &[b'\n'], 1);

        assert!(!backend.is_multibyte());
        assert_eq!(backend.metrics().chars_usize(), 4);
        assert_eq!(backend.metrics().emacs_bytes_usize(), 4);
        assert_eq!(piece_byte_to_char(&backend, 3), 3);
        assert_eq!(piece_char_to_byte(&backend, 4), 4);

        let mut bytes = Vec::new();
        copy_piece_bytes(&backend, 0, backend.len(), &mut bytes);
        assert_eq!(bytes, vec![0xFF, b'\n', b'A', 0x80]);
    }

    proptest! {
        #[test]
        fn piece_tree_random_edit_sequences_match_gap_buffer(
            ops in prop::collection::vec((0u8..3, 0usize..200, 0usize..200, 0u8..32), 0..80)
        ) {
            let mut piece = PieceTreeTextBackend::from_str("abécd日本");
            let mut gap = GapBuffer::from_str("abécd日本");
            assert_matches_gap(&piece, &gap);

            for (kind, a, b, seed) in ops {
                match kind {
                    0 => {
                        let char_pos = a % (piece.metrics().chars_usize() + 1);
                        let byte_pos = piece_char_to_byte(&piece, char_pos);
                        let text = sample_insert(seed);
                        insert_piece_str(&mut piece, byte_pos, text);
                        insert_gap_str(&mut gap, byte_pos, text);
                    }
                    1 => {
                        if piece.metrics().chars_usize() > 0 {
                            let char_a = a % (piece.metrics().chars_usize() + 1);
                            let char_b = b % (piece.metrics().chars_usize() + 1);
                            let start_char = char_a.min(char_b);
                            let end_char = char_a.max(char_b);
                            let start = piece_char_to_byte(&piece, start_char);
                            let end = piece_char_to_byte(&piece, end_char);
                            let nchars = end_char - start_char;
                            delete_piece_range_both(&mut piece, start, end, nchars);
                            delete_gap_range_both(&mut gap, start, end, nchars);
                        }
                    }
                    _ => {
                        if piece.metrics().chars_usize() > 0 {
                            let char_pos = a % piece.metrics().chars_usize();
                            let start = piece_char_to_byte(&piece, char_pos);
                            let end = piece_char_to_byte(&piece, char_pos + 1);
                            if let Some(replacement) = replacement_bytes_for_len(end - start, seed) {
                                replace_piece_same_len(&mut piece, start, end, &replacement);
                                replace_gap_same_len(&mut gap, start, end, &replacement);
                            }
                        }
                    }
                }
                assert_matches_gap(&piece, &gap);
            }
        }
    }

    proptest! {
        #[test]
        fn piece_tree_unibyte_random_edit_sequences_match_gap_buffer(
            ops in prop::collection::vec((0u8..3, 0usize..200, 0usize..200, any::<u8>()), 0..80)
        ) {
            let initial = vec![0xFF, b'A', 0x80, b'\n', b'Z'];
            let mut piece = PieceTreeTextBackend::from_emacs_bytes(&initial, false);
            let mut gap = GapBuffer::from_emacs_bytes(&initial, false);
            assert_matches_gap(&piece, &gap);

            for (kind, a, b, seed) in ops {
                match kind {
                    0 => {
                        let byte_pos = a % (piece.len() + 1);
                        let bytes = sample_unibyte_insert(seed);
                        insert_piece_bytes_both(&mut piece, byte_pos, &bytes, bytes.len());
                        insert_gap_bytes_both(&mut gap, byte_pos, &bytes, bytes.len());
                    }
                    1 => {
                        if !piece.is_empty() {
                            let byte_a = a % (piece.len() + 1);
                            let byte_b = b % (piece.len() + 1);
                            let start = byte_a.min(byte_b);
                            let end = byte_a.max(byte_b);
                            delete_piece_range_both(&mut piece, start, end, end - start);
                            delete_gap_range_both(&mut gap, start, end, end - start);
                        }
                    }
                    _ => {
                        if !piece.is_empty() {
                            let start = a % piece.len();
                            let end = (start + 1 + (b % 4)).min(piece.len());
                            let replacement = vec![seed; end - start];
                            replace_piece_same_len(&mut piece, start, end, &replacement);
                            replace_gap_same_len(&mut gap, start, end, &replacement);
                        }
                    }
                }
                assert_matches_gap(&piece, &gap);
            }
        }
    }
}
