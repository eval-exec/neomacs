//! Buffer and BufferManager — the core text container for the Elisp VM.
//!
//! A `Buffer` wraps a [`GapBuffer`] with Emacs-style point, mark, narrowing,
//! markers, and buffer-local variables.  `BufferManager` owns all live buffers
//! and tracks the current buffer.

use std::collections::HashMap;

use super::gap_buffer::GapBuffer;
use super::overlay::OverlayList;
use super::text_props::TextPropertyTable;
use crate::elisp::syntax::SyntaxTable;
use crate::elisp::value::Value;

// ---------------------------------------------------------------------------
// BufferId
// ---------------------------------------------------------------------------

/// Opaque, cheaply-copyable identifier for a buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BufferId(pub u64);

// ---------------------------------------------------------------------------
// InsertionType
// ---------------------------------------------------------------------------

/// Controls whether a marker advances when text is inserted exactly at its
/// position.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InsertionType {
    /// Marker stays before the new text (does NOT advance).
    Before,
    /// Marker moves after the new text (advances).
    After,
}

// ---------------------------------------------------------------------------
// MarkerEntry
// ---------------------------------------------------------------------------

/// A tracked position inside a buffer.
#[derive(Clone, Debug)]
pub struct MarkerEntry {
    pub id: u64,
    pub byte_pos: usize,
    pub insertion_type: InsertionType,
}

// ---------------------------------------------------------------------------
// Buffer
// ---------------------------------------------------------------------------

/// A single text buffer with point, mark, narrowing, markers, and local vars.
pub struct Buffer {
    /// Unique identifier.
    pub id: BufferId,
    /// Buffer name (e.g. `"*scratch*"`).
    pub name: String,
    /// The underlying text storage.
    pub text: GapBuffer,
    /// Point — the current cursor byte position.
    pub pt: usize,
    /// Mark — optional byte position for region operations.
    pub mark: Option<usize>,
    /// Beginning of accessible (narrowed) portion (byte pos, inclusive).
    pub begv: usize,
    /// End of accessible (narrowed) portion (byte pos, exclusive).
    pub zv: usize,
    /// Whether the buffer has been modified since last save.
    pub modified: bool,
    /// If true, insertions/deletions are forbidden.
    pub read_only: bool,
    /// Multi-byte encoding flag.  Always `true` for now.
    pub multibyte: bool,
    /// Associated file path, if any.
    pub file_name: Option<String>,
    /// Active markers that track positions across edits.
    pub markers: Vec<MarkerEntry>,
    /// Buffer-local variables (name -> Lisp value).
    pub properties: HashMap<String, Value>,
    /// Text properties attached to ranges of text.
    pub text_props: TextPropertyTable,
    /// Overlays attached to the buffer.
    pub overlays: OverlayList,
    /// Syntax table for character classification.
    pub syntax_table: SyntaxTable,
}

impl Buffer {
    // -- Construction --------------------------------------------------------

    /// Create a new, empty buffer.
    pub fn new(id: BufferId, name: String) -> Self {
        Self {
            id,
            name,
            text: GapBuffer::new(),
            pt: 0,
            mark: None,
            begv: 0,
            zv: 0,
            modified: false,
            read_only: false,
            multibyte: true,
            file_name: None,
            markers: Vec::new(),
            properties: HashMap::new(),
            text_props: TextPropertyTable::new(),
            overlays: OverlayList::new(),
            syntax_table: SyntaxTable::new_standard(),
        }
    }

    // -- Point queries -------------------------------------------------------

    /// Current point as a byte position.
    pub fn point(&self) -> usize {
        self.pt
    }

    /// Current point converted to a character position.
    pub fn point_char(&self) -> usize {
        self.text.byte_to_char(self.pt)
    }

    /// Beginning of the accessible portion (byte position).
    pub fn point_min(&self) -> usize {
        self.begv
    }

    /// End of the accessible portion (byte position).
    pub fn point_max(&self) -> usize {
        self.zv
    }

    // -- Point movement ------------------------------------------------------

    /// Set point, clamping to the accessible region `[begv, zv]`.
    pub fn goto_char(&mut self, pos: usize) {
        self.pt = pos.clamp(self.begv, self.zv);
    }

    // -- Editing -------------------------------------------------------------

    /// Insert `text` at point, advancing point past the inserted text.
    ///
    /// Markers at the insertion site move according to their `InsertionType`.
    pub fn insert(&mut self, text: &str) {
        let insert_pos = self.pt;
        let len = text.len();
        if len == 0 {
            return;
        }

        self.text.insert_str(insert_pos, text);

        // Advance point past inserted text.
        self.pt += len;

        // Adjust zv (end of accessible region grows with buffer).
        self.zv += len;

        // Adjust mark.
        if let Some(m) = self.mark {
            if m > insert_pos {
                self.mark = Some(m + len);
            }
        }

        // Adjust markers.
        for marker in &mut self.markers {
            if marker.byte_pos > insert_pos {
                marker.byte_pos += len;
            } else if marker.byte_pos == insert_pos {
                // Insertion at exact marker position: behaviour depends on type.
                if marker.insertion_type == InsertionType::After {
                    marker.byte_pos += len;
                }
                // InsertionType::Before => marker stays put.
            }
        }

        // Adjust text properties and overlays.
        self.text_props.adjust_for_insert(insert_pos, len);
        self.overlays.adjust_for_insert(insert_pos, len);

        self.modified = true;
    }

    /// Delete the byte range `[start, end)`.
    ///
    /// Adjusts point, mark, markers, and the narrowing boundary.
    pub fn delete_region(&mut self, start: usize, end: usize) {
        if start >= end {
            return;
        }
        let len = end - start;

        self.text.delete_range(start, end);

        // Adjust point.
        if self.pt > end {
            self.pt -= len;
        } else if self.pt > start {
            self.pt = start;
        }

        // Adjust mark.
        if let Some(m) = self.mark {
            if m > end {
                self.mark = Some(m - len);
            } else if m > start {
                self.mark = Some(start);
            }
        }

        // Adjust markers.
        for marker in &mut self.markers {
            if marker.byte_pos > end {
                marker.byte_pos -= len;
            } else if marker.byte_pos > start {
                marker.byte_pos = start;
            }
        }

        // Adjust zv.
        if self.zv > end {
            self.zv -= len;
        } else if self.zv > start {
            self.zv = start;
        }

        // Adjust text properties and overlays.
        self.text_props.adjust_for_delete(start, end);
        self.overlays.adjust_for_delete(start, end);

        self.modified = true;
    }

    // -- Text queries --------------------------------------------------------

    /// Return a `String` copy of the byte range `[start, end)`.
    pub fn buffer_substring(&self, start: usize, end: usize) -> String {
        self.text.text_range(start, end)
    }

    /// Return the entire accessible portion of the buffer as a `String`.
    pub fn buffer_string(&self) -> String {
        self.text.text_range(self.begv, self.zv)
    }

    /// Byte-length of the accessible portion.
    pub fn buffer_size(&self) -> usize {
        self.zv - self.begv
    }

    /// Character at byte position `pos`, or `None` if out of range.
    pub fn char_after(&self, pos: usize) -> Option<char> {
        if pos >= self.text.len() {
            return None;
        }
        self.text.char_at(pos)
    }

    /// Character immediately before byte position `pos`, or `None`.
    pub fn char_before(&self, pos: usize) -> Option<char> {
        if pos == 0 || pos > self.text.len() {
            return None;
        }
        // Walk backwards to find the start of the previous UTF-8 character.
        // The gap buffer stores valid UTF-8, so we can probe up to 4 bytes back.
        let text = self.text.to_string();
        let bytes = text.as_bytes();
        let mut back = 1;
        while back <= 4 && back <= pos {
            let idx = pos - back;
            if (bytes[idx] & 0xC0) != 0x80 {
                // Found a leading byte.
                return text[idx..pos].chars().next();
            }
            back += 1;
        }
        None
    }

    // -- Narrowing -----------------------------------------------------------

    /// Restrict the accessible portion to `[start, end)`.
    pub fn narrow_to_region(&mut self, start: usize, end: usize) {
        let total = self.text.len();
        let s = start.min(total);
        let e = end.clamp(s, total);
        self.begv = s;
        self.zv = e;
        // Clamp point into the new accessible region.
        self.pt = self.pt.clamp(self.begv, self.zv);
    }

    /// Remove narrowing — make the entire buffer accessible again.
    pub fn widen(&mut self) {
        self.begv = 0;
        self.zv = self.text.len();
    }

    // -- Mark ----------------------------------------------------------------

    /// Set the mark to `pos`.
    pub fn set_mark(&mut self, pos: usize) {
        self.mark = Some(pos);
    }

    /// Return the mark, if set.
    pub fn mark(&self) -> Option<usize> {
        self.mark
    }

    // -- Modified flag -------------------------------------------------------

    pub fn is_modified(&self) -> bool {
        self.modified
    }

    pub fn set_modified(&mut self, flag: bool) {
        self.modified = flag;
    }

    // -- Buffer-local variables ----------------------------------------------

    pub fn set_buffer_local(&mut self, name: &str, value: Value) {
        self.properties.insert(name.to_string(), value);
    }

    pub fn get_buffer_local(&self, name: &str) -> Option<&Value> {
        self.properties.get(name)
    }
}

// ---------------------------------------------------------------------------
// BufferManager
// ---------------------------------------------------------------------------

/// Owns every live buffer, tracks the current buffer, and hands out ids.
pub struct BufferManager {
    buffers: HashMap<BufferId, Buffer>,
    current: Option<BufferId>,
    next_id: u64,
    next_marker_id: u64,
}

impl BufferManager {
    /// Create a new `BufferManager` pre-populated with a `*scratch*` buffer.
    pub fn new() -> Self {
        let mut mgr = Self {
            buffers: HashMap::new(),
            current: None,
            next_id: 1,
            next_marker_id: 1,
        };
        let scratch = mgr.create_buffer("*scratch*");
        mgr.current = Some(scratch);
        mgr
    }

    /// Allocate a new buffer with the given name and return its id.
    pub fn create_buffer(&mut self, name: &str) -> BufferId {
        let id = BufferId(self.next_id);
        self.next_id += 1;
        let buf = Buffer::new(id, name.to_string());
        self.buffers.insert(id, buf);
        id
    }

    /// Immutable access to a buffer by id.
    pub fn get(&self, id: BufferId) -> Option<&Buffer> {
        self.buffers.get(&id)
    }

    /// Mutable access to a buffer by id.
    pub fn get_mut(&mut self, id: BufferId) -> Option<&mut Buffer> {
        self.buffers.get_mut(&id)
    }

    /// Immutable access to the current buffer.
    pub fn current_buffer(&self) -> Option<&Buffer> {
        self.current.and_then(|id| self.buffers.get(&id))
    }

    /// Mutable access to the current buffer.
    pub fn current_buffer_mut(&mut self) -> Option<&mut Buffer> {
        self.current.and_then(|id| self.buffers.get_mut(&id))
    }

    /// Switch the current buffer.
    pub fn set_current(&mut self, id: BufferId) {
        if self.buffers.contains_key(&id) {
            self.current = Some(id);
        }
    }

    /// Find a buffer by name, returning its id if it exists.
    pub fn find_buffer_by_name(&self, name: &str) -> Option<BufferId> {
        self.buffers
            .values()
            .find(|b| b.name == name)
            .map(|b| b.id)
    }

    /// Remove a buffer.  Returns `true` if the buffer existed.
    ///
    /// If the killed buffer was current, `current` is set to `None`.
    pub fn kill_buffer(&mut self, id: BufferId) -> bool {
        if self.buffers.remove(&id).is_some() {
            if self.current == Some(id) {
                self.current = None;
            }
            true
        } else {
            false
        }
    }

    /// List all live buffer ids (arbitrary order).
    pub fn buffer_list(&self) -> Vec<BufferId> {
        self.buffers.keys().copied().collect()
    }

    /// Generate a unique buffer name.  If `base` is not taken, returns it
    /// unchanged; otherwise appends `<2>`, `<3>`, ... until a free name is
    /// found.
    pub fn generate_new_buffer_name(&self, base: &str) -> String {
        if self.find_buffer_by_name(base).is_none() {
            return base.to_string();
        }
        let mut n = 2u64;
        loop {
            let candidate = format!("{}<{}>", base, n);
            if self.find_buffer_by_name(&candidate).is_none() {
                return candidate;
            }
            n += 1;
        }
    }

    /// Create a marker in `buffer_id` at byte position `pos` with the given
    /// insertion type.  Returns the new marker's id.
    pub fn create_marker(
        &mut self,
        buffer_id: BufferId,
        pos: usize,
        insertion_type: InsertionType,
    ) -> u64 {
        let marker_id = self.next_marker_id;
        self.next_marker_id += 1;
        if let Some(buf) = self.buffers.get_mut(&buffer_id) {
            let clamped = pos.min(buf.text.len());
            buf.markers.push(MarkerEntry {
                id: marker_id,
                byte_pos: clamped,
                insertion_type,
            });
        }
        marker_id
    }

    /// Query the current byte position of a marker.
    pub fn marker_position(&self, buffer_id: BufferId, marker_id: u64) -> Option<usize> {
        self.buffers.get(&buffer_id).and_then(|buf| {
            buf.markers
                .iter()
                .find(|m| m.id == marker_id)
                .map(|m| m.byte_pos)
        })
    }
}

impl Default for BufferManager {
    fn default() -> Self {
        Self::new()
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Helper: create a buffer with some text and correct zv.
    // -----------------------------------------------------------------------
    fn buf_with_text(text: &str) -> Buffer {
        let mut buf = Buffer::new(BufferId(1), "test".into());
        buf.text = GapBuffer::from_str(text);
        buf.zv = buf.text.len();
        buf
    }

    // -----------------------------------------------------------------------
    // Buffer creation & naming
    // -----------------------------------------------------------------------

    #[test]
    fn new_buffer_is_empty() {
        let buf = Buffer::new(BufferId(1), "*scratch*".into());
        assert_eq!(buf.name, "*scratch*");
        assert_eq!(buf.point(), 0);
        assert_eq!(buf.point_min(), 0);
        assert_eq!(buf.point_max(), 0);
        assert_eq!(buf.buffer_size(), 0);
        assert!(!buf.is_modified());
        assert!(!buf.read_only);
        assert!(buf.multibyte);
        assert!(buf.file_name.is_none());
        assert!(buf.mark().is_none());
    }

    #[test]
    fn buffer_id_equality() {
        let a = BufferId(1);
        let b = BufferId(1);
        let c = BufferId(2);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    // -----------------------------------------------------------------------
    // Point movement
    // -----------------------------------------------------------------------

    #[test]
    fn goto_char_clamps_to_accessible_region() {
        let mut buf = buf_with_text("hello");
        buf.goto_char(3);
        assert_eq!(buf.point(), 3);

        // Past end — clamped to zv.
        buf.goto_char(999);
        assert_eq!(buf.point(), buf.point_max());

        // Before start — clamped to begv.
        buf.goto_char(0);
        buf.begv = 2;
        buf.goto_char(0);
        assert_eq!(buf.point(), 2);
    }

    #[test]
    fn point_char_converts_byte_to_char_pos() {
        // "cafe\u{0301}" — 'e' + combining acute = 5 bytes, 5 chars in UTF-8
        let mut buf = buf_with_text("hello");
        buf.goto_char(3);
        assert_eq!(buf.point_char(), 3);
    }

    // -----------------------------------------------------------------------
    // Insertion
    // -----------------------------------------------------------------------

    #[test]
    fn insert_at_point_advances_point() {
        let mut buf = Buffer::new(BufferId(1), "test".into());
        // zv starts at 0 for an empty buffer; insert should extend it.
        buf.insert("hello");
        assert_eq!(buf.point(), 5);
        assert_eq!(buf.buffer_string(), "hello");
        assert_eq!(buf.buffer_size(), 5);
        assert!(buf.is_modified());
    }

    #[test]
    fn insert_in_middle() {
        let mut buf = buf_with_text("helo");
        buf.goto_char(3);
        buf.insert("l");
        assert_eq!(buf.buffer_string(), "hello");
        assert_eq!(buf.point(), 4);
    }

    #[test]
    fn insert_adjusts_mark() {
        let mut buf = buf_with_text("ab");
        buf.set_mark(1);
        buf.goto_char(0);
        buf.insert("X");
        // Mark was at 1, insert at 0 pushes it to 2.
        assert_eq!(buf.mark(), Some(2));
    }

    #[test]
    fn insert_empty_string_is_noop() {
        let mut buf = buf_with_text("hello");
        buf.goto_char(2);
        buf.insert("");
        assert_eq!(buf.buffer_string(), "hello");
        assert!(!buf.is_modified()); // still unmodified from initial state
    }

    // -----------------------------------------------------------------------
    // Deletion
    // -----------------------------------------------------------------------

    #[test]
    fn delete_region_basic() {
        let mut buf = buf_with_text("hello world");
        buf.goto_char(11); // at end
        buf.delete_region(5, 11);
        assert_eq!(buf.buffer_string(), "hello");
        assert_eq!(buf.point(), 5); // was past deleted range
    }

    #[test]
    fn delete_region_adjusts_point_inside() {
        let mut buf = buf_with_text("abcdef");
        buf.goto_char(3); // in middle of deleted range
        buf.delete_region(1, 5);
        assert_eq!(buf.point(), 1); // collapsed to start of deletion
        assert_eq!(buf.buffer_string(), "af");
    }

    #[test]
    fn delete_region_adjusts_mark() {
        let mut buf = buf_with_text("abcdef");
        buf.set_mark(4);
        buf.delete_region(1, 3);
        // mark was at 4, past deleted range end (3), so shifts by 2
        assert_eq!(buf.mark(), Some(2));
    }

    #[test]
    fn delete_region_adjusts_zv() {
        let mut buf = buf_with_text("abcdef");
        assert_eq!(buf.zv, 6);
        buf.delete_region(2, 4);
        assert_eq!(buf.zv, 4);
    }

    #[test]
    fn delete_empty_range_is_noop() {
        let mut buf = buf_with_text("hello");
        buf.delete_region(2, 2);
        assert_eq!(buf.buffer_string(), "hello");
    }

    // -----------------------------------------------------------------------
    // Substring / buffer_string
    // -----------------------------------------------------------------------

    #[test]
    fn buffer_substring_range() {
        let buf = buf_with_text("hello world");
        assert_eq!(buf.buffer_substring(6, 11), "world");
    }

    #[test]
    fn buffer_string_returns_accessible() {
        let mut buf = buf_with_text("hello world");
        buf.narrow_to_region(6, 11);
        assert_eq!(buf.buffer_string(), "world");
    }

    // -----------------------------------------------------------------------
    // char_after / char_before
    // -----------------------------------------------------------------------

    #[test]
    fn char_after_basic() {
        let buf = buf_with_text("hello");
        assert_eq!(buf.char_after(0), Some('h'));
        assert_eq!(buf.char_after(4), Some('o'));
        assert_eq!(buf.char_after(5), None);
    }

    #[test]
    fn char_before_basic() {
        let buf = buf_with_text("hello");
        assert_eq!(buf.char_before(0), None);
        assert_eq!(buf.char_before(1), Some('h'));
        assert_eq!(buf.char_before(5), Some('o'));
    }

    #[test]
    fn char_after_multibyte() {
        // Each Chinese character is 3 bytes in UTF-8.
        let buf = buf_with_text("\u{4f60}\u{597d}"); // "nihao" in Chinese
        assert_eq!(buf.char_after(0), Some('\u{4f60}'));
        assert_eq!(buf.char_after(3), Some('\u{597d}'));
    }

    #[test]
    fn char_before_multibyte() {
        let buf = buf_with_text("\u{4f60}\u{597d}");
        assert_eq!(buf.char_before(3), Some('\u{4f60}'));
        assert_eq!(buf.char_before(6), Some('\u{597d}'));
    }

    // -----------------------------------------------------------------------
    // Narrowing
    // -----------------------------------------------------------------------

    #[test]
    fn narrow_and_widen() {
        let mut buf = buf_with_text("hello world");
        buf.goto_char(8);
        buf.narrow_to_region(6, 11);
        assert_eq!(buf.point_min(), 6);
        assert_eq!(buf.point_max(), 11);
        assert_eq!(buf.buffer_size(), 5);
        assert_eq!(buf.buffer_string(), "world");
        // Point was 8 — still within [6, 11].
        assert_eq!(buf.point(), 8);

        buf.widen();
        assert_eq!(buf.point_min(), 0);
        assert_eq!(buf.point_max(), 11);
    }

    #[test]
    fn narrow_clamps_point() {
        let mut buf = buf_with_text("hello world");
        buf.goto_char(2);
        buf.narrow_to_region(5, 11);
        // Point 2 < begv 5 => clamped to 5.
        assert_eq!(buf.point(), 5);
    }

    // -----------------------------------------------------------------------
    // Markers
    // -----------------------------------------------------------------------

    #[test]
    fn marker_tracks_insertion_after() {
        let mut buf = buf_with_text("ab");
        buf.markers.push(MarkerEntry {
            id: 1,
            byte_pos: 1,
            insertion_type: InsertionType::After,
        });
        buf.goto_char(1);
        buf.insert("XY");
        // Marker was at 1 with After => advances to 3.
        assert_eq!(buf.markers[0].byte_pos, 3);
    }

    #[test]
    fn marker_stays_on_insertion_before() {
        let mut buf = buf_with_text("ab");
        buf.markers.push(MarkerEntry {
            id: 1,
            byte_pos: 1,
            insertion_type: InsertionType::Before,
        });
        buf.goto_char(1);
        buf.insert("XY");
        // Marker was at 1 with Before => stays at 1.
        assert_eq!(buf.markers[0].byte_pos, 1);
    }

    #[test]
    fn marker_adjusts_on_deletion() {
        let mut buf = buf_with_text("abcdef");
        buf.markers.push(MarkerEntry {
            id: 1,
            byte_pos: 4,
            insertion_type: InsertionType::After,
        });
        buf.delete_region(1, 3);
        // Marker was at 4 (past deleted range [1,3)), shifts by 2 => 2.
        assert_eq!(buf.markers[0].byte_pos, 2);
    }

    #[test]
    fn marker_inside_deleted_range_collapses() {
        let mut buf = buf_with_text("abcdef");
        buf.markers.push(MarkerEntry {
            id: 1,
            byte_pos: 2,
            insertion_type: InsertionType::After,
        });
        buf.delete_region(1, 5);
        // Marker at 2 inside [1,5) => collapses to 1.
        assert_eq!(buf.markers[0].byte_pos, 1);
    }

    // -----------------------------------------------------------------------
    // Buffer-local variables
    // -----------------------------------------------------------------------

    #[test]
    fn buffer_local_get_set() {
        let mut buf = Buffer::new(BufferId(1), "test".into());
        assert!(buf.get_buffer_local("tab-width").is_none());

        buf.set_buffer_local("tab-width", Value::Int(4));
        let val = buf.get_buffer_local("tab-width").unwrap();
        assert!(matches!(val, Value::Int(4)));

        buf.set_buffer_local("tab-width", Value::Int(8));
        let val = buf.get_buffer_local("tab-width").unwrap();
        assert!(matches!(val, Value::Int(8)));
    }

    #[test]
    fn buffer_local_multiple_vars() {
        let mut buf = Buffer::new(BufferId(1), "test".into());
        buf.set_buffer_local("fill-column", Value::Int(80));
        buf.set_buffer_local("major-mode", Value::symbol("text-mode"));

        assert!(buf.get_buffer_local("fill-column").is_some());
        assert!(buf.get_buffer_local("major-mode").is_some());
        assert!(buf.get_buffer_local("nonexistent").is_none());
    }

    // -----------------------------------------------------------------------
    // Modified flag
    // -----------------------------------------------------------------------

    #[test]
    fn modified_flag() {
        let mut buf = Buffer::new(BufferId(1), "test".into());
        assert!(!buf.is_modified());
        buf.insert("x");
        assert!(buf.is_modified());
        buf.set_modified(false);
        assert!(!buf.is_modified());
    }

    // -----------------------------------------------------------------------
    // BufferManager — creation, lookup, kill
    // -----------------------------------------------------------------------

    #[test]
    fn manager_starts_with_scratch() {
        let mgr = BufferManager::new();
        let scratch = mgr.find_buffer_by_name("*scratch*");
        assert!(scratch.is_some());
        assert!(mgr.current_buffer().is_some());
        assert_eq!(mgr.current_buffer().unwrap().name, "*scratch*");
    }

    #[test]
    fn manager_create_and_lookup() {
        let mut mgr = BufferManager::new();
        let id = mgr.create_buffer("foo.el");
        assert!(mgr.get(id).is_some());
        assert_eq!(mgr.get(id).unwrap().name, "foo.el");
        assert_eq!(mgr.find_buffer_by_name("foo.el"), Some(id));
        assert_eq!(mgr.find_buffer_by_name("bar.el"), None);
    }

    #[test]
    fn manager_set_current() {
        let mut mgr = BufferManager::new();
        let a = mgr.create_buffer("a");
        let b = mgr.create_buffer("b");
        mgr.set_current(a);
        assert_eq!(mgr.current_buffer().unwrap().name, "a");
        mgr.set_current(b);
        assert_eq!(mgr.current_buffer().unwrap().name, "b");
    }

    #[test]
    fn manager_kill_buffer() {
        let mut mgr = BufferManager::new();
        let id = mgr.create_buffer("doomed");
        assert!(mgr.kill_buffer(id));
        assert!(mgr.get(id).is_none());
        assert!(!mgr.kill_buffer(id)); // already dead
    }

    #[test]
    fn manager_kill_current_clears_current() {
        let mut mgr = BufferManager::new();
        let scratch = mgr.find_buffer_by_name("*scratch*").unwrap();
        mgr.set_current(scratch);
        mgr.kill_buffer(scratch);
        assert!(mgr.current_buffer().is_none());
    }

    #[test]
    fn manager_buffer_list() {
        let mut mgr = BufferManager::new();
        mgr.create_buffer("a");
        mgr.create_buffer("b");
        // *scratch* + a + b = 3
        assert_eq!(mgr.buffer_list().len(), 3);
    }

    #[test]
    fn manager_generate_new_buffer_name_unique() {
        let mgr = BufferManager::new();
        // "*scratch*" is taken, "foo" is not.
        assert_eq!(mgr.generate_new_buffer_name("foo"), "foo");
        assert_eq!(
            mgr.generate_new_buffer_name("*scratch*"),
            "*scratch*<2>"
        );
    }

    #[test]
    fn manager_generate_new_buffer_name_increments() {
        let mut mgr = BufferManager::new();
        mgr.create_buffer("buf");
        assert_eq!(mgr.generate_new_buffer_name("buf"), "buf<2>");
        mgr.create_buffer("buf<2>");
        assert_eq!(mgr.generate_new_buffer_name("buf"), "buf<3>");
    }

    // -----------------------------------------------------------------------
    // BufferManager — markers
    // -----------------------------------------------------------------------

    #[test]
    fn manager_create_and_query_marker() {
        let mut mgr = BufferManager::new();
        let id = mgr.create_buffer("m");
        // Insert some text so there is room for a marker.
        mgr.get_mut(id).unwrap().text = GapBuffer::from_str("abcdef");
        mgr.get_mut(id).unwrap().zv = 6;

        let mid = mgr.create_marker(id, 3, InsertionType::After);
        assert_eq!(mgr.marker_position(id, mid), Some(3));
    }

    #[test]
    fn manager_marker_clamped_to_buffer_len() {
        let mut mgr = BufferManager::new();
        let id = mgr.create_buffer("m");
        // Buffer is empty (len = 0), marker at 100 should be clamped.
        let mid = mgr.create_marker(id, 100, InsertionType::Before);
        assert_eq!(mgr.marker_position(id, mid), Some(0));
    }

    #[test]
    fn manager_marker_nonexistent_buffer() {
        let mgr = BufferManager::new();
        let pos = mgr.marker_position(BufferId(9999), 1);
        assert_eq!(pos, None);
    }

    // -----------------------------------------------------------------------
    // BufferManager — current_buffer_mut
    // -----------------------------------------------------------------------

    #[test]
    fn manager_current_buffer_mut_insert() {
        let mut mgr = BufferManager::new();
        mgr.current_buffer_mut().unwrap().insert("hello");
        assert_eq!(mgr.current_buffer().unwrap().buffer_string(), "hello");
    }

    // -----------------------------------------------------------------------
    // Integration: multiple operations
    // -----------------------------------------------------------------------

    #[test]
    fn integration_edit_narrow_widen() {
        let mut buf = Buffer::new(BufferId(1), "work".into());
        buf.insert("abcdefghij");
        assert_eq!(buf.buffer_string(), "abcdefghij");

        buf.narrow_to_region(2, 8);
        assert_eq!(buf.buffer_string(), "cdefgh");

        buf.goto_char(5);
        buf.insert("XX");
        assert_eq!(buf.buffer_string(), "cdeXXfgh");

        buf.widen();
        assert_eq!(buf.buffer_string(), "abcdeXXfghij");
    }
}
