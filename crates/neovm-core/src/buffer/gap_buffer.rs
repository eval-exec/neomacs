//! A raw Emacs-byte gap buffer for efficient text editing.
//!
//! The gap buffer stores text in a contiguous `Vec<u8>` with a movable "gap"
//! (unused region) that makes insertions and deletions near the gap O(1)
//! amortized. The gap is relocated to the edit site before each mutation so
//! that sequential edits in the same neighborhood avoid large copies.
//!
//! Raw internal helpers use byte positions into the logical text (i.e. the
//! text with the gap removed). Cross-module entrypoints use typed
//! `EmacsBytePos`/`EmacsByteRange` plus measured edit types, so callers cannot
//! accidentally mix Emacs-byte and character coordinates at the backend
//! boundary. The underlying bytes are Emacs internal bytes, not
//! sentinel-encoded Rust strings.

use std::cell::Cell;
use std::fmt;

use crate::buffer::text::{GapCompatState, emacs_char_count_bytes, emacs_char_to_byte_in_slice};
use crate::buffer::{
    CharLen, CharPos0, EmacsBytePos, EmacsByteRange, TextEditRange, TextExtent, TextPositionAnchor,
    TextReplacement,
};

/// Default extra gap bytes to pre-allocate on any growth.
/// Matches GNU Emacs `GAP_BYTES_DFL` (`src/buffer.h:205`).
pub(crate) const GAP_BYTES_DFL: usize = 2000;

/// Floor for the gap after shrinking — not enforced today because we don't
/// shrink yet, but kept as a named constant to match GNU's `GAP_BYTES_MIN`
/// (`src/buffer.h:210`).
#[allow(dead_code)]
pub(crate) const GAP_BYTES_MIN: usize = 20;

/// A gap buffer holding raw Emacs bytes.
///
/// Internally the backing store looks like:
///
/// ```text
///  [ text-before-gap | gap (unused) | text-after-gap ]
///    0..gap_start      gap_start..gap_end  gap_end..buf.len()
/// ```
///
/// The *logical* text is the concatenation of `buf[..gap_start]` and
/// `buf[gap_end..]`.
#[derive(Clone)]
pub struct GapBuffer {
    /// Raw backing store.
    buf: Vec<u8>,
    /// Whether the logical text should be interpreted as a multibyte buffer.
    multibyte: bool,
    /// Byte index where the gap begins (first unused byte).
    gap_start: usize,
    /// Byte index one past the last gap byte (first byte of text after gap).
    gap_end: usize,
    /// Number of logical Emacs characters before the gap.
    gap_start_chars: usize,
    /// Number of logical Emacs characters in the buffer.
    total_chars: usize,
    /// Number of logical Emacs bytes before the gap.
    gap_start_bytes: usize,
    /// Number of logical Emacs bytes in the buffer.
    total_bytes: usize,
    /// Ring of recently converted `(logical char position, logical byte
    /// position)` correspondences, used as extra anchors so a conversion scans
    /// O(distance to the nearest recent conversion) instead of O(distance to
    /// the gap).  GNU `marker.c` gets the same effect from every marker in
    /// the buffer plus `cached_bytepos`/`cached_charpos`.  Edits ADJUST the
    /// ring like markers (before: keep; after: shift; inside: drop) instead of
    /// clearing it -- a single slot zeroed on every edit left comment/uncomment
    /// loops with only the gap as an anchor, and the regexp search had parked
    /// the gap ~1K bytes away, so each line edit re-counted ~1K bytes three
    /// times (58% of its gap-move cost).
    position_anchors: Cell<[TextPositionAnchor; POSITION_ANCHOR_RING]>,
    /// Next ring slot to overwrite.
    anchor_next: Cell<usize>,
}

/// Recent-conversion anchors kept per gap buffer (see `position_anchors`).
const POSITION_ANCHOR_RING: usize = 4;

// ---------------------------------------------------------------------------
// Construction
// ---------------------------------------------------------------------------

impl GapBuffer {
    /// Create an empty gap buffer with a default-sized gap.
    pub fn new() -> Self {
        Self::new_with_multibyte(true)
    }

    pub fn new_with_multibyte(multibyte: bool) -> Self {
        Self {
            buf: vec![0u8; GAP_BYTES_MIN],
            multibyte,
            gap_start: 0,
            gap_end: GAP_BYTES_MIN,
            gap_start_chars: 0,
            total_chars: 0,
            gap_start_bytes: 0,
            total_bytes: 0,
            position_anchors: Cell::new([TextPositionAnchor::ZERO; POSITION_ANCHOR_RING]),
            anchor_next: Cell::new(0),
        }
    }

    /// Create a gap buffer pre-loaded with raw Emacs bytes.
    pub fn from_emacs_bytes(text: &[u8], multibyte: bool) -> Self {
        let gap = GAP_BYTES_DFL;
        let char_count = emacs_char_count_bytes(text, multibyte).get();
        let byte_count = text.len();
        let mut buf = Vec::with_capacity(text.len() + gap);
        buf.extend_from_slice(text);
        buf.resize(text.len() + gap, 0);
        Self {
            buf,
            multibyte,
            gap_start: text.len(),
            gap_end: text.len() + gap,
            gap_start_chars: char_count,
            total_chars: char_count,
            gap_start_bytes: byte_count,
            total_bytes: byte_count,
            position_anchors: Cell::new([TextPositionAnchor::ZERO; POSITION_ANCHOR_RING]),
            anchor_next: Cell::new(0),
        }
    }

    pub(in crate::buffer) fn from_emacs_bytes_with_gap_compat_state(
        text: &[u8],
        multibyte: bool,
        gap_state: GapCompatState,
    ) -> Self {
        let total_chars = emacs_char_count_bytes(text, multibyte).get();
        let gap_start_chars = gap_state.pos().get();
        assert!(
            gap_start_chars <= total_chars,
            "from_emacs_bytes_with_gap_compat_state: gap char position {gap_start_chars} out of range ({total_chars})",
        );
        let gap_start_bytes = emacs_char_to_byte_in_slice(text, gap_start_chars, multibyte);
        let gap = gap_state.byte_len().get();
        let mut buf = Vec::with_capacity(text.len() + gap);
        buf.extend_from_slice(&text[..gap_start_bytes]);
        buf.resize(gap_start_bytes + gap, 0);
        buf.extend_from_slice(&text[gap_start_bytes..]);
        Self {
            buf,
            multibyte,
            gap_start: gap_start_bytes,
            gap_end: gap_start_bytes + gap,
            gap_start_chars,
            total_chars,
            gap_start_bytes,
            total_bytes: text.len(),
            position_anchors: Cell::new([TextPositionAnchor::ZERO; POSITION_ANCHOR_RING]),
            anchor_next: Cell::new(0),
        }
    }

    /// Create a gap buffer pre-loaded with the contents of `s`.
    pub fn from_str(s: &str) -> Self {
        let decoded = super::text::storage_string_to_emacs_buffer_bytes(s);
        Self::from_emacs_bytes(decoded.bytes(), decoded.multibyte())
    }

    // -----------------------------------------------------------------------
    // Queries
    // -----------------------------------------------------------------------

    /// Total length of the logical text in **bytes** (excluding the gap).
    #[inline]
    pub fn len(&self) -> usize {
        self.buf.len() - self.gap_size()
    }

    /// Whether the buffer contains no text.
    #[inline]
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn is_multibyte(&self) -> bool {
        self.multibyte
    }

    pub fn set_multibyte(&mut self, multibyte: bool) {
        if self.multibyte == multibyte {
            return;
        }
        self.multibyte = multibyte;
        let mut logical = Vec::with_capacity(self.len());
        self.copy_emacs_byte_range_to(
            EmacsByteRange::new(EmacsBytePos::ZERO, EmacsBytePos::new(self.len())),
            &mut logical,
        );
        self.gap_start_chars =
            emacs_char_count_bytes(&logical[..self.gap_start], self.multibyte).get();
        self.total_chars = emacs_char_count_bytes(&logical, self.multibyte).get();
        self.gap_start_bytes = self.gap_start;
        self.total_bytes = logical.len();
        self.reset_position_anchors();
    }

    /// Number of logical Emacs characters in the buffer storage.
    pub fn char_count(&self) -> usize {
        self.total_chars
    }

    /// Number of logical Emacs bytes in the buffer.
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub fn emacs_byte_len(&self) -> usize {
        self.total_bytes
    }

    /// GNU `GPT`: character position of the gap.
    pub fn gpt(&self) -> usize {
        self.gap_start_chars
    }

    /// GNU `Z`: character position of the end of buffer text.
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub fn z(&self) -> usize {
        self.total_chars
    }

    /// GNU `GPT_BYTE`: logical Emacs byte position of the gap.
    pub fn gpt_byte(&self) -> usize {
        self.gap_start_bytes
    }

    /// GNU `Z_BYTE`: logical Emacs byte position of the end of buffer text.
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub fn z_byte(&self) -> usize {
        self.total_bytes
    }

    /// Size of the gap in bytes.
    #[inline]
    pub fn gap_size(&self) -> usize {
        self.gap_end - self.gap_start
    }

    // -----------------------------------------------------------------------
    // Single-element access
    // -----------------------------------------------------------------------

    /// Return the byte at logical position `pos`.
    ///
    /// # Panics
    ///
    /// Panics if `pos >= self.len()`.
    fn byte_at(&self, pos: usize) -> u8 {
        assert!(
            pos < self.len(),
            "byte_at: position {pos} out of range (len {})",
            self.len()
        );
        if pos < self.gap_start {
            self.buf[pos]
        } else {
            self.buf[pos + self.gap_size()]
        }
    }

    pub(crate) fn byte_at_emacs_byte_pos(&self, pos: EmacsBytePos) -> u8 {
        self.byte_at(pos.get())
    }

    /// Return the logical Emacs byte at `pos`, or `None` if out of range.
    fn emacs_byte_at(&self, pos: usize) -> Option<u8> {
        (pos < self.total_bytes).then(|| self.byte_at(pos))
    }

    pub(crate) fn emacs_byte_at_pos(&self, pos: EmacsBytePos) -> Option<u8> {
        self.emacs_byte_at(pos.get())
    }

    pub(crate) fn char_at_emacs_byte_pos(&self, pos: EmacsBytePos) -> Option<char> {
        self.char_code_at_emacs_byte_pos(pos)
            .and_then(char::from_u32)
    }

    /// Return the Emacs character code whose first byte begins at logical
    /// byte position `pos`, or `None` if `pos >= self.len()`.
    fn char_code_at(&self, pos: usize) -> Option<u32> {
        if pos >= self.len() {
            return None;
        }
        debug_assert!(
            self.is_char_boundary(pos),
            "char_code_at: byte position {pos} is not a character boundary"
        );
        if !self.multibyte {
            return Some(self.byte_at(pos) as u32);
        }
        // The gap is always kept at a character boundary, so a character never
        // straddles it: its bytes are contiguous in one physical segment. Decode
        // directly from that slice -- like GNU `FETCH_MULTIBYTE_CHAR` reading from
        // `BUF_BYTE_ADDRESS` -- instead of copying up to MAX_MULTIBYTE_LENGTH bytes
        // one at a time through the gap-mapped `byte_at` (which, for the common
        // ASCII-in-a-multibyte-buffer character, copied 5 bytes to decode 1).
        let (phys_start, phys_end) = if pos < self.gap_start {
            (pos, self.gap_start)
        } else {
            (pos + self.gap_size(), self.buf.len())
        };
        Some(crate::emacs_core::emacs_char::string_char(&self.buf[phys_start..phys_end]).0)
    }

    pub(crate) fn char_code_at_emacs_byte_pos(&self, pos: EmacsBytePos) -> Option<u32> {
        self.char_code_at(pos.get())
    }

    /// The contiguous physical window containing logical byte `pos`: the
    /// gap half `pos` falls in, as `(logical_start, base_ptr, len)` where
    /// logical byte `logical_start + i` lives at `base_ptr.add(i)`.
    ///
    /// The pointer is valid until the next text mutation (anything that
    /// moves the gap or reallocates `buf`). Callers hold it only across
    /// pure-read scans — the same lifetime discipline GNU's scanners apply
    /// to `BYTE_POS_ADDR` pointers.
    pub(crate) fn contiguous_window_at(&self, pos: usize) -> Option<(usize, *const u8, usize)> {
        if pos >= self.len() {
            return None;
        }
        if pos < self.gap_start {
            Some((0, self.buf.as_ptr(), self.gap_start))
        } else {
            // SAFETY: gap_end <= buf.len() by construction.
            let base = unsafe { self.buf.as_ptr().add(self.gap_end) };
            Some((self.gap_start, base, self.len() - self.gap_start))
        }
    }

    // -----------------------------------------------------------------------
    // Range extraction
    // -----------------------------------------------------------------------

    pub(crate) fn text_emacs_byte_range(&self, range: EmacsByteRange) -> String {
        let start = range.start().get();
        let end = range.end().get();
        assert!(start <= end, "text_range: start ({start}) > end ({end})");
        assert!(
            end <= self.len(),
            "text_range: end ({end}) > len ({})",
            self.len()
        );
        if start == end {
            return String::new();
        }
        let mut out = Vec::with_capacity(end - start);
        self.copy_emacs_byte_range_to(range, &mut out);
        crate::emacs_core::emacs_char::emacs_bytes_to_lossy_string(&out, self.multibyte)
    }

    pub(crate) fn copy_emacs_byte_range_to(&self, range: EmacsByteRange, out: &mut Vec<u8>) {
        let start = range.start().get();
        let end = range.end().get();
        assert!(
            start <= end,
            "copy_emacs_bytes_to: start ({start}) > end ({end})"
        );
        assert!(
            end <= self.total_bytes,
            "copy_emacs_bytes_to: end ({end}) > emacs len ({})",
            self.total_bytes
        );
        out.clear();
        if start == end {
            return;
        }
        // One spare slot beyond the copied range: these bytes usually become a
        // LispString payload, whose constructor appends a trailing NUL — an
        // exact-capacity Vec would realloc and re-copy there.
        out.reserve(end - start + 1);

        // Intersection with segment A (logical 0..gap_start).
        if start < self.gap_start {
            let seg_end = end.min(self.gap_start);
            out.extend_from_slice(&self.buf[start..seg_end]);
        }

        // Intersection with segment B (logical gap_start..len).
        if end > self.gap_start {
            let seg_start = start.max(self.gap_start);
            let phys_start = seg_start + self.gap_size();
            let phys_end = end + self.gap_size();
            out.extend_from_slice(&self.buf[phys_start..phys_end]);
        }
    }

    pub(crate) fn for_each_emacs_byte_range_chunk<E>(
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
            end <= self.total_bytes,
            "for_each_emacs_byte_chunk: end ({end}) > emacs len ({})",
            self.total_bytes
        );
        if start == end {
            return Ok(());
        }
        if end <= self.gap_start_bytes {
            return f(&self.buf[start..end]);
        }
        if start >= self.gap_start_bytes {
            let gap = self.gap_size();
            return f(&self.buf[start + gap..end + gap]);
        }

        f(&self.buf[start..self.gap_start])?;
        let gap = self.gap_size();
        f(&self.buf[self.gap_start + gap..end + gap])
    }

    pub(crate) fn has_contiguous_emacs_byte_range(&self, range: EmacsByteRange) -> bool {
        let start = range.start().get();
        let end = range.end().get();
        assert!(
            start <= end,
            "has_contiguous_emacs_bytes: start ({start}) > end ({end})"
        );
        assert!(
            end <= self.total_bytes,
            "has_contiguous_emacs_bytes: end ({end}) > emacs len ({})",
            self.total_bytes
        );
        start == end || end <= self.gap_start_bytes || start >= self.gap_start_bytes
    }

    pub(crate) fn with_contiguous_emacs_byte_range<R>(
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
            end <= self.total_bytes,
            "with_contiguous_emacs_bytes: end ({end}) > emacs len ({})",
            self.total_bytes
        );
        if start == end {
            return Some(f(&[]));
        }
        if end <= self.gap_start_bytes {
            return Some(f(&self.buf[start..end]));
        }
        if start >= self.gap_start_bytes {
            let gap = self.gap_size();
            return Some(f(&self.buf[start + gap..end + gap]));
        }
        None
    }

    // -----------------------------------------------------------------------
    // Mutation
    // -----------------------------------------------------------------------

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(crate) fn insert_emacs_bytes_at_emacs_byte_pos(&mut self, pos: EmacsBytePos, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        self.insert_measured_emacs_bytes(
            pos,
            bytes,
            TextExtent::from_emacs_bytes(bytes, self.multibyte),
        );
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(crate) fn insert_emacs_bytes_at_emacs_byte_pos_with_char_len(
        &mut self,
        pos: EmacsBytePos,
        bytes: &[u8],
        char_len: CharLen,
    ) {
        self.insert_measured_emacs_bytes(
            pos,
            bytes,
            TextExtent::new(char_len, crate::buffer::EmacsByteLen::new(bytes.len())),
        );
    }

    pub(crate) fn insert_measured_emacs_bytes(
        &mut self,
        pos: EmacsBytePos,
        bytes: &[u8],
        extent: TextExtent,
    ) {
        let pos = pos.get();
        let nchars = extent.chars().get();
        assert!(
            pos <= self.len(),
            "insert_emacs_bytes_both: position {pos} out of range (len {})",
            self.len()
        );
        if bytes.is_empty() {
            return;
        }
        debug_assert!(
            pos == self.len() || self.is_char_boundary(pos),
            "insert_emacs_bytes_both: position {pos} is not on an Emacs character boundary"
        );
        debug_assert_eq!(
            extent.emacs_bytes().get(),
            bytes.len(),
            "insert_emacs_bytes_both: caller-supplied byte count mismatches actual"
        );
        debug_assert_eq!(
            CharLen::new(nchars),
            emacs_char_count_bytes(bytes, self.multibyte),
            "insert_emacs_bytes_both: caller-supplied nchars mismatches actual"
        );

        let inserted_bytes = bytes.len();
        self.move_gap_to(pos);
        self.ensure_gap(inserted_bytes);

        self.buf[self.gap_start..self.gap_start + inserted_bytes].copy_from_slice(bytes);
        self.gap_start += inserted_bytes;
        self.gap_start_chars += nchars;
        self.total_chars += nchars;
        self.gap_start_bytes += inserted_bytes;
        self.total_bytes += inserted_bytes;
        self.adjust_position_anchors_for_insert(pos, inserted_bytes, nchars);
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(crate) fn insert_storage_string_at_emacs_byte_pos(&mut self, pos: EmacsBytePos, s: &str) {
        if s.is_empty() {
            return;
        }
        let bytes =
            crate::emacs_core::string_escape::storage_string_to_buffer_bytes(s, self.multibyte);
        self.insert_emacs_bytes_at_emacs_byte_pos(pos, &bytes);
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(crate) fn delete_emacs_byte_range(&mut self, range: EmacsByteRange) {
        let start = range.start().get();
        let end = range.end().get();
        assert!(start <= end, "delete_range: start ({start}) > end ({end})");
        assert!(
            end <= self.len(),
            "delete_range: end ({end}) > len ({})",
            self.len()
        );
        if start == end {
            return;
        }
        // Count chars in the about-to-be-deleted region. This is the scan that
        // delete_emacs_byte_range_with_char_len lets callers skip.
        let mut tmp = Vec::with_capacity(end - start);
        self.copy_emacs_byte_range_to(range, &mut tmp);
        let nchars = emacs_char_count_bytes(&tmp, self.multibyte);
        self.delete_emacs_byte_range_with_char_len(range, nchars);
    }

    /// Delete the logical byte range `[start, end)`, given pre-computed char
    /// count of the region.
    ///
    /// Mirrors GNU `del_range_2` (`src/insdel.c:1991`).
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(crate) fn delete_emacs_byte_range_with_char_len(
        &mut self,
        range: EmacsByteRange,
        char_len: CharLen,
    ) {
        let start_char = self.emacs_byte_pos_to_char_pos(range.start());
        self.delete_measured_range(TextEditRange::from_start_extent(
            range.start(),
            start_char,
            TextExtent::new(char_len, range.len()),
        ));
    }

    pub(crate) fn delete_measured_range(&mut self, range: TextEditRange) {
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
        debug_assert!(
            self.is_char_boundary(start),
            "delete_range_both: start ({start}) is not on an Emacs character boundary"
        );
        debug_assert!(
            end == self.len() || self.is_char_boundary(end),
            "delete_range_both: end ({end}) is not on an Emacs character boundary"
        );
        debug_assert_eq!(
            nchars,
            self.emacs_byte_pos_to_char_pos(EmacsBytePos::new(end))
                .get()
                - self
                    .emacs_byte_pos_to_char_pos(EmacsBytePos::new(start))
                    .get(),
            "delete_range_both: caller-supplied nchars mismatches actual"
        );

        // The range is measured: its char start is known, so the gap move
        // needs no byte->char conversion (GNU `del_range_2` has both
        // positions in hand when it calls `gap_left`/`gap_right`).
        self.move_gap_to_emacs_byte_pos_and_char_pos(range.byte_start(), range.char_start());
        let deleted_bytes = end - start;
        // After the gap move, bytes [start, end) now live at
        // buf[gap_end .. gap_end + deleted_bytes]; extend the gap to swallow them.
        self.gap_end += deleted_bytes;
        self.total_chars -= nchars;
        self.total_bytes -= deleted_bytes;
        self.adjust_position_anchors_for_delete(start, end, deleted_bytes, nchars);
    }

    pub(crate) fn replace_measured_range(&mut self, replacement: TextReplacement, bytes: &[u8]) {
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

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(crate) fn replace_same_len_emacs_byte_range(
        &mut self,
        range: EmacsByteRange,
        replacement: &[u8],
    ) {
        let start_char = self.emacs_byte_pos_to_char_pos(range.start());
        let end_char = self.emacs_byte_pos_to_char_pos(range.end());
        self.replace_same_len_measured_range(
            TextReplacement::new(
                TextEditRange::from_start_end(
                    TextPositionAnchor::new(start_char, range.start()),
                    TextPositionAnchor::new(end_char, range.end()),
                ),
                TextExtent::from_emacs_bytes(replacement, self.multibyte),
            ),
            replacement,
        );
    }

    pub(crate) fn replace_same_len_measured_range(
        &mut self,
        replacement: TextReplacement,
        bytes: &[u8],
    ) {
        let start = replacement.old_range().byte_start().get();
        let end = replacement.old_range().byte_end().get();
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
        debug_assert!(
            self.is_char_boundary(start),
            "replace_same_len_range: start ({start}) is not on an Emacs character boundary"
        );
        debug_assert!(
            end == self.len() || self.is_char_boundary(end),
            "replace_same_len_range: end ({end}) is not on an Emacs character boundary"
        );

        let before_gap_len = if start < self.gap_start_bytes {
            end.min(self.gap_start_bytes) - start
        } else {
            0
        };
        let after_gap_len = if end > self.gap_start_bytes {
            end - start.max(self.gap_start_bytes)
        } else {
            0
        };
        let gap = self.gap_size();
        let old_before_chars = if before_gap_len == 0 {
            CharLen::ZERO
        } else {
            emacs_char_count_bytes(&self.buf[start..start + before_gap_len], self.multibyte)
        };
        let old_after_chars = if after_gap_len == 0 {
            CharLen::ZERO
        } else {
            let phys_start = start.max(self.gap_start_bytes) + gap;
            emacs_char_count_bytes(
                &self.buf[phys_start..phys_start + after_gap_len],
                self.multibyte,
            )
        };
        debug_assert_eq!(
            replacement.new_char_len(),
            emacs_char_count_bytes(bytes, self.multibyte),
            "replace_same_len_range: measured new char count mismatches replacement bytes"
        );
        debug_assert_eq!(
            replacement.old_char_len(),
            old_before_chars.add_len(old_after_chars),
            "replace_same_len_range: measured old char count mismatches storage"
        );

        if before_gap_len != 0 {
            self.buf[start..start + before_gap_len].copy_from_slice(&bytes[..before_gap_len]);
        }
        if after_gap_len != 0 {
            let src_start = before_gap_len;
            let phys_start = start.max(self.gap_start_bytes) + gap;
            self.buf[phys_start..phys_start + after_gap_len]
                .copy_from_slice(&bytes[src_start..src_start + after_gap_len]);
        }

        let old_chars = replacement.old_char_len().get();
        let new_chars = replacement.new_char_len().get();
        let new_before_chars = emacs_char_count_bytes(&bytes[..before_gap_len], self.multibyte);
        if old_before_chars != new_before_chars {
            let delta = new_before_chars.get() as isize - old_before_chars.get() as isize;
            self.gap_start_chars = self.gap_start_chars.saturating_add_signed(delta);
        }
        if old_chars != new_chars {
            let delta = new_chars as isize - old_chars as isize;
            self.total_chars = self.total_chars.saturating_add_signed(delta);
        }
        // An in-place replacement can shift char boundaries inside the replaced
        // range even when the total counts are unchanged, so invalidate the
        // position anchors unconditionally.
        self.reset_position_anchors();
    }

    // -----------------------------------------------------------------------
    // Gap management
    // -----------------------------------------------------------------------

    /// Move the gap out of `range` so the whole range becomes one
    /// contiguous slice.
    ///
    /// The regex searcher uses this instead of copying the accessible
    /// region on every search when the gap sits mid-buffer (audit #17:
    /// GNU searches across the gap with `re_search_2`'s two-segment
    /// protocol; neomacs's engine is single-segment, so one gap motion
    /// per gap-position change is the equivalent fix).  Content and all
    /// logical positions are unaffected — only the physical gap moves,
    /// exactly as GNU `move_gap_both` does before `fast_string_match`
    /// style operations.  Cost: one memmove of the shorter distance from
    /// the gap to either range boundary.
    pub(crate) fn make_emacs_byte_range_contiguous(&mut self, range: EmacsByteRange) {
        if self.has_contiguous_emacs_byte_range(range) {
            return;
        }
        let start = range.start().get();
        let end = range.end().get();
        // Not contiguous, so the gap lies strictly inside (start, end);
        // move it to the nearer boundary.
        if self.gap_start - start <= end - self.gap_start {
            self.move_gap_to(start);
        } else {
            self.move_gap_to(end);
        }
    }

    /// Move the gap so that `gap_start == pos`.
    ///
    /// Wrapper that computes the char delta by scanning moved bytes. Prefer
    /// `move_gap_both` when the caller knows the target char position.
    fn reset_position_anchors(&self) {
        self.position_anchors
            .set([TextPositionAnchor::ZERO; POSITION_ANCHOR_RING]);
        self.anchor_next.set(0);
    }

    fn remember_position_anchor(&self, anchor: TextPositionAnchor) {
        let mut ring = self.position_anchors.get();
        let slot = self.anchor_next.get();
        ring[slot] = anchor;
        self.position_anchors.set(ring);
        self.anchor_next.set((slot + 1) % POSITION_ANCHOR_RING);
    }

    /// The ring's anchors that lie inside the current text (adjustment keeps
    /// them there; the guard is belt and braces).
    fn valid_position_anchors(&self) -> impl Iterator<Item = TextPositionAnchor> {
        let (total_bytes, total_chars) = (self.total_bytes, self.total_chars);
        self.position_anchors
            .get()
            .into_iter()
            .filter(move |anchor| {
                anchor.emacs_byte_pos_usize() <= total_bytes
                    && anchor.char_pos_usize() <= total_chars
            })
    }

    /// Marker rule for an insertion of `bytes`/`chars` at `pos`: anchors at or
    /// before the insertion keep their correspondence, later ones shift.
    fn adjust_position_anchors_for_insert(&self, pos: usize, bytes: usize, chars: usize) {
        let ring = self.position_anchors.get().map(|anchor| {
            if anchor.emacs_byte_pos_usize() > pos {
                TextPositionAnchor::new(
                    CharPos0::new(anchor.char_pos_usize() + chars),
                    EmacsBytePos::new(anchor.emacs_byte_pos_usize() + bytes),
                )
            } else {
                anchor
            }
        });
        self.position_anchors.set(ring);
    }

    /// Marker rule for deleting `[start, end)` (`bytes`/`chars` long):
    /// anchors before keep, after shift back, inside are dropped (the zero
    /// anchor is always valid).
    fn adjust_position_anchors_for_delete(
        &self,
        start: usize,
        end: usize,
        bytes: usize,
        chars: usize,
    ) {
        let ring = self.position_anchors.get().map(|anchor| {
            let byte = anchor.emacs_byte_pos_usize();
            if byte <= start {
                anchor
            } else if byte >= end {
                TextPositionAnchor::new(
                    CharPos0::new(anchor.char_pos_usize() - chars),
                    EmacsBytePos::new(byte - bytes),
                )
            } else {
                TextPositionAnchor::ZERO
            }
        });
        self.position_anchors.set(ring);
    }

    fn move_gap_to(&mut self, pos: usize) {
        assert!(
            pos <= self.len(),
            "move_gap_to: position {pos} out of range (len {})",
            self.len()
        );
        if pos == self.gap_start {
            return;
        }
        // Derive the target char position through the anchored converter
        // rather than scanning the whole moved region: it is O(1) when
        // chars == bytes (unibyte and all-ASCII buffers - `Z == Z_byte`,
        // GNU marker.c's fast path) and otherwise walks from the NEAREST of
        // {0, gap, end, cache}, which is never farther than the moved region
        // the old scan counted. kill/yank cycles were re-counting ~250KB per
        // gap move (~14.5M Ir of a 71M killyank phase) in a buffer where the
        // answer was arithmetic.
        let charpos = self
            .emacs_byte_pos_to_char_pos(EmacsBytePos::new(pos))
            .get();
        self.move_gap_to_emacs_byte_pos_and_char_pos(
            EmacsBytePos::new(pos),
            CharPos0::new(charpos),
        );
    }

    pub(crate) fn move_gap_to_emacs_byte_pos_and_char_pos(
        &mut self,
        bytepos: EmacsBytePos,
        charpos: CharPos0,
    ) {
        let bytepos = bytepos.get();
        let charpos = charpos.get();
        assert!(
            bytepos <= self.len(),
            "move_gap_both: bytepos {bytepos} out of range (len {})",
            self.len()
        );
        if bytepos == self.gap_start {
            return;
        }
        let gap = self.gap_size();

        if bytepos < self.gap_start {
            let count = self.gap_start - bytepos;
            self.buf
                .copy_within(bytepos..bytepos + count, bytepos + gap);
        } else {
            let count = bytepos - self.gap_start;
            let src_start = self.gap_end;
            let dst_start = self.gap_start;
            self.buf
                .copy_within(src_start..src_start + count, dst_start);
        }
        self.gap_start = bytepos;
        self.gap_end = bytepos + gap;
        self.gap_start_chars = charpos;
        self.gap_start_bytes = bytepos;
    }

    /// Ensure the gap is at least `min_size` bytes. If it is already large
    /// enough this is a no-op; otherwise the backing buffer is reallocated.
    pub fn ensure_gap(&mut self, min_size: usize) {
        if self.gap_size() >= min_size {
            return;
        }
        // GNU insdel.c:483 (`make_gap_larger`): add GAP_BYTES_DFL beyond the
        // caller's requested need so a run of sequential inserts is amortized
        // O(1) rather than paying realloc on every ~64 bytes.
        let need = min_size - self.gap_size();
        let grow = need.saturating_add(GAP_BYTES_DFL);
        let old_gap_end = self.gap_end;
        let after_gap_len = self.buf.len() - old_gap_end;

        self.buf.resize(self.buf.len() + grow, 0);

        if after_gap_len > 0 {
            self.buf
                .copy_within(old_gap_end..old_gap_end + after_gap_len, old_gap_end + grow);
        }
        self.gap_end += grow;
    }

    // -----------------------------------------------------------------------
    // Position conversion
    // -----------------------------------------------------------------------

    /// Convert a logical Emacs byte position to a logical character position.
    ///
    /// Returns the number of complete characters before `byte_pos`.
    ///
    /// # Panics
    ///
    /// Panics if `byte_pos > self.len()` or is not on an Emacs character
    /// boundary.
    pub(crate) fn emacs_byte_pos_to_char_pos(&self, byte_pos: EmacsBytePos) -> CharPos0 {
        let byte_pos = byte_pos.get();
        assert!(
            byte_pos <= self.len(),
            "byte_to_char: byte_pos ({byte_pos}) > len ({})",
            self.len()
        );
        // GNU marker.c fast path (`if (Z == Z_byte) return bytepos`): when the
        // buffer has as many characters as bytes, every character is one byte,
        // so the char position equals the byte position.  Covers unibyte and
        // all-ASCII multibyte buffers in O(1) instead of scanning and decoding
        // from the buffer start.
        if self.total_chars == self.total_bytes {
            return CharPos0::new(byte_pos);
        }
        let result = self.char_pos_from_byte_anchors(byte_pos, self.valid_position_anchors());
        // The anchors must never change the answer: validate against the
        // anchor-free computation in debug/test builds so any missed
        // adjustment fails loudly.
        debug_assert_eq!(
            result,
            self.char_pos_from_byte_anchors(byte_pos, std::iter::empty()),
            "stale byte->char position anchor at byte {byte_pos}"
        );
        self.remember_position_anchor(TextPositionAnchor::new(
            CharPos0::new(result),
            EmacsBytePos::new(byte_pos),
        ));
        CharPos0::new(result)
    }

    /// Char position for logical byte `target`, scanning from the nearest of
    /// the structural anchors `{0, gap, end}` plus an optional extra anchor
    /// (the cache).  Mirrors GNU `marker.c`'s nearest-anchor scan, so
    /// sequential conversions cost O(distance between calls), not O(buffer).
    fn char_pos_from_byte_anchors(
        &self,
        target: usize,
        extra: impl IntoIterator<Item = TextPositionAnchor>,
    ) -> usize {
        let mut below = TextPositionAnchor::ZERO;
        let mut above = TextPositionAnchor::new(
            CharPos0::new(self.total_chars),
            EmacsBytePos::new(self.total_bytes),
        );
        for anchor in std::iter::once(TextPositionAnchor::new(
            CharPos0::new(self.gap_start_chars),
            EmacsBytePos::new(self.gap_start_bytes),
        ))
        .chain(extra)
        {
            let byte_pos = anchor.emacs_byte_pos_usize();
            if byte_pos <= target && byte_pos > below.emacs_byte_pos_usize() {
                below = anchor;
            }
            if byte_pos >= target && byte_pos < above.emacs_byte_pos_usize() {
                above = anchor;
            }
        }
        let below_byte = below.emacs_byte_pos_usize();
        let above_byte = above.emacs_byte_pos_usize();
        // GNU marker.c CONSIDER interpolation: if the bracketing anchors
        // span as many chars as bytes, everything between them is
        // single-byte and the conversion is pure arithmetic. In an
        // ASCII-dominant buffer this collapses most conversions to zero
        // scanning (the multibyte characters cluster in a few spans).
        if above_byte - below_byte == above.char_pos_usize() - below.char_pos_usize() {
            return below.char_pos_usize() + (target - below_byte);
        }
        if target - below_byte <= above_byte - target {
            below.char_pos_usize() + self.count_chars_in_logical_byte_range(below_byte, target)
        } else {
            above.char_pos_usize() - self.count_chars_in_logical_byte_range(target, above_byte)
        }
    }

    /// Count Emacs characters in the logical byte range `[lo, hi)`, mapping
    /// logical positions through the gap.  Both ends must be char boundaries
    /// (callers only pass known correspondences and the gap split, which are
    /// all char-aligned).
    fn count_chars_in_logical_byte_range(&self, lo: usize, hi: usize) -> usize {
        debug_assert!(lo <= hi && hi <= self.total_bytes);
        let mut chars = 0;
        if lo < self.gap_start_bytes {
            let pre_hi = hi.min(self.gap_start_bytes);
            chars += emacs_char_count_bytes(&self.buf[lo..pre_hi], self.multibyte).get();
        }
        if hi > self.gap_start_bytes {
            let post_lo = lo.max(self.gap_start_bytes);
            let phys_lo = self.gap_end + (post_lo - self.gap_start_bytes);
            let phys_hi = self.gap_end + (hi - self.gap_start_bytes);
            chars += emacs_char_count_bytes(&self.buf[phys_lo..phys_hi], self.multibyte).get();
        }
        chars
    }

    /// Convert a char position to a logical Emacs byte position.
    ///
    /// `char_pos` is the number of characters from the start of the buffer.
    ///
    pub(crate) fn char_pos_to_emacs_byte_pos(&self, char_pos: CharPos0) -> EmacsBytePos {
        let char_pos = char_pos.get();
        if char_pos == 0 {
            return EmacsBytePos::new(0);
        }
        if char_pos > self.total_chars {
            // Clamp to end of buffer instead of panicking — this can happen
            // when window_start / point are stale after buffer modification.
            // Must precede the fast path below, which would otherwise return
            // the unclamped position for an all-single-byte buffer.
            tracing::debug!(
                "char_to_byte: char_pos ({char_pos}) exceeds char_count ({}), clamping",
                self.total_chars
            );
            return EmacsBytePos::new(self.total_bytes);
        }
        // GNU marker.c fast path: as many characters as bytes => every
        // character is one byte, so byte position equals char position.
        if self.total_chars == self.total_bytes {
            return EmacsBytePos::new(char_pos);
        }
        let result = self.byte_pos_from_char_anchors(char_pos, self.valid_position_anchors());
        debug_assert_eq!(
            result,
            self.byte_pos_from_char_anchors(char_pos, std::iter::empty()),
            "stale char->byte position anchor at char {char_pos}"
        );
        self.remember_position_anchor(TextPositionAnchor::new(
            CharPos0::new(char_pos),
            EmacsBytePos::new(result),
        ));
        EmacsBytePos::new(result)
    }

    /// Byte position for char `target` (must be `<= total_chars`), scanning
    /// forward from the nearest known char anchor at or below it (structural
    /// anchors `{0, gap}` plus the optional cache).  Shares GNU `marker.c`'s
    /// cached correspondence with `char_pos_from_byte_anchors`.
    fn byte_pos_from_char_anchors(
        &self,
        target: usize,
        extra: impl IntoIterator<Item = TextPositionAnchor>,
    ) -> usize {
        let mut below = TextPositionAnchor::ZERO;
        let mut above = TextPositionAnchor::new(
            CharPos0::new(self.total_chars),
            EmacsBytePos::new(self.total_bytes),
        );
        for anchor in std::iter::once(TextPositionAnchor::new(
            CharPos0::new(self.gap_start_chars),
            EmacsBytePos::new(self.gap_start_bytes),
        ))
        .chain(extra)
        {
            let char_pos = anchor.char_pos_usize();
            if char_pos <= target && char_pos > below.char_pos_usize() {
                below = anchor;
            }
            if char_pos >= target && char_pos < above.char_pos_usize() {
                above = anchor;
            }
        }
        let below_byte = below.emacs_byte_pos_usize();
        let below_char = below.char_pos_usize();
        let above_byte = above.emacs_byte_pos_usize();
        let above_char = above.char_pos_usize();
        // GNU marker.c CONSIDER interpolation — see char_pos_from_byte_anchors.
        if above_byte - below_byte == above_char - below_char {
            return below_byte + (target - below_char);
        }
        if target - below_char <= above_char - target {
            below_byte + self.bytes_for_n_chars_from_logical_byte(below_byte, target - below_char)
        } else {
            above_byte - self.bytes_for_n_chars_before_logical_byte(above_byte, above_char - target)
        }
    }

    /// Logical byte span covering the `nchars` characters ENDING at logical
    /// byte `end_byte` (a char boundary), mapped through the gap — the
    /// backward twin of `bytes_for_n_chars_from_logical_byte`, so a
    /// conversion can scan from an anchor above the target when that side
    /// is closer (GNU buf_charpos_to_bytepos scans from the nearer anchor).
    fn bytes_for_n_chars_before_logical_byte(&self, end_byte: usize, nchars: usize) -> usize {
        if nchars == 0 {
            return 0;
        }
        let mut remaining = nchars;
        let mut consumed = 0;
        if end_byte > self.gap_start_bytes {
            let phys_end = self.gap_end + (end_byte - self.gap_start_bytes);
            let slice = &self.buf[self.gap_end..phys_end];
            match nth_lead_from_end(slice, remaining, self.multibyte) {
                Some(offset) => return consumed + (slice.len() - offset),
                None => {
                    remaining -= emacs_char_count_bytes(slice, self.multibyte).get();
                    consumed += slice.len();
                }
            }
        }
        let pre_end = end_byte.min(self.gap_start_bytes);
        let slice = &self.buf[..pre_end];
        match nth_lead_from_end(slice, remaining, self.multibyte) {
            Some(offset) => consumed + (slice.len() - offset),
            None => consumed + slice.len(),
        }
    }

    /// Logical byte span covering the next `nchars` characters starting at
    /// logical byte `start_byte` (a char boundary), mapped through the gap.
    fn bytes_for_n_chars_from_logical_byte(&self, start_byte: usize, nchars: usize) -> usize {
        if nchars == 0 {
            return 0;
        }
        let mut remaining = nchars;
        let mut consumed = 0;
        if start_byte < self.gap_start_bytes {
            let slice = &self.buf[start_byte..self.gap_start];
            let avail = emacs_char_count_bytes(slice, self.multibyte).get();
            if remaining <= avail {
                return emacs_char_to_byte_in_slice(slice, remaining, self.multibyte);
            }
            remaining -= avail;
            consumed = slice.len();
        }
        let post_phys = if start_byte >= self.gap_start_bytes {
            self.gap_end + (start_byte - self.gap_start_bytes)
        } else {
            self.gap_end
        };
        consumed + emacs_char_to_byte_in_slice(&self.buf[post_phys..], remaining, self.multibyte)
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    /// Check whether `pos` falls on a logical Emacs-character boundary in the
    /// text. O(1): single-byte bit test matching GNU's `CHAR_HEAD_P`
    /// (character.h). Multibyte trailing bytes have the form 10xxxxxx (0x80..=0xBF).
    /// Any other byte value is a character head.
    fn is_char_boundary(&self, pos: usize) -> bool {
        if !self.multibyte || pos == 0 || pos >= self.len() {
            return true;
        }
        // Multibyte trailing bytes have the form 10xxxxxx (0x80..=0xBF).
        // Any other byte value is a character head.
        (self.byte_at(pos) & 0xC0) != 0x80
    }

    // pdump accessors
    /// Extract the logical text content as a byte vector (for pdump).
    pub(crate) fn dump_text(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.len());
        out.extend_from_slice(&self.buf[..self.gap_start]);
        out.extend_from_slice(&self.buf[self.gap_end..]);
        out
    }
    /// Reconstruct from text bytes (for pdump load).
    pub(crate) fn from_dump(text: Vec<u8>, multibyte: bool) -> Self {
        let len = text.len();
        let char_count = emacs_char_count_bytes(&text, multibyte).get();
        let byte_count = text.len();
        Self {
            buf: text,
            multibyte,
            gap_start: len,
            gap_end: len,
            gap_start_chars: char_count,
            total_chars: char_count,
            gap_start_bytes: byte_count,
            total_bytes: byte_count,
            position_anchors: Cell::new([TextPositionAnchor::ZERO; POSITION_ANCHOR_RING]),
            anchor_next: Cell::new(0),
        }
    }
}

// ---------------------------------------------------------------------------
// Trait implementations
// ---------------------------------------------------------------------------

impl Default for GapBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for GapBuffer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text_emacs_byte_range(EmacsByteRange::new(
            EmacsBytePos::ZERO,
            EmacsBytePos::new(self.len()),
        )))
    }
}

impl fmt::Debug for GapBuffer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GapBuffer")
            .field("len", &self.len())
            .field("char_count", &self.total_chars)
            .field("gap_start", &self.gap_start)
            .field("gap_start_chars", &self.gap_start_chars)
            .field("gap_start_bytes", &self.gap_start_bytes)
            .field("gap_end", &self.gap_end)
            .field("gap_size", &self.gap_size())
            .field("emacs_byte_len", &self.total_bytes)
            .field("text", &self.to_string())
            .finish()
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
#[path = "gap_buffer_test.rs"]
mod tests;

/// Byte offset (from the START of `slice`) of the lead byte that begins the
/// `n`-th character counting BACKWARD from the end of `slice` (1-based), or
/// `None` if `slice` holds fewer than `n` characters. Block-counts leads from
/// the tail (vectorized) and walks only the final block, mirroring
/// `char_to_byte_pos`'s forward form.
fn nth_lead_from_end(slice: &[u8], n: usize, multibyte: bool) -> Option<usize> {
    if !multibyte {
        return (slice.len() >= n).then(|| slice.len() - n);
    }
    let mut remaining = n;
    let mut end = slice.len();
    const BLOCK: usize = 64;
    while end >= BLOCK {
        let leads = slice[end - BLOCK..end]
            .iter()
            .filter(|&&b| (b & 0xC0) != 0x80)
            .count();
        if leads >= remaining {
            break;
        }
        remaining -= leads;
        end -= BLOCK;
    }
    for i in (0..end).rev() {
        if (slice[i] & 0xC0) != 0x80 {
            remaining -= 1;
            if remaining == 0 {
                return Some(i);
            }
        }
    }
    None
}
