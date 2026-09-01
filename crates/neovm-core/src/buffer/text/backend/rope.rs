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
mod tests {
    use super::*;
    use crate::buffer::gap_buffer::GapBuffer;
    use proptest::prelude::*;

    fn byte_range(start: usize, end: usize) -> EmacsByteRange {
        assert!(start <= end);
        EmacsByteRange::from_start_len(EmacsBytePos::new(start), EmacsByteLen::new(end - start))
    }

    fn assert_matches_gap(rope: &RopeTextBackend, gap: &GapBuffer) {
        rope.assert_invariants();
        assert_eq!(rope.len(), gap.len());
        assert_eq!(rope.metrics().chars_usize(), gap.char_count());
        assert_eq!(rope.to_string(), gap.to_string());

        let mut rope_bytes = Vec::new();
        let mut gap_bytes = Vec::new();
        rope.copy_emacs_byte_range_to(byte_range(0, rope.len()), &mut rope_bytes);
        gap.copy_emacs_byte_range_to(byte_range(0, gap.len()), &mut gap_bytes);
        assert_eq!(rope_bytes, gap_bytes);

        for byte_pos in 0..rope.len() {
            assert_eq!(rope_byte_at(rope, byte_pos), gap_byte_at(gap, byte_pos));
            assert_eq!(
                rope_emacs_byte_at(rope, byte_pos),
                gap_emacs_byte_at(gap, byte_pos)
            );
        }
        assert_eq!(rope_emacs_byte_at(rope, rope.len()), None);
        assert_eq!(gap_emacs_byte_at(gap, gap.len()), None);

        for char_pos in 0..=rope.metrics().chars_usize() {
            let rope_byte = rope_char_to_byte(rope, char_pos);
            let gap_byte = gap_char_to_byte(gap, char_pos);
            assert_eq!(rope_byte, gap_byte, "char_to_byte({char_pos})");
            assert_eq!(rope_byte_to_char(rope, rope_byte), char_pos);
            assert_eq!(gap_byte_to_char(gap, gap_byte), char_pos);
            if char_pos < rope.metrics().chars_usize() {
                assert_eq!(
                    rope_char_code_at(rope, rope_byte),
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

    fn rope_byte_to_char(rope: &RopeTextBackend, byte_pos: usize) -> usize {
        rope.emacs_byte_pos_to_char_pos(EmacsBytePos::new(byte_pos))
            .get()
    }

    fn rope_char_to_byte(rope: &RopeTextBackend, char_pos: usize) -> usize {
        rope.char_pos_to_emacs_byte_pos(CharPos0::new(char_pos))
            .get()
    }

    fn rope_byte_at(rope: &RopeTextBackend, byte_pos: usize) -> u8 {
        rope.byte_at_emacs_byte_pos(EmacsBytePos::new(byte_pos))
    }

    fn rope_emacs_byte_at(rope: &RopeTextBackend, byte_pos: usize) -> Option<u8> {
        rope.emacs_byte_at_pos(EmacsBytePos::new(byte_pos))
    }

    fn rope_char_code_at(rope: &RopeTextBackend, byte_pos: usize) -> Option<u32> {
        rope.char_code_at_emacs_byte_pos(EmacsBytePos::new(byte_pos))
    }

    fn insert_rope_str(rope: &mut RopeTextBackend, byte_pos: usize, text: &str) {
        let bytes =
            crate::emacs_core::string_escape::storage_string_to_buffer_bytes(text, rope.multibyte);
        let extent = TextExtent::from_emacs_bytes(&bytes, rope.multibyte);
        rope.insert_measured_emacs_bytes(EmacsBytePos::new(byte_pos), &bytes, extent);
    }

    fn insert_rope_bytes_both(
        rope: &mut RopeTextBackend,
        byte_pos: usize,
        bytes: &[u8],
        nchars: usize,
    ) {
        rope.insert_measured_emacs_bytes(
            EmacsBytePos::new(byte_pos),
            bytes,
            TextExtent::new(CharLen::new(nchars), EmacsByteLen::new(bytes.len())),
        );
    }

    fn delete_rope_range_both(rope: &mut RopeTextBackend, start: usize, end: usize, nchars: usize) {
        let start_char = rope_byte_to_char(rope, start);
        rope.delete_measured_range(TextEditRange::from_usize(
            start,
            end,
            start_char,
            start_char + nchars,
        ));
    }

    fn replace_rope_same_len(
        rope: &mut RopeTextBackend,
        start: usize,
        end: usize,
        replacement: &[u8],
    ) {
        let range = byte_range(start, end);
        let edit_range = TextEditRange::new(
            range,
            rope.emacs_byte_pos_to_char_pos(range.start()),
            rope.emacs_byte_pos_to_char_pos(range.end()),
        );
        rope.replace_same_len_measured_range(
            TextReplacement::new(
                edit_range,
                TextExtent::from_emacs_bytes(replacement, rope.multibyte),
            ),
            replacement,
        );
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

    #[test]
    fn rope_reports_metrics_and_layout() {
        let backend = RopeTextBackend::from_str("éz");
        assert_eq!(
            backend.debug_layout(),
            TextBackendDebugLayout::Rope(TextMetrics::from_usize(2, 3))
        );
        assert_eq!(rope_char_to_byte(&backend, 1), "é".len());
        assert_eq!(rope_byte_to_char(&backend, "é".len()), 1);
    }

    #[test]
    fn rope_large_initial_text_uses_multiple_chunks_and_preserves_text() {
        let text = "a".repeat(MAX_LEAF_BYTES.get() * 2 + 17);
        let backend = RopeTextBackend::from_str(&text);
        backend.assert_invariants();
        let mut chunks = Vec::new();
        backend
            .for_each_emacs_byte_range_chunk(byte_range(0, backend.len()), |chunk| {
                chunks.push(chunk.len());
                Ok::<(), ()>(())
            })
            .unwrap();
        assert!(
            chunks.len() > 1,
            "large rope text should be represented by multiple chunks"
        );
        assert_eq!(backend.to_string(), text);
    }

    #[test]
    fn rope_coalesces_adjacent_small_chunks_after_edits() {
        let mut backend = RopeTextBackend::from_str("abcdef");
        insert_rope_str(&mut backend, 3, "XY");
        assert_eq!(backend.debug_chunk_lengths(), vec![8]);

        delete_rope_range_both(&mut backend, 3, 5, 2);
        assert_eq!(backend.to_string(), "abcdef");
        assert_eq!(backend.debug_chunk_lengths(), vec![6]);
        backend.assert_invariants();
    }

    #[test]
    fn rope_insert_delete_and_replace_match_gap_buffer() {
        let mut rope = RopeTextBackend::from_str("abécd日本");
        let mut gap = GapBuffer::from_str("abécd日本");
        assert_matches_gap(&rope, &gap);

        let pos = rope_char_to_byte(&rope, 2);
        insert_rope_str(&mut rope, pos, "XYZ");
        insert_gap_str(&mut gap, pos, "XYZ");
        assert_matches_gap(&rope, &gap);

        let start = rope_char_to_byte(&rope, 1);
        let end = rope_char_to_byte(&rope, 5);
        let nchars = rope_byte_to_char(&rope, end) - rope_byte_to_char(&rope, start);
        delete_rope_range_both(&mut rope, start, end, nchars);
        delete_gap_range_both(&mut gap, start, end, nchars);
        assert_matches_gap(&rope, &gap);

        let start = rope_char_to_byte(&rope, 1);
        let end = rope_char_to_byte(&rope, 2);
        replace_rope_same_len(&mut rope, start, end, "ß".as_bytes());
        replace_gap_same_len(&mut gap, start, end, "ß".as_bytes());
        assert_matches_gap(&rope, &gap);
    }

    #[test]
    fn rope_unibyte_raw_bytes_round_trip() {
        let raw = vec![0xFF, b'A', 0x80];
        let mut backend = RopeTextBackend::from_emacs_bytes(&raw, false);
        insert_rope_bytes_both(&mut backend, 1, &[b'\n'], 1);

        assert!(!backend.is_multibyte());
        assert_eq!(backend.metrics().chars_usize(), 4);
        assert_eq!(backend.metrics().emacs_bytes_usize(), 4);
        assert_eq!(rope_byte_to_char(&backend, 3), 3);
        assert_eq!(rope_char_to_byte(&backend, 4), 4);

        let mut bytes = Vec::new();
        backend.copy_emacs_byte_range_to(byte_range(0, backend.len()), &mut bytes);
        assert_eq!(bytes, vec![0xFF, b'\n', b'A', 0x80]);
    }

    proptest! {
        #[test]
        fn rope_random_edit_sequences_match_gap_buffer(
            ops in prop::collection::vec((0u8..3, 0usize..200, 0usize..200, 0u8..32), 0..80)
        ) {
            let mut rope = RopeTextBackend::from_str("abécd日本");
            let mut gap = GapBuffer::from_str("abécd日本");
            assert_matches_gap(&rope, &gap);

            for (kind, a, b, seed) in ops {
                match kind {
                    0 => {
                        let char_pos = a % (rope.metrics().chars_usize() + 1);
                        let byte_pos = rope_char_to_byte(&rope, char_pos);
                        let text = sample_insert(seed);
                        insert_rope_str(&mut rope, byte_pos, text);
                        insert_gap_str(&mut gap, byte_pos, text);
                    }
                    1 => {
                        if rope.metrics().chars_usize() > 0 {
                            let char_a = a % (rope.metrics().chars_usize() + 1);
                            let char_b = b % (rope.metrics().chars_usize() + 1);
                            let start_char = char_a.min(char_b);
                            let end_char = char_a.max(char_b);
                            let start = rope_char_to_byte(&rope, start_char);
                            let end = rope_char_to_byte(&rope, end_char);
                            let nchars = end_char - start_char;
                            delete_rope_range_both(&mut rope, start, end, nchars);
                            delete_gap_range_both(&mut gap, start, end, nchars);
                        }
                    }
                    _ => {
                        if rope.metrics().chars_usize() > 0 {
                            let char_pos = a % rope.metrics().chars_usize();
                            let start = rope_char_to_byte(&rope, char_pos);
                            let end = rope_char_to_byte(&rope, char_pos + 1);
                            if let Some(replacement) = replacement_bytes_for_len(end - start, seed) {
                                replace_rope_same_len(&mut rope, start, end, &replacement);
                                replace_gap_same_len(&mut gap, start, end, &replacement);
                            }
                        }
                    }
                }
                assert_matches_gap(&rope, &gap);
            }
        }
    }

    proptest! {
        #[test]
        fn rope_unibyte_random_edit_sequences_match_gap_buffer(
            ops in prop::collection::vec((0u8..3, 0usize..200, 0usize..200, any::<u8>()), 0..80)
        ) {
            let initial = vec![0xFF, b'A', 0x80, b'\n', b'Z'];
            let mut rope = RopeTextBackend::from_emacs_bytes(&initial, false);
            let mut gap = GapBuffer::from_emacs_bytes(&initial, false);
            assert_matches_gap(&rope, &gap);

            for (kind, a, b, seed) in ops {
                match kind {
                    0 => {
                        let byte_pos = a % (rope.len() + 1);
                        let bytes = sample_unibyte_insert(seed);
                        insert_rope_bytes_both(&mut rope, byte_pos, &bytes, bytes.len());
                        insert_gap_bytes_both(&mut gap, byte_pos, &bytes, bytes.len());
                    }
                    1 => {
                        if !rope.is_empty() {
                            let byte_a = a % (rope.len() + 1);
                            let byte_b = b % (rope.len() + 1);
                            let start = byte_a.min(byte_b);
                            let end = byte_a.max(byte_b);
                            delete_rope_range_both(&mut rope, start, end, end - start);
                            delete_gap_range_both(&mut gap, start, end, end - start);
                        }
                    }
                    _ => {
                        if !rope.is_empty() {
                            let start = a % rope.len();
                            let end = (start + 1 + (b % 4)).min(rope.len());
                            let replacement = vec![seed; end - start];
                            replace_rope_same_len(&mut rope, start, end, &replacement);
                            replace_gap_same_len(&mut gap, start, end, &replacement);
                        }
                    }
                }
                assert_matches_gap(&rope, &gap);
            }
        }
    }
}
