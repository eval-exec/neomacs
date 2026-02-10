//! Pure gap buffer data structure.
//!
//! Implements the core gap buffer operations used by Emacs for text storage:
//! - Gap movement (left/right)
//! - Gap resizing (enlarge/shrink)
//! - Text insertion and deletion at the gap
//! - Byte position ↔ address translation
//!
//! This module contains only the algorithmic core — no markers, intervals,
//! overlays, undo, or hooks. Those remain in the C caller.

use std::cmp::{max, min};

/// Default number of bytes to add when enlarging the gap.
pub const GAP_BYTES_DFL: usize = 2000;

/// Minimum gap size after compaction.
pub const GAP_BYTES_MIN: usize = 20;

/// Maximum buffer size (same as Emacs: ptrdiff_t max minus some safety margin).
pub const BUF_BYTES_MAX: usize = (isize::MAX as usize) / 2;

/// Error type for gap buffer operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GapError {
    /// Buffer would exceed maximum size.
    Overflow,
    /// Invalid position.
    InvalidPosition,
    /// Allocation failure.
    AllocFailed,
}

impl std::fmt::Display for GapError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GapError::Overflow => write!(f, "Buffer overflow"),
            GapError::InvalidPosition => write!(f, "Invalid position"),
            GapError::AllocFailed => write!(f, "Allocation failed"),
        }
    }
}

impl std::error::Error for GapError {}

/// A gap buffer for text storage.
///
/// The buffer stores text in a contiguous allocation with a "gap" —
/// a region of unused space that can be cheaply moved to any position.
/// Insertions at the gap position are O(1) (just write into the gap),
/// while insertions elsewhere require moving the gap first (O(N) in
/// the worst case, but amortized O(1) for sequential editing).
///
/// # Memory Layout
///
/// ```text
/// buf: [---text before gap---][===GAP===][---text after gap---]
///       ^                      ^          ^                     ^
///       0                  gpt_byte   gap_end              z_byte+gap_size
/// ```
///
/// Positions are 0-based byte offsets into the *logical* text
/// (i.e., as if the gap didn't exist).
pub struct GapBuffer {
    /// The underlying buffer. Text is stored with a gap.
    buf: Vec<u8>,
    /// Byte position of the gap start (0-based).
    gpt_byte: usize,
    /// Total byte size of the logical text (excluding the gap).
    z_byte: usize,
    /// Size of the gap in bytes.
    gap_size: usize,
}

impl GapBuffer {
    /// Create a new empty gap buffer with default gap size.
    pub fn new() -> Self {
        let gap_size = GAP_BYTES_DFL;
        let mut buf = vec![0u8; gap_size];
        buf[0] = 0; // Anchor byte
        GapBuffer {
            buf,
            gpt_byte: 0,
            z_byte: 0,
            gap_size,
        }
    }

    /// Create a gap buffer from initial content.
    pub fn from_bytes(content: &[u8]) -> Self {
        let gap_size = GAP_BYTES_DFL;
        let total = content.len() + gap_size;
        let mut buf = vec![0u8; total];

        // Text goes before the gap
        buf[..content.len()].copy_from_slice(content);
        // Gap is at the end
        GapBuffer {
            buf,
            gpt_byte: content.len(),
            z_byte: content.len(),
            gap_size,
        }
    }

    /// Return the total number of bytes of text in the buffer.
    pub fn len(&self) -> usize {
        self.z_byte
    }

    /// Return true if the buffer contains no text.
    pub fn is_empty(&self) -> bool {
        self.z_byte == 0
    }

    /// Return the current gap position (byte offset in logical text).
    pub fn gap_position(&self) -> usize {
        self.gpt_byte
    }

    /// Return the current gap size in bytes.
    pub fn gap_size(&self) -> usize {
        self.gap_size
    }

    /// Return the byte at the given logical position.
    /// Returns None if out of bounds.
    pub fn byte_at(&self, pos: usize) -> Option<u8> {
        if pos >= self.z_byte {
            return None;
        }
        let physical = self.logical_to_physical(pos);
        Some(self.buf[physical])
    }

    /// Convert a logical byte position to a physical buffer index.
    #[inline]
    fn logical_to_physical(&self, pos: usize) -> usize {
        if pos < self.gpt_byte {
            pos
        } else {
            pos + self.gap_size
        }
    }

    /// Return the physical address of the gap start.
    #[inline]
    fn gap_addr(&self) -> usize {
        self.gpt_byte
    }

    /// Return the physical address just past the gap end.
    #[inline]
    fn gap_end_addr(&self) -> usize {
        self.gpt_byte + self.gap_size
    }

    // ===== Gap movement =====

    /// Move the gap to the given byte position.
    pub fn move_gap(&mut self, bytepos: usize) {
        debug_assert!(bytepos <= self.z_byte);
        if bytepos < self.gpt_byte {
            self.gap_left(bytepos);
        } else if bytepos > self.gpt_byte {
            self.gap_right(bytepos);
        }
    }

    /// Move the gap leftward (toward lower addresses) to `bytepos`.
    fn gap_left(&mut self, bytepos: usize) {
        debug_assert!(bytepos <= self.gpt_byte);

        let bytes_to_move = self.gpt_byte - bytepos;
        if bytes_to_move == 0 {
            return;
        }

        // Source: text before gap, from bytepos to gpt_byte
        // Destination: just before gap end (shift text right by gap_size)
        // This is equivalent to: move_gap ← shift text that was before gap to after gap
        let src = bytepos;
        let dst = bytepos + self.gap_size;

        // Use memmove semantics (handles overlap)
        self.buf.copy_within(src..src + bytes_to_move, dst);

        self.gpt_byte = bytepos;

        // Place anchor byte at new gap start
        if self.gpt_byte < self.buf.len() {
            self.buf[self.gpt_byte] = 0;
        }
    }

    /// Move the gap rightward (toward higher addresses) to `bytepos`.
    fn gap_right(&mut self, bytepos: usize) {
        debug_assert!(bytepos >= self.gpt_byte);
        debug_assert!(bytepos <= self.z_byte);

        let bytes_to_move = bytepos - self.gpt_byte;
        if bytes_to_move == 0 {
            return;
        }

        // Source: text after gap, from gap_end to gap_end + bytes_to_move
        // Destination: gap start (shift text left by gap_size)
        let src = self.gpt_byte + self.gap_size;
        let dst = self.gpt_byte;

        self.buf.copy_within(src..src + bytes_to_move, dst);

        self.gpt_byte = bytepos;

        // Place anchor byte
        if self.gpt_byte < self.buf.len() {
            self.buf[self.gpt_byte] = 0;
        }
    }

    // ===== Gap resizing =====

    /// Ensure the gap is at least `needed` bytes.
    /// If the gap is already large enough, does nothing.
    pub fn ensure_gap(&mut self, needed: usize) -> Result<(), GapError> {
        if self.gap_size >= needed {
            return Ok(());
        }
        self.make_gap_larger(needed - self.gap_size)
    }

    /// Enlarge the gap by `nbytes_added` bytes.
    pub fn make_gap_larger(&mut self, nbytes_added: usize) -> Result<(), GapError> {
        let current_size = self.z_byte;
        if BUF_BYTES_MAX - current_size < nbytes_added {
            return Err(GapError::Overflow);
        }

        // Auto-size up
        let actual_add = min(
            nbytes_added + GAP_BYTES_DFL,
            BUF_BYTES_MAX - current_size,
        );

        let new_total = self.buf.len() + actual_add;
        self.buf.resize(new_total, 0);

        // Strategy: create a "fake" gap at the end, then move it to the real gap position
        let old_gpt = self.gpt_byte;
        let old_gap_size = self.gap_size;

        // Fake gap starts at the end of old allocation
        // Move text after the old gap to make room
        // Actually, since we resized, we just need to move the text-after-gap
        // to the end of the new buffer.

        // Text after gap: from old_gpt + old_gap_size .. old_buf_len
        let after_gap_start = old_gpt + old_gap_size;
        let after_gap_len = self.z_byte - old_gpt;

        if after_gap_len > 0 {
            // Move text-after-gap to the end of the new buffer
            let new_after_start = old_gpt + old_gap_size + actual_add;
            self.buf.copy_within(after_gap_start..after_gap_start + after_gap_len, new_after_start);
        }

        self.gap_size = old_gap_size + actual_add;

        // Place anchor at end
        let z_addr = self.gpt_byte + self.gap_size + (self.z_byte - self.gpt_byte);
        if z_addr < self.buf.len() {
            self.buf[z_addr] = 0;
        }

        Ok(())
    }

    /// Shrink the gap by `nbytes_removed` bytes, freeing memory.
    pub fn make_gap_smaller(&mut self, mut nbytes_removed: usize) -> Result<(), GapError> {
        if self.gap_size <= GAP_BYTES_MIN {
            return Ok(()); // Already at minimum
        }

        // Don't shrink below minimum
        if self.gap_size - nbytes_removed < GAP_BYTES_MIN {
            nbytes_removed = self.gap_size - GAP_BYTES_MIN;
        }

        if nbytes_removed == 0 {
            return Ok(());
        }

        // Move text-after-gap closer to gap
        let after_gap_start = self.gpt_byte + self.gap_size;
        let after_gap_len = self.z_byte - self.gpt_byte;

        if after_gap_len > 0 {
            let new_after_start = after_gap_start - nbytes_removed;
            self.buf.copy_within(after_gap_start..after_gap_start + after_gap_len, new_after_start);
        }

        self.gap_size -= nbytes_removed;

        // Truncate buffer
        let new_total = self.z_byte + self.gap_size;
        self.buf.truncate(new_total);

        // Place anchor
        let z_addr = self.gpt_byte + self.gap_size + (self.z_byte - self.gpt_byte);
        if z_addr < self.buf.len() {
            self.buf[z_addr] = 0;
        }

        Ok(())
    }

    // ===== Text operations =====

    /// Insert bytes at the current gap position.
    /// The gap must be at the desired insertion point (call `move_gap` first).
    /// The gap must be large enough (call `ensure_gap` first).
    pub fn insert_at_gap(&mut self, data: &[u8]) -> Result<(), GapError> {
        let nbytes = data.len();
        if nbytes == 0 {
            return Ok(());
        }

        if self.gap_size < nbytes {
            self.ensure_gap(nbytes)?;
        }

        // Copy data into the gap
        let dst = self.gpt_byte;
        self.buf[dst..dst + nbytes].copy_from_slice(data);

        // Advance gap past inserted text
        self.gpt_byte += nbytes;
        self.gap_size -= nbytes;
        self.z_byte += nbytes;

        Ok(())
    }

    /// Insert bytes at a specific byte position.
    /// Moves the gap to `pos` first if necessary.
    pub fn insert_at(&mut self, pos: usize, data: &[u8]) -> Result<(), GapError> {
        if pos > self.z_byte {
            return Err(GapError::InvalidPosition);
        }
        self.move_gap(pos);
        self.insert_at_gap(data)
    }

    /// Delete `nbytes` bytes starting at byte position `from`.
    /// Returns the deleted bytes.
    pub fn delete(&mut self, from: usize, nbytes: usize) -> Result<Vec<u8>, GapError> {
        if from + nbytes > self.z_byte {
            return Err(GapError::InvalidPosition);
        }

        // Save deleted text
        let mut deleted = Vec::with_capacity(nbytes);
        for i in 0..nbytes {
            deleted.push(self.byte_at(from + i).unwrap());
        }

        // Move gap to encompass the deleted region
        if from > self.gpt_byte {
            self.gap_right(from);
        } else if from < self.gpt_byte {
            self.gap_left(from);
        }
        // Now gap is at `from`

        // Expand gap over the deleted region
        self.gap_size += nbytes;
        self.z_byte -= nbytes;

        Ok(deleted)
    }

    /// Delete `nbytes` bytes at `from` without returning the deleted text.
    pub fn delete_no_return(&mut self, from: usize, nbytes: usize) -> Result<(), GapError> {
        if from + nbytes > self.z_byte {
            return Err(GapError::InvalidPosition);
        }

        if from > self.gpt_byte {
            self.gap_right(from);
        } else if from < self.gpt_byte {
            self.gap_left(from);
        }

        self.gap_size += nbytes;
        self.z_byte -= nbytes;

        Ok(())
    }

    // ===== Text access =====

    /// Copy the entire buffer contents (without gap) into a new Vec.
    pub fn contents(&self) -> Vec<u8> {
        let mut result = Vec::with_capacity(self.z_byte);
        // Text before gap
        result.extend_from_slice(&self.buf[..self.gpt_byte]);
        // Text after gap
        let after_start = self.gpt_byte + self.gap_size;
        let after_end = after_start + (self.z_byte - self.gpt_byte);
        result.extend_from_slice(&self.buf[after_start..after_end]);
        result
    }

    /// Copy a range of bytes [from, from+len) from the logical text.
    pub fn slice(&self, from: usize, len: usize) -> Vec<u8> {
        debug_assert!(from + len <= self.z_byte);
        let mut result = Vec::with_capacity(len);

        if from + len <= self.gpt_byte {
            // Entirely before gap
            result.extend_from_slice(&self.buf[from..from + len]);
        } else if from >= self.gpt_byte {
            // Entirely after gap
            let physical = from + self.gap_size;
            result.extend_from_slice(&self.buf[physical..physical + len]);
        } else {
            // Spans the gap
            let before_len = self.gpt_byte - from;
            result.extend_from_slice(&self.buf[from..self.gpt_byte]);
            let after_start = self.gap_end_addr();
            let after_len = len - before_len;
            result.extend_from_slice(&self.buf[after_start..after_start + after_len]);
        }

        result
    }

    /// Get a reference to the text before the gap.
    pub fn before_gap(&self) -> &[u8] {
        &self.buf[..self.gpt_byte]
    }

    /// Get a reference to the text after the gap.
    pub fn after_gap(&self) -> &[u8] {
        let start = self.gap_end_addr();
        let end = start + (self.z_byte - self.gpt_byte);
        &self.buf[start..end]
    }

    /// Replace bytes at [from, from+old_len) with `new_data`.
    pub fn replace(&mut self, from: usize, old_len: usize, new_data: &[u8]) -> Result<(), GapError> {
        if from + old_len > self.z_byte {
            return Err(GapError::InvalidPosition);
        }

        // Delete old text
        self.delete_no_return(from, old_len)?;

        // Insert new text
        self.insert_at(from, new_data)?;

        Ok(())
    }
}

impl Default for GapBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for GapBuffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GapBuffer")
            .field("len", &self.z_byte)
            .field("gap_pos", &self.gpt_byte)
            .field("gap_size", &self.gap_size)
            .field("buf_size", &self.buf.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_empty() {
        let buf = GapBuffer::new();
        assert!(buf.is_empty());
        assert_eq!(buf.len(), 0);
        assert_eq!(buf.gap_size(), GAP_BYTES_DFL);
    }

    #[test]
    fn test_from_bytes() {
        let buf = GapBuffer::from_bytes(b"hello");
        assert_eq!(buf.len(), 5);
        assert_eq!(buf.contents(), b"hello");
    }

    #[test]
    fn test_insert_at_beginning() {
        let mut buf = GapBuffer::new();
        buf.insert_at(0, b"hello").unwrap();
        assert_eq!(buf.len(), 5);
        assert_eq!(buf.contents(), b"hello");
    }

    #[test]
    fn test_insert_at_end() {
        let mut buf = GapBuffer::from_bytes(b"hello");
        buf.insert_at(5, b" world").unwrap();
        assert_eq!(buf.contents(), b"hello world");
    }

    #[test]
    fn test_insert_in_middle() {
        let mut buf = GapBuffer::from_bytes(b"helo");
        buf.insert_at(2, b"l").unwrap();
        assert_eq!(buf.contents(), b"hello");
    }

    #[test]
    fn test_delete() {
        let mut buf = GapBuffer::from_bytes(b"hello world");
        let deleted = buf.delete(5, 6).unwrap();
        assert_eq!(deleted, b" world");
        assert_eq!(buf.contents(), b"hello");
    }

    #[test]
    fn test_delete_at_beginning() {
        let mut buf = GapBuffer::from_bytes(b"hello");
        buf.delete_no_return(0, 2).unwrap();
        assert_eq!(buf.contents(), b"llo");
    }

    #[test]
    fn test_delete_at_end() {
        let mut buf = GapBuffer::from_bytes(b"hello");
        buf.delete_no_return(3, 2).unwrap();
        assert_eq!(buf.contents(), b"hel");
    }

    #[test]
    fn test_multiple_insertions() {
        let mut buf = GapBuffer::new();
        buf.insert_at(0, b"world").unwrap();
        buf.insert_at(0, b"hello ").unwrap();
        assert_eq!(buf.contents(), b"hello world");
    }

    #[test]
    fn test_sequential_typing() {
        let mut buf = GapBuffer::new();
        for &ch in b"hello" {
            let pos = buf.len();
            buf.insert_at(pos, &[ch]).unwrap();
        }
        assert_eq!(buf.contents(), b"hello");
    }

    #[test]
    fn test_byte_at() {
        let buf = GapBuffer::from_bytes(b"hello");
        assert_eq!(buf.byte_at(0), Some(b'h'));
        assert_eq!(buf.byte_at(4), Some(b'o'));
        assert_eq!(buf.byte_at(5), None);
    }

    #[test]
    fn test_byte_at_after_gap_move() {
        let mut buf = GapBuffer::from_bytes(b"hello world");
        buf.move_gap(5);
        // Gap is at position 5, text is "hello[gap] world"
        assert_eq!(buf.byte_at(0), Some(b'h'));
        assert_eq!(buf.byte_at(5), Some(b' '));
        assert_eq!(buf.byte_at(10), Some(b'd'));
    }

    #[test]
    fn test_slice() {
        let buf = GapBuffer::from_bytes(b"hello world");
        assert_eq!(buf.slice(0, 5), b"hello");
        assert_eq!(buf.slice(6, 5), b"world");
    }

    #[test]
    fn test_slice_across_gap() {
        let mut buf = GapBuffer::from_bytes(b"hello world");
        buf.move_gap(5);
        // Slice that spans the gap
        assert_eq!(buf.slice(3, 5), b"lo wo");
    }

    #[test]
    fn test_before_after_gap() {
        let mut buf = GapBuffer::from_bytes(b"hello world");
        buf.move_gap(5);
        assert_eq!(buf.before_gap(), b"hello");
        assert_eq!(buf.after_gap(), b" world");
    }

    #[test]
    fn test_replace() {
        let mut buf = GapBuffer::from_bytes(b"hello world");
        buf.replace(6, 5, b"rust").unwrap();
        assert_eq!(buf.contents(), b"hello rust");
    }

    #[test]
    fn test_gap_enlargement() {
        let mut buf = GapBuffer::new();
        // Insert more than the default gap size
        let big_text = vec![b'x'; GAP_BYTES_DFL + 100];
        buf.insert_at(0, &big_text).unwrap();
        assert_eq!(buf.len(), big_text.len());
        assert_eq!(buf.contents(), big_text);
    }

    #[test]
    fn test_gap_shrink() {
        let mut buf = GapBuffer::from_bytes(b"hello");
        // Gap should be GAP_BYTES_DFL
        assert_eq!(buf.gap_size(), GAP_BYTES_DFL);
        buf.make_gap_smaller(GAP_BYTES_DFL - GAP_BYTES_MIN).unwrap();
        assert_eq!(buf.gap_size(), GAP_BYTES_MIN);
        // Text should be preserved
        assert_eq!(buf.contents(), b"hello");
    }

    #[test]
    fn test_mixed_operations() {
        let mut buf = GapBuffer::new();
        buf.insert_at(0, b"The quick brown fox").unwrap();
        buf.delete_no_return(4, 6).unwrap(); // Remove "quick "
        assert_eq!(buf.contents(), b"The brown fox");
        buf.insert_at(4, b"lazy ").unwrap();
        assert_eq!(buf.contents(), b"The lazy brown fox");
        buf.replace(9, 5, b"red").unwrap();
        assert_eq!(buf.contents(), b"The lazy red fox");
    }

    #[test]
    fn test_empty_operations() {
        let mut buf = GapBuffer::new();
        buf.insert_at(0, b"").unwrap();
        assert!(buf.is_empty());
    }

    #[test]
    fn test_delete_all() {
        let mut buf = GapBuffer::from_bytes(b"hello");
        buf.delete_no_return(0, 5).unwrap();
        assert!(buf.is_empty());
        assert_eq!(buf.contents(), b"");
    }

    #[test]
    fn test_invalid_position() {
        let mut buf = GapBuffer::from_bytes(b"hello");
        assert!(buf.insert_at(10, b"x").is_err());
        assert!(buf.delete(3, 5).is_err());
    }

    #[test]
    fn test_large_sequential_insert() {
        let mut buf = GapBuffer::new();
        let n = 10000;
        for i in 0..n {
            buf.insert_at(buf.len(), &[b'a' + (i % 26) as u8]).unwrap();
        }
        assert_eq!(buf.len(), n);
        let contents = buf.contents();
        for i in 0..n {
            assert_eq!(contents[i], b'a' + (i % 26) as u8);
        }
    }

    #[test]
    fn test_random_position_inserts() {
        let mut buf = GapBuffer::new();
        buf.insert_at(0, b"0123456789").unwrap();

        // Insert at various positions
        buf.insert_at(5, b"X").unwrap();
        assert_eq!(buf.contents(), b"01234X56789");

        buf.insert_at(0, b"Y").unwrap();
        assert_eq!(buf.contents(), b"Y01234X56789");

        buf.insert_at(12, b"Z").unwrap();
        assert_eq!(buf.contents(), b"Y01234X56789Z");
    }

    #[test]
    fn test_move_gap_idempotent() {
        let mut buf = GapBuffer::from_bytes(b"hello world");
        buf.move_gap(5);
        let c1 = buf.contents();
        buf.move_gap(5);
        let c2 = buf.contents();
        assert_eq!(c1, c2);
    }

    #[test]
    fn test_move_gap_all_positions() {
        let text = b"abcdefghij";
        let mut buf = GapBuffer::from_bytes(text);

        for pos in 0..=text.len() {
            buf.move_gap(pos);
            assert_eq!(buf.contents(), text);
            assert_eq!(buf.gap_position(), pos);
        }
    }
}
