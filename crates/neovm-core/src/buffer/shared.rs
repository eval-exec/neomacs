use std::cell::{Cell, RefCell};
use std::rc::Rc;

use super::CharPos0;
use super::buffer::BufferId;
use crate::emacs_core::value::Value;

/// The point GNU saves for a possible undo point entry, together with the
/// buffer it was saved in.
///
/// GNU keeps these as the pair of globals `point_before_last_command_or_undo`
/// and `buffer_before_last_command_or_undo` (src/keyboard.c:232-233).  Both
/// assignment sites write them together -- the command loop
/// (src/keyboard.c:1536-1537) and `Fundo_boundary` (src/undo.c:278-279) -- and
/// `record_point` reads them together, refusing the entry unless the buffer
/// still matches (src/undo.c:73-78).  Keeping them in one value is what makes
/// "saved the point, forgot the buffer" unrepresentable; a bare `CharPos0` let
/// a point saved in one buffer be spent on an edit in another (ledger 121: an
/// indirect buffer's point spent on its base).
///
/// It is private on purpose: the pair is only reachable through
/// [`SavedPointBeforeCommand`], whose accessors are GNU's paired write and
/// GNU's paired read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PointBeforeCommand {
    /// The buffer that was current when the point was saved.
    buffer: BufferId,
    /// `point_before_last_command_or_undo`.
    point: CharPos0,
}

/// The editor's ONE saved point-before-command-or-undo.
///
/// GNU's two globals are singular: keyboard.h documents
/// `point_before_last_command_or_undo` as "the location of point immediately
/// before **the last command** was executed, or the last time the
/// undo-boundary command added a boundary" (src/keyboard.h:257-266).  There is
/// one of them for the whole editor, and both assignment sites overwrite it
/// unconditionally with whatever buffer happens to be current
/// (src/keyboard.c:1536-1537, src/undo.c:278-279).  So a command-loop
/// iteration in ANY buffer -- every minibuffer keystroke of an `M-x` read is
/// one -- supersedes the point saved for every other buffer, and
/// `record_point`'s buffer guard (src/undo.c:73-78) is what turns that into a
/// dropped point entry.
///
/// Modelling it as per-buffer state cannot express "superseded": a buffer's own
/// saved point always names that buffer, so the guard degenerates into a
/// tautology and the entry is always recorded.  Every buffer holds a clone of
/// the one `Rc` below instead, so writing a saved point necessarily discards
/// the previous one, exactly as assigning a C global does.  Nothing has to be
/// kept in sync, and `Buffer::clone`/`swap-text` cannot fork the cell.
///
/// The only read accessor takes the buffer that is about to record the entry,
/// so GNU's third guard cannot be forgotten at a call site: there is no way to
/// obtain the point without naming a buffer to check it against.
#[derive(Clone)]
pub struct SavedPointBeforeCommand {
    cell: Rc<Cell<Option<PointBeforeCommand>>>,
}

impl SavedPointBeforeCommand {
    /// Mint the editor's single saved-point cell.
    ///
    /// Production code calls this exactly where GNU's globals come into being
    /// -- once per editor, in a [`super::buffer::BufferManager`] constructor.
    /// Every buffer that manager owns is handed a `clone()` of the result.
    pub fn new_editor_global() -> Self {
        Self {
            cell: Rc::new(Cell::new(None)),
        }
    }

    /// GNU's paired assignment: `point_before_last_command_or_undo = PT;
    /// buffer_before_last_command_or_undo = current_buffer;`
    /// (src/keyboard.c:1536-1537, src/undo.c:278-279).
    pub(in crate::buffer) fn save(&self, buffer: BufferId, point: CharPos0) {
        self.cell.set(Some(PointBeforeCommand { buffer, point }));
    }

    /// GNU's paired read (src/undo.c:73-78): the saved point is usable only by
    /// the buffer it was saved in.  Returns `None` when the last command ran
    /// somewhere else -- "we must not do this if the buffer has changed since
    /// the last command, since the value of point that we have will be for that
    /// buffer, not this".
    pub(in crate::buffer) fn point_saved_in(&self, buffer: BufferId) -> Option<CharPos0> {
        self.cell
            .get()
            .filter(|saved| saved.buffer == buffer)
            .map(|saved| saved.point)
    }

    /// True when both handles name the same editor-global cell.
    pub(crate) fn shares_cell_with(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.cell, &other.cell)
    }
}

#[derive(Clone)]
pub struct SharedUndoState {
    inner: Rc<RefCell<SharedUndoStateInner>>,
}

#[derive(Clone)]
struct SharedUndoStateInner {
    list: Value,
    in_progress: bool,
    recorded_first_change: bool,
    /// GNU `BUF_COMPACT` (`src/buffer.h:139,267`): the modification tick this
    /// text had when `compact_buffer` last ran on it.  Zero-initialised like
    /// GNU's calloc'd `struct buffer_text`, so the first collection after a
    /// buffer is created always compacts it (modification ticks start at 1).
    compacted_modified_tick: i64,
}

impl Default for SharedUndoState {
    fn default() -> Self {
        Self::new()
    }
}

impl SharedUndoState {
    pub fn new() -> Self {
        Self::from_parts(Value::NIL, false, false)
    }

    pub fn from_parts(list: Value, in_progress: bool, recorded_first_change: bool) -> Self {
        Self {
            inner: Rc::new(RefCell::new(SharedUndoStateInner {
                list,
                in_progress,
                recorded_first_change,
                compacted_modified_tick: 0,
            })),
        }
    }

    pub fn shares_with(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.inner, &other.inner)
    }

    pub fn list(&self) -> Value {
        self.inner.borrow().list
    }

    pub fn set_list(&self, list: Value) {
        self.inner.borrow_mut().list = list;
    }

    pub fn in_progress(&self) -> bool {
        self.inner.borrow().in_progress
    }

    pub fn set_in_progress(&self, in_progress: bool) {
        self.inner.borrow_mut().in_progress = in_progress;
    }

    /// GNU `BUF_COMPACT (b)` — the modification tick at the last compaction.
    pub fn compacted_modified_tick(&self) -> i64 {
        self.inner.borrow().compacted_modified_tick
    }

    /// GNU `BUF_COMPACT (buffer) = BUF_MODIFF (buffer)` (`src/buffer.c:1884`).
    pub fn set_compacted_modified_tick(&self, tick: i64) {
        self.inner.borrow_mut().compacted_modified_tick = tick;
    }

    pub fn recorded_first_change(&self) -> bool {
        self.inner.borrow().recorded_first_change
    }

    pub fn set_recorded_first_change(&self, recorded_first_change: bool) {
        self.inner.borrow_mut().recorded_first_change = recorded_first_change;
    }

    pub fn trace_roots(&self, roots: &mut Vec<Value>) {
        roots.push(self.list());
    }
}
