use std::fmt;

use crate::buffer::position::{CharLen, CharPos0, EmacsByteLen, EmacsBytePos, EmacsByteRange};
#[cfg(test)]
use crate::buffer::text::TextBackendDebugLayout;
use crate::buffer::text::{
    BufferTextBytesSnapshot, TextEditRange, TextExtent, TextMetrics, TextReplacement,
    emacs_byte_to_char_in_slice, emacs_char_count_bytes, emacs_char_to_byte_in_slice,
    is_emacs_char_boundary,
};

use super::treap::{TreapPriority, TreapSerial};

const MAX_LEAF_BYTES: EmacsByteLen = EmacsByteLen::new(1024);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
struct ChunkByteOffset(usize);

#[derive(Clone)]
struct RopeChunk {
    bytes: Vec<u8>,
    extent: TextExtent,
}

impl ChunkByteOffset {
    const fn new(pos: usize) -> Self {
        Self(pos)
    }

    const fn get(self) -> usize {
        self.0
    }
}

impl RopeChunk {
    fn new(bytes: Vec<u8>, multibyte: bool) -> Self {
        let extent = TextExtent::from_emacs_bytes(&bytes, multibyte);
        Self { bytes, extent }
    }

    fn emacs_byte_len(&self) -> EmacsByteLen {
        self.extent.emacs_bytes()
    }

    fn char_len(&self) -> CharLen {
        self.extent.chars()
    }

    fn len_usize(&self) -> usize {
        self.emacs_byte_len().get()
    }

    fn char_len_usize(&self) -> usize {
        self.char_len().get()
    }

    fn metrics(&self) -> TextMetrics {
        TextMetrics::from_extent(self.extent)
    }

    fn split_at(&self, byte_pos: ChunkByteOffset, multibyte: bool) -> (Self, Self) {
        let byte_pos = byte_pos.get();
        debug_assert!(byte_pos > 0 && byte_pos < self.len_usize());
        assert!(
            is_emacs_char_boundary(&self.bytes, byte_pos, multibyte),
            "rope chunk split position {byte_pos} is not an Emacs character boundary",
        );
        let left = self.bytes[..byte_pos].to_vec();
        let right = self.bytes[byte_pos..].to_vec();
        (Self::new(left, multibyte), Self::new(right, multibyte))
    }
}

#[derive(Clone)]
struct RopeNode {
    chunk: RopeChunk,
    priority: TreapPriority,
    metrics: TextMetrics,
    left: Option<Box<RopeNode>>,
    right: Option<Box<RopeNode>>,
}

impl RopeNode {
    fn new(chunk: RopeChunk, priority: TreapPriority) -> Box<Self> {
        let metrics = chunk.metrics();
        Box::new(Self {
            chunk,
            priority,
            metrics,
            left: None,
            right: None,
        })
    }

    fn refresh(&mut self) {
        let left = node_metrics(&self.left);
        let right = node_metrics(&self.right);
        self.metrics = left.add_extent(self.chunk.extent).add_metrics(right);
    }
}

#[derive(Clone)]
pub(in crate::buffer) struct RopeTextBackend {
    root: Option<Box<RopeNode>>,
    multibyte: bool,
    next_node_serial: TreapSerial,
}

impl RopeTextBackend {
    pub(in crate::buffer) fn new() -> Self {
        Self {
            root: None,
            multibyte: true,
            next_node_serial: TreapSerial::FIRST,
        }
    }

    pub(in crate::buffer) fn from_str(text: &str) -> Self {
        let decoded = crate::buffer::text::storage_string_to_emacs_buffer_bytes(text);
        Self::from_emacs_bytes(decoded.bytes(), decoded.multibyte())
    }

    pub(in crate::buffer) fn from_emacs_bytes(bytes: &[u8], multibyte: bool) -> Self {
        let mut backend = Self {
            root: None,
            multibyte,
            next_node_serial: TreapSerial::FIRST,
        };
        backend.root = backend.tree_for_bytes(bytes);
        backend
    }

    pub(in crate::buffer) fn from_snapshot(snapshot: BufferTextBytesSnapshot) -> Self {
        Self::from_emacs_bytes(snapshot.bytes(), snapshot.is_multibyte())
    }

    #[cfg(test)]
    pub(in crate::buffer) fn debug_layout(&self) -> TextBackendDebugLayout {
        TextBackendDebugLayout::Rope(self.metrics())
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
                    "rope char_to_byte: char_pos ({char_pos}) exceeds char_count ({}), clamping",
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
        out.reserve(end - start);
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

        let insertion = self.tree_for_bytes(bytes);
        let root = self.root.take();
        let (left, right) = self.split_at_byte(root, pos);
        let merged = self.merge_adjacent(left, insertion);
        self.root = self.merge_adjacent(merged, right);
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
        self.root = self.merge_adjacent(left, right);
    }

    pub(in crate::buffer) fn replace_measured_range(
        &mut self,
        replacement: TextReplacement,
        bytes: &[u8],
    ) {
        let old_range = replacement.old_range();
        let start = old_range.byte_start().get();
        let end = old_range.byte_end().get();
        assert!(
            start <= end,
            "replace_range_both: start ({start}) > end ({end})"
        );
        assert!(
            end <= self.len(),
            "replace_range_both: end ({end}) > len ({})",
            self.len()
        );
        if old_range.is_empty() && bytes.is_empty() {
            return;
        }
        debug_assert_eq!(
            replacement.new_byte_len().get(),
            bytes.len(),
            "replace_range_both: caller-supplied new byte count mismatches actual"
        );
        debug_assert_eq!(
            replacement.new_char_len(),
            emacs_char_count_bytes(bytes, self.multibyte),
            "replace_range_both: caller-supplied new char count mismatches actual"
        );
        self.emacs_byte_pos_to_char_pos(old_range.byte_start());
        self.emacs_byte_pos_to_char_pos(old_range.byte_end());

        let root = self.root.take();
        let (left, rest) = self.split_at_byte(root, start);
        let (_deleted, right) = self.split_at_byte(rest, end - start);
        let inserted = self.tree_for_bytes(bytes);
        let merged = self.merge_adjacent(left, inserted);
        self.root = self.merge_adjacent(merged, right);
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
        self.root = None;
        self.multibyte = multibyte;
        self.next_node_serial = TreapSerial::FIRST;
        self.root = self.tree_for_bytes(&bytes);
    }

    fn tree_for_bytes(&mut self, bytes: &[u8]) -> Option<Box<RopeNode>> {
        let mut tree = None;
        let mut rest = bytes;
        while !rest.is_empty() {
            let take = split_leaf_len(rest, self.multibyte).get();
            let chunk = RopeChunk::new(rest[..take].to_vec(), self.multibyte);
            let node = Some(RopeNode::new(chunk, self.next_priority()));
            tree = self.merge_adjacent(tree, node);
            rest = &rest[take..];
        }
        tree
    }

    fn next_priority(&mut self) -> TreapPriority {
        self.next_node_serial.next_priority()
    }

    fn split_at_byte(
        &mut self,
        tree: Option<Box<RopeNode>>,
        byte_pos: usize,
    ) -> (Option<Box<RopeNode>>, Option<Box<RopeNode>>) {
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
        let chunk_start = left_metrics.emacs_bytes_usize();
        let chunk_end = chunk_start + node.chunk.len_usize();

        if byte_pos < chunk_start {
            let (left, right_of_left) = self.split_at_byte(node.left.take(), byte_pos);
            node.left = right_of_left;
            node.refresh();
            return (left, Some(node));
        }

        if byte_pos > chunk_end {
            let (left_of_right, right) =
                self.split_at_byte(node.right.take(), byte_pos - chunk_end);
            node.right = left_of_right;
            node.refresh();
            return (Some(node), right);
        }

        let local = byte_pos - chunk_start;
        if local == 0 {
            let left = node.left.take();
            node.refresh();
            return (left, Some(node));
        }
        if local == node.chunk.len_usize() {
            let right = node.right.take();
            node.refresh();
            return (Some(node), right);
        }

        let (left_chunk, right_chunk) = node
            .chunk
            .split_at(ChunkByteOffset::new(local), self.multibyte);
        let left_node = Some(RopeNode::new(left_chunk, self.next_priority()));
        let right_node = Some(RopeNode::new(right_chunk, self.next_priority()));
        let left_tree = self.merge_adjacent(node.left.take(), left_node);
        let right_tree = self.merge_adjacent(right_node, node.right.take());
        (left_tree, right_tree)
    }

    fn merge_adjacent(
        &mut self,
        left: Option<Box<RopeNode>>,
        right: Option<Box<RopeNode>>,
    ) -> Option<Box<RopeNode>> {
        match (left, right) {
            (None, right) => right,
            (left, None) => left,
            (Some(left), Some(right)) => {
                if rightmost_chunk_len(&left).add_len(leftmost_chunk_len(&right)) <= MAX_LEAF_BYTES
                {
                    let (left, left_chunk) = pop_rightmost(Some(left));
                    let (right_chunk, right) = pop_leftmost(Some(right));
                    let mut bytes = left_chunk.bytes;
                    bytes.extend_from_slice(&right_chunk.bytes);
                    let combined = Some(RopeNode::new(
                        RopeChunk::new(bytes, self.multibyte),
                        self.next_priority(),
                    ));
                    let merged_left = self.merge_adjacent(left, combined);
                    return self.merge_adjacent(merged_left, right);
                }

                Self::merge(Some(left), Some(right))
            }
        }
    }

    fn merge(left: Option<Box<RopeNode>>, right: Option<Box<RopeNode>>) -> Option<Box<RopeNode>> {
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

    fn chunk_byte_to_char(&self, chunk: &RopeChunk, byte_pos: usize) -> usize {
        emacs_byte_to_char_in_slice(&chunk.bytes, byte_pos, self.multibyte, "rope byte boundary")
    }

    fn byte_to_char_in_node(&self, tree: &Option<Box<RopeNode>>, byte_pos: usize) -> usize {
        let Some(node) = tree.as_ref() else {
            return 0;
        };

        let left = node_metrics(&node.left);
        if byte_pos <= left.emacs_bytes_usize() {
            return self.byte_to_char_in_node(&node.left, byte_pos);
        }

        let after_left = byte_pos - left.emacs_bytes_usize();
        if after_left <= node.chunk.len_usize() {
            return left.chars_usize() + self.chunk_byte_to_char(&node.chunk, after_left);
        }

        left.chars_usize()
            + node.chunk.char_len_usize()
            + self.byte_to_char_in_node(&node.right, after_left - node.chunk.len_usize())
    }

    fn char_to_byte_in_node(&self, tree: &Option<Box<RopeNode>>, char_pos: usize) -> usize {
        let Some(node) = tree.as_ref() else {
            return 0;
        };

        let left = node_metrics(&node.left);
        if char_pos <= left.chars_usize() {
            return self.char_to_byte_in_node(&node.left, char_pos);
        }

        let after_left = char_pos - left.chars_usize();
        if after_left <= node.chunk.char_len_usize() {
            return left.emacs_bytes_usize()
                + emacs_char_to_byte_in_slice(&node.chunk.bytes, after_left, self.multibyte);
        }

        left.emacs_bytes_usize()
            + node.chunk.len_usize()
            + self.char_to_byte_in_node(&node.right, after_left - node.chunk.char_len_usize())
    }

    fn for_each_range<E>(
        &self,
        tree: &Option<Box<RopeNode>>,
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

        let chunk_start = left.emacs_bytes_usize();
        let chunk_end = chunk_start + node.chunk.len_usize();
        if start < chunk_end && end > chunk_start {
            let local_start = start.max(chunk_start) - chunk_start;
            let local_end = end.min(chunk_end) - chunk_start;
            f(&node.chunk.bytes[local_start..local_end])?;
        }

        if end > chunk_end {
            self.for_each_range(
                &node.right,
                start.saturating_sub(chunk_end),
                end - chunk_end,
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

    fn contiguous_slice_in_node<'a>(
        &self,
        tree: &'a Option<Box<RopeNode>>,
        start: usize,
        end: usize,
    ) -> Option<&'a [u8]> {
        let node = tree.as_ref()?;
        let left = node_metrics(&node.left);
        if end <= left.emacs_bytes_usize() {
            return self.contiguous_slice_in_node(&node.left, start, end);
        }

        let chunk_start = left.emacs_bytes_usize();
        let chunk_end = chunk_start + node.chunk.len_usize();
        if start >= chunk_end {
            return self.contiguous_slice_in_node(&node.right, start - chunk_end, end - chunk_end);
        }

        if start >= chunk_start && end <= chunk_end {
            let local_start = start - chunk_start;
            let local_end = end - chunk_start;
            return Some(&node.chunk.bytes[local_start..local_end]);
        }

        None
    }

    #[cfg(test)]
    fn debug_chunk_lengths(&self) -> Vec<usize> {
        let mut lengths = Vec::new();
        collect_chunk_lengths(&self.root, &mut lengths);
        lengths
    }

    #[cfg(test)]
    fn assert_invariants(&self) {
        let metrics = assert_node_invariants(&self.root, self.multibyte);
        assert_eq!(metrics, self.metrics());
    }
}

impl fmt::Display for RopeTextBackend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text_emacs_byte_range(EmacsByteRange::new(
            EmacsBytePos::ZERO,
            self.metrics().emacs_byte_end(),
        )))
    }
}

impl fmt::Debug for RopeTextBackend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RopeTextBackend")
            .field("bytes", &self.len())
            .field("chars", &self.metrics().chars_usize())
            .field("multibyte", &self.multibyte)
            .finish()
    }
}

fn node_metrics(node: &Option<Box<RopeNode>>) -> TextMetrics {
    node.as_ref().map(|node| node.metrics).unwrap_or_default()
}

fn leftmost_chunk_len(node: &RopeNode) -> EmacsByteLen {
    if let Some(left) = node.left.as_ref() {
        return leftmost_chunk_len(left);
    }
    node.chunk.emacs_byte_len()
}

fn rightmost_chunk_len(node: &RopeNode) -> EmacsByteLen {
    if let Some(right) = node.right.as_ref() {
        return rightmost_chunk_len(right);
    }
    node.chunk.emacs_byte_len()
}

fn pop_leftmost(mut tree: Option<Box<RopeNode>>) -> (RopeChunk, Option<Box<RopeNode>>) {
    let mut node = tree.take().expect("pop_leftmost requires a non-empty tree");
    if node.left.is_none() {
        let right = node.right.take();
        return (node.chunk, right);
    }

    let (chunk, left) = pop_leftmost(node.left.take());
    node.left = left;
    node.refresh();
    (chunk, Some(node))
}

fn pop_rightmost(mut tree: Option<Box<RopeNode>>) -> (Option<Box<RopeNode>>, RopeChunk) {
    let mut node = tree
        .take()
        .expect("pop_rightmost requires a non-empty tree");
    if node.right.is_none() {
        let left = node.left.take();
        return (left, node.chunk);
    }

    let (right, chunk) = pop_rightmost(node.right.take());
    node.right = right;
    node.refresh();
    (Some(node), chunk)
}

#[cfg(test)]
fn collect_chunk_lengths(node: &Option<Box<RopeNode>>, out: &mut Vec<usize>) {
    let Some(node) = node.as_ref() else {
        return;
    };
    collect_chunk_lengths(&node.left, out);
    out.push(node.chunk.len_usize());
    collect_chunk_lengths(&node.right, out);
}

#[cfg(test)]
fn assert_node_invariants(node: &Option<Box<RopeNode>>, multibyte: bool) -> TextMetrics {
    let Some(node) = node.as_ref() else {
        return TextMetrics::ZERO;
    };

    assert!(!node.chunk.bytes.is_empty(), "rope leaf must not be empty");
    assert!(
        node.chunk.emacs_byte_len() <= MAX_LEAF_BYTES,
        "rope leaf length {} exceeds max {}",
        node.chunk.len_usize(),
        MAX_LEAF_BYTES.get()
    );
    assert_eq!(
        node.chunk.char_len(),
        emacs_char_count_bytes(&node.chunk.bytes, multibyte),
        "rope leaf cached char count diverged"
    );
    if let Some(left) = node.left.as_ref() {
        assert!(
            node.priority >= left.priority,
            "rope treap priority invariant failed on left child"
        );
    }
    if let Some(right) = node.right.as_ref() {
        assert!(
            node.priority >= right.priority,
            "rope treap priority invariant failed on right child"
        );
    }

    let left = assert_node_invariants(&node.left, multibyte);
    let right = assert_node_invariants(&node.right, multibyte);
    let expected = left.add_extent(node.chunk.extent).add_metrics(right);
    assert_eq!(
        node.metrics, expected,
        "rope cached subtree metrics diverged"
    );
    expected
}

fn split_leaf_len(bytes: &[u8], multibyte: bool) -> EmacsByteLen {
    if bytes.len() <= MAX_LEAF_BYTES.get() {
        return EmacsByteLen::new(bytes.len());
    }
    if !multibyte {
        return MAX_LEAF_BYTES;
    }

    let mut end = MAX_LEAF_BYTES.get();
    while end > 0 && !is_emacs_char_boundary(bytes, end, multibyte) {
        end -= 1;
    }
    assert!(end > 0, "Emacs multibyte character exceeds rope leaf size");
    EmacsByteLen::new(end)
}

#[cfg(test)]
#[path = "rope/tests/rope_test.rs"]
mod tests;
