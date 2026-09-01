//! Undo system for buffers — GNU Emacs–compatible Lisp list approach.
//!
//! The undo list is stored as a direct Lisp `Value` in the buffer-local
//! property `buffer-undo-list`.  This module provides helper functions
//! that manipulate that `Value` list, matching GNU Emacs's undo.c:
//!
//! - `t` means undo is disabled
//! - `nil` means undo is enabled with an empty list
//! - Records are cons-ed onto the FRONT (most recent first)
//!
//! Entry types:
//! - `(BEG . END)` — insertion (1-indexed positions)
//! - `(TEXT . POS)` — deletion (TEXT is string, POS is 1-indexed,
//!   negative if point was at end of deleted region)
//! - `POS` (integer) — cursor position (1-indexed)
//! - `(t . MODTIME)` — first-change marker
//! - `nil` — undo boundary

use crate::emacs_core::eval::{
    push_scratch_gc_root, restore_scratch_gc_roots, save_scratch_gc_roots,
};
use crate::emacs_core::value::{Value, ValueKind};
use crate::heap_types::LispString;

use super::{CharLen, CharPos0, CharRange};

fn prepend_undo_entry(undo_list: &mut Value, entry: Value) {
    let saved = save_scratch_gc_roots();
    push_scratch_gc_root(*undo_list);
    push_scratch_gc_root(entry);
    *undo_list = Value::cons(entry, *undo_list);
    restore_scratch_gc_roots(saved);
}

/// The two states GNU's `buffer-undo-list` slot can be in.
///
/// The slot holds either the symbol `t` -- undo turned off -- or a list of
/// undo records, which may be nil for "on, but nothing recorded yet".  GNU
/// tests this domain by hand at every decision point (`EQ (..., Qt)` in
/// `record_insert` src/undo.c:91, `Fbuffer_enable_undo` src/buffer.c:1846,
/// `compact_buffer` src/buffer.c:1869).  Naming it once makes each caller's
/// branch exhaustive at compile time, so a new state cannot be forgotten and
/// "on with an empty history" cannot be confused with "off".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UndoRecording {
    /// `buffer-undo-list` is `t`: changes are not recorded.
    Disabled,
    /// `buffer-undo-list` is a list of records, possibly empty (nil).
    Enabled,
}

impl UndoRecording {
    /// Classify a `buffer-undo-list` value.
    pub fn of(undo_list: &Value) -> Self {
        if undo_list.is_t() {
            Self::Disabled
        } else {
            Self::Enabled
        }
    }
}

/// Returns `true` when `buffer-undo-list` is `t` (undo disabled).
pub fn undo_list_is_disabled(undo_list: &Value) -> bool {
    matches!(UndoRecording::of(undo_list), UndoRecording::Disabled)
}

/// True when `buffer-undo-list` currently sits at an undo boundary: it is
/// empty, or its newest entry is the `nil` boundary.
///
/// GNU `record_point` (`src/undo.c:59-61`) reads exactly this, and reads it
/// *before* `record_first_change` may cons `(t . TIME)` onto the list.  The
/// answer is therefore a fact about the list as the command found it, which
/// is why it is read once by [`crate::buffer::Buffer::undo_prepare_change`]
/// rather than re-derived by each recorder.
pub fn undo_list_at_boundary(undo_list: &Value) -> bool {
    undo_list.is_nil() || (undo_list.is_cons() && undo_list.cons_car().is_nil())
}

/// Record that text was inserted at character position `beg` with character
/// length `len`.  Positions stored in the list are 1-indexed.
///
/// The caller must already have run the GNU `record_point` prologue (see
/// [`crate::buffer::Buffer::undo_prepare_change`]); this records only the
/// insertion itself, exactly like GNU's `record_insert` body after its
/// `record_point (beg)` call.
///
/// Consecutive adjacent inserts are merged when the head entry is an
/// insert whose END equals `beg+1` (the 1-indexed start of the new
/// insert), and only then: an insert that ends where the head entry begins
/// stays its own record, exactly as in GNU `record_insert`.
pub fn undo_list_record_insert(undo_list: &mut Value, beg: CharPos0, len: CharLen) {
    // GNU `record_insert` (undo.c) returns early only for a disabled undo
    // list; a zero-length insertion still conses `(BEG . BEG)`.  That record
    // is load-bearing, because `record_insert` coalesces into the newest
    // record only when that record is an insertion ending where the new one
    // begins — a zero-length record breaks the chain between two adjacent
    // change runs.
    if undo_list_is_disabled(undo_list) {
        return;
    }

    let beg1 = beg.to_lisp().as_i64();
    let end1 = beg.add_len(len).to_lisp().as_i64();

    // GNU `record_insert` (undo.c:98-112) coalesces in exactly ONE direction:
    // into a newest record that is an insertion ENDING where this insertion
    // BEGINS.  There is deliberately no reverse rule.  `primitive-undo` replays
    // the records newest-first and each deletion reshapes the buffer the later
    // records are read against, so two insertions made back-to-front --
    // the ordinary shape when a client applies a server's edit list in reverse
    // to keep earlier positions valid -- must stay two records.  Merging them
    // would claim the untouched text between them as newly inserted and delete
    // it on undo.
    if undo_list.is_cons() {
        let head = undo_list.cons_car();
        if head.is_cons() {
            let car = head.cons_car();
            let cdr = head.cons_cdr();
            if let (Some(_prev_beg), Some(prev_end)) = (car.as_fixnum(), cdr.as_fixnum())
                && prev_end == beg1
            {
                // Merge: extend the existing insert entry.
                head.set_cdr(Value::fixnum(prev_end + len.get() as i64));
                return;
            }
        }
    }

    let saved = save_scratch_gc_roots();
    push_scratch_gc_root(*undo_list);
    let entry = Value::cons(Value::fixnum(beg1), Value::fixnum(end1));
    push_scratch_gc_root(entry);
    *undo_list = Value::cons(entry, *undo_list);
    restore_scratch_gc_roots(saved);
}

/// Record a deletion.  `beg` is the 0-indexed character position, `text` is
/// the deleted string, `pt` is the 0-indexed cursor character position at
/// the time of deletion.
///
/// The stored position is 1-indexed and negative when `pt` was at the
/// END of the deleted region (i.e. `pt == beg + SCHARS (text)`).
///
/// The caller must already have run the GNU `record_point` prologue (see
/// [`crate::buffer::Buffer::undo_prepare_change`]), which is also what keeps
/// the point entry ahead of any `(MARKER . ADJUSTMENT)` entries (GNU bug
/// 16818 ordering).
pub fn undo_list_record_delete(
    undo_list: &mut Value,
    beg: CharPos0,
    text: LispString,
    pt: CharPos0,
) {
    // GNU `record_delete` (undo.c) returns early only for a disabled undo
    // list; it never tests the string's length, so a zero-length deletion
    // still conses `("" . POS)`.  See [`undo_list_record_insert`] for why an
    // empty record matters.
    if undo_list_is_disabled(undo_list) {
        return;
    }

    let pos1 = beg.to_lisp().as_i64();
    let stored_pos = if pt == beg.add_len(CharLen::new(text.schars())) {
        -pos1
    } else {
        pos1
    };

    let saved = save_scratch_gc_roots();
    push_scratch_gc_root(*undo_list);
    let text = Value::heap_string(text);
    push_scratch_gc_root(text);
    let entry = Value::cons(text, Value::fixnum(stored_pos));
    push_scratch_gc_root(entry);
    *undo_list = Value::cons(entry, *undo_list);
    restore_scratch_gc_roots(saved);
}

/// Record a marker adjustment immediately before a deletion record.
///
/// GNU `record_marker_adjustments` conses `(MARKER . ADJUSTMENT)` entries
/// before `record_delete` conses `(TEXT . POS)`, so the final undo list has
/// the deletion first followed by its marker adjustments.
pub fn undo_list_record_marker_adjustment(undo_list: &mut Value, marker: Value, adjustment: i64) {
    if undo_list_is_disabled(undo_list) || adjustment == 0 {
        return;
    }

    let saved = save_scratch_gc_roots();
    push_scratch_gc_root(*undo_list);
    push_scratch_gc_root(marker);
    let entry = Value::cons(marker, Value::fixnum(adjustment));
    push_scratch_gc_root(entry);
    *undo_list = Value::cons(entry, *undo_list);
    restore_scratch_gc_roots(saved);
}

/// Record the cursor position (0-indexed `pt`) as a 1-indexed integer.
/// Skips if the most recent entry is the same position.
pub fn undo_list_record_point(undo_list: &mut Value, pt: CharPos0) {
    if undo_list_is_disabled(undo_list) {
        return;
    }
    let pt1 = Value::fixnum(pt.to_lisp().as_i64());

    // Don't record consecutive identical positions.
    if undo_list.is_cons() {
        let head = undo_list.cons_car();
        if head == pt1 {
            return;
        }
    }

    prepend_undo_entry(undo_list, pt1);
}

/// Record a text-property change: `(nil PROP VAL BEG . END)`.
///
/// `prop` is the property name (symbol), `val` is the OLD value before
/// the change (so that undoing restores it), `beg` and `end` are
/// 0-indexed character positions; they are stored as 1-indexed integers.
pub fn undo_list_record_property_change(
    undo_list: &mut Value,
    prop: Value,
    val: Value,
    range: CharRange,
) {
    if undo_list_is_disabled(undo_list) || range.is_empty() {
        return;
    }
    let saved = save_scratch_gc_roots();
    push_scratch_gc_root(*undo_list);
    push_scratch_gc_root(prop);
    push_scratch_gc_root(val);
    let beg1 = Value::fixnum(range.start().to_lisp().as_i64());
    let end1 = Value::fixnum(range.end().to_lisp().as_i64());
    // Build (nil PROP VAL BEG . END)
    let inner = Value::cons(beg1, end1);
    push_scratch_gc_root(inner);
    let inner = Value::cons(val, inner);
    push_scratch_gc_root(inner);
    let inner = Value::cons(prop, inner);
    push_scratch_gc_root(inner);
    let entry = Value::cons(Value::NIL, inner);
    push_scratch_gc_root(entry);
    *undo_list = Value::cons(entry, *undo_list);
    restore_scratch_gc_roots(saved);
}

/// Record the first-change sentinel `(t . VISITED-FILE-MODTIME)`.
///
/// GNU `record_first_change` (`src/undo.c:209-223`) records the buffer's
/// visited-file modtime, not a placeholder:
///
/// ```c
/// bset_undo_list (current_buffer,
///                 Fcons (Fcons (Qt, buffer_visited_file_modtime (base_buffer)),
///                        BVAR (current_buffer, undo_list)));
/// ```
///
/// The datum is what makes the entry *mean* anything: `primitive-undo`'s
/// `(t . TIME)` arm (`lisp/simple.el:3669-3688`) clears the modified flag only
/// when `(time-equal-p time (visited-file-modtime))`, so that undoing back to
/// a save that has since been superseded on disk does NOT claim the buffer is
/// unmodified.  Recording a constant made every such comparison fail for a
/// file-visiting buffer, and `undo` back to the saved text left
/// `buffer-modified-p` t where GNU reports nil.
///
/// GNU resolves the buffer to read the modtime FROM before reading it -- the
/// base buffer when the change happens in an indirect buffer
/// (`src/undo.c:213-214`) -- so this takes a
/// [`FirstChangeModtime`](crate::buffer::FirstChangeModtime), which only
/// [`Buffer::first_change_modtime`](crate::buffer::Buffer::first_change_modtime)
/// can mint.  Passing the current buffer's own
/// `visited-file-modtime` is therefore not expressible here; it was the whole
/// of ledger 105's residual.
pub fn undo_list_record_first_change(
    undo_list: &mut Value,
    first_change_modtime: super::visited_file_modtime::FirstChangeModtime,
) {
    if undo_list_is_disabled(undo_list) {
        return;
    }
    let saved = save_scratch_gc_roots();
    push_scratch_gc_root(*undo_list);
    // Rooted first: a `Known` modtime conses a four-element timestamp, and the
    // list this entry is about to head must survive that allocation.
    let visited_file_modtime = first_change_modtime.to_lisp_value();
    push_scratch_gc_root(visited_file_modtime);
    let entry = Value::cons(Value::T, visited_file_modtime);
    push_scratch_gc_root(entry);
    *undo_list = Value::cons(entry, *undo_list);
    restore_scratch_gc_roots(saved);
}

/// Return true if LIST contains a GNU first-change sentinel `(t . MODTIME)`.
pub fn undo_list_contains_first_change(undo_list: &Value) -> bool {
    let mut cursor = *undo_list;
    while cursor.is_cons() {
        let entry = cursor.cons_car();
        if entry.is_cons() && entry.cons_car().is_t() {
            return true;
        }
        cursor = cursor.cons_cdr();
    }
    false
}

/// Insert an undo boundary (`nil`).  Skips if the list is empty/nil or
/// already starts with a nil boundary.
pub fn undo_list_boundary(undo_list: &mut Value) {
    if undo_list_is_disabled(undo_list) {
        return;
    }
    // Don't add boundary to empty list or if head is already nil.
    if undo_list.is_nil() {
        return;
    }
    if undo_list.is_cons() && undo_list.cons_car().is_nil() {
        return;
    }
    prepend_undo_entry(undo_list, Value::NIL);
}

// No `undo_list_pop_group', `undo_list_is_empty' or
// `undo_list_contains_boundary' here.  All three existed only for a Rust
// replay loop (`BufferManager::undo_buffer') that preloaded Lisp shadowed and
// entry 150 deleted; grouping the list for replay is `primitive-undo''s job
// (lisp/simple.el:3645), and it is Lisp.  This module RECORDS.

/// Check whether the most recent entry is a nil boundary.
pub fn undo_list_has_trailing_boundary(undo_list: &Value) -> bool {
    undo_list.is_cons() && undo_list.cons_car().is_nil()
}

// ---------------------------------------------------------------------------
// Truncation (GNU `truncate_undo_list', src/undo.c:284-419)
// ---------------------------------------------------------------------------

/// `sizeof (struct Lisp_Cons)` on a 64-bit build — the unit GNU charges for
/// every list link and every record cons (`src/undo.c:316,335,338`).
const GNU_CONS_BYTES: i64 = 16;

/// `sizeof (struct Lisp_String) - 1` on a 64-bit build (four pointer-sized
/// fields: size, size_byte, intervals, data), the constant part of the charge
/// GNU makes for a saved deletion string (`src/undo.c:340-341`).
const GNU_STRING_HEADER_BYTES: i64 = 31;

/// Bytes GNU charges for one undo list element plus its chain link.
///
/// `src/undo.c:334-342`: the link is always one cons; a cons-shaped record
/// costs a second cons; and a record whose car is a string (a recorded
/// deletion) additionally costs the string header plus one byte per
/// *character* (`SCHARS`, not bytes).
fn undo_element_bytes(element: Value) -> i64 {
    let mut size = GNU_CONS_BYTES;
    if element.is_cons() {
        size += GNU_CONS_BYTES;
        let car = element.cons_car();
        if matches!(car.kind(), ValueKind::String) {
            let chars = car.as_lisp_string().map(|s| s.schars()).unwrap_or(0);
            size += GNU_STRING_HEADER_BYTES + chars as i64;
        }
    }
    size
}

/// The value of a GNU `DEFVAR_INT` variable.
///
/// `store_symval_forwarding` (`src/data.c:1475-1483`) rejects anything that is
/// not an integer fitting `intmax_t`, so the C slot behind `undo-limit` can
/// only ever hold an `intmax_t`.  Both size limits are now
/// `forward::LispIntFwd` slots here too, so `NotAnIntSlotValue` is no longer
/// reachable through them; it stays because this reader is also pointed at
/// values that reach it from somewhere other than the forwarded slot, and a
/// silent substitute number is the outcome worth keeping impossible.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GnuIntVariable {
    /// An integer that fits `intmax_t`, exactly as GNU's slot would hold it.
    Int(i64),
    /// Not something GNU's `DEFVAR_INT` slot could contain.
    NotAnIntSlotValue,
}

impl GnuIntVariable {
    fn of(value: Value) -> Self {
        match value.kind() {
            ValueKind::Fixnum(n) => Self::Int(n),
            // GNU's `integer_to_intmax` accepts a bignum that fits the slot,
            // and a forwarded slot can hold one.
            _ => match value.as_bignum().and_then(|big| i64::try_from(big).ok()) {
                Some(n) => Self::Int(n),
                None => Self::NotAnIntSlotValue,
            },
        }
    }
}

/// What `undo-outer-limit` says about a single command's undo record.
///
/// Mirrors the three outcomes GNU's guard at `src/undo.c:352-356` can produce
/// for a `DEFVAR_LISP` variable that is documented to hold "a size, or nil for
/// no limit".
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OuterUndoLimit {
    /// `undo-outer-limit` is not an integer (nil is the documented "no limit"
    /// value, and `--batch` installs it — `src/emacs.c:1700-1707`), or it is a
    /// positive bignum too large for `intmax_t`, for which `Fnatnump` is
    /// non-nil and GNU's guard stays false.
    NoLimit,
    /// An integer that fits `intmax_t`: exceeded when the first undo group is
    /// strictly larger.
    Bytes(i64),
    /// A negative integer too large in magnitude for `intmax_t`:
    /// `integer_to_intmax` fails and `Fnatnump` is nil, so GNU's guard is
    /// unconditionally true.
    AlwaysExceeded,
}

impl OuterUndoLimit {
    fn of(value: Value) -> Self {
        match value.kind() {
            ValueKind::Fixnum(n) => Self::Bytes(n),
            _ => match value.as_bignum() {
                Some(big) if *big < 0 => Self::AlwaysExceeded,
                // A bignum that big is not a limit any undo record can reach,
                // and a non-integer fails GNU's `INTEGERP` guard outright.
                _ => Self::NoLimit,
            },
        }
    }

    fn is_exceeded_by(self, size_so_far: i64) -> bool {
        match self {
            Self::NoLimit => false,
            Self::Bytes(limit) => limit < size_so_far,
            Self::AlwaysExceeded => true,
        }
    }
}

/// The four Lisp variables `truncate_undo_list` consults.
///
/// GNU reads them only *after* `set_buffer_internal (b)` (`src/undo.c:296-306`
/// — "Make the buffer current to get its local values of variables such as
/// undo_limit"), so what governs a buffer's truncation is the binding visible
/// in that buffer: its own buffer-local value, or a `let` binding, or the
/// global default, in the ordinary Lisp order.
///
/// Implementing this trait is the *only* way to obtain an [`UndoLimits`], so a
/// caller cannot truncate with a number nobody configured.
pub trait UndoLimitBindings {
    fn undo_limit(&self) -> Value;
    fn undo_strong_limit(&self) -> Value;
    fn undo_outer_limit(&self) -> Value;
    fn undo_outer_limit_function(&self) -> Value;
}

/// One buffer's truncation limits, read at the moment GNU reads them.
#[derive(Clone, Copy, Debug)]
pub struct UndoLimits {
    /// `undo-limit`: truncate *after* the boundary that pushes the list past
    /// this size (`src/undo.c:380-390`).
    limit: i64,
    /// `undo-strong-limit`: truncate *before* it instead (`src/undo.c:386`).
    strong_limit: i64,
    outer: OuterUndoLimit,
    outer_function: Value,
}

impl UndoLimits {
    /// Read the limits through BINDINGS, which must already be positioned on
    /// the buffer being truncated.
    ///
    /// `None` means `undo-limit` or `undo-strong-limit` holds something GNU's
    /// `DEFVAR_INT` slot could never hold; there is no defensible number to
    /// truncate with in that state, so the caller leaves the list alone.
    pub fn read(bindings: &impl UndoLimitBindings) -> Option<Self> {
        let (GnuIntVariable::Int(limit), GnuIntVariable::Int(strong_limit)) = (
            GnuIntVariable::of(bindings.undo_limit()),
            GnuIntVariable::of(bindings.undo_strong_limit()),
        ) else {
            return None;
        };
        Some(Self {
            limit,
            strong_limit,
            outer: OuterUndoLimit::of(bindings.undo_outer_limit()),
            outer_function: bindings.undo_outer_limit_function(),
        })
    }

    /// The function GNU offers the oversized record to, when there is both an
    /// exceeded `undo-outer-limit` and an `undo-outer-limit-function`
    /// (`src/undo.c:352-356`). `None` means GNU would not call anything.
    pub fn outer_limit_function_for(self, first_group_bytes: i64) -> Option<Value> {
        if !self.outer.is_exceeded_by(first_group_bytes) || self.outer_function.is_nil() {
            return None;
        }
        Some(self.outer_function)
    }
}

/// Bytes occupied by the leading boundary plus the most recent undo record —
/// the size GNU has accumulated when it tests `undo-outer-limit`, and the
/// number it passes to `undo-outer-limit-function` (`src/undo.c:312-361`).
pub fn undo_first_group_bytes(undo_list: Value) -> i64 {
    let mut size_so_far = 0;
    let mut next = undo_list;

    if next.is_cons() && next.cons_car().is_nil() {
        size_so_far += GNU_CONS_BYTES;
        next = next.cons_cdr();
    }
    while next.is_cons() && !next.cons_car().is_nil() {
        size_so_far += undo_element_bytes(next.cons_car());
        next = next.cons_cdr();
    }
    size_so_far
}

/// Truncate an undo list at the end, returning the truncated list.
///
/// A direct port of GNU `truncate_undo_list` (`src/undo.c:289-419`) minus its
/// `undo-outer-limit-function` call, which needs the evaluator and is made by
/// the caller between [`undo_first_group_bytes`] and this call.
///
/// The shape that matters, and that a "cut as soon as the running total
/// exceeds the limit" loop does not have: the most recent record is always
/// kept whole, and every cut lands on a group edge, so an undo list is never
/// left holding half of a command's changes.
pub fn truncate_undo_list(undo_list: Value, limits: &UndoLimits) -> Value {
    let mut size_so_far: i64 = 0;
    let mut prev = Value::NIL;
    let mut next = undo_list;
    let mut last_boundary = Value::NIL;

    // If the first element is an undo boundary, skip past it.
    if next.is_cons() && next.cons_car().is_nil() {
        size_so_far += GNU_CONS_BYTES;
        prev = next;
        next = next.cons_cdr();
    }

    // Always preserve at least the most recent undo record.
    while next.is_cons() && !next.cons_car().is_nil() {
        size_so_far += undo_element_bytes(next.cons_car());
        prev = next;
        next = next.cons_cdr();
    }

    if next.is_cons() {
        last_boundary = prev;
    }

    // Keep additional undo data, if it fits in the limits.
    while next.is_cons() {
        let element = next.cons_car();
        // At a boundary, decide whether to truncate before or after it: the
        // lower threshold truncates after, the higher one before.
        if element.is_nil() {
            if size_so_far > limits.strong_limit {
                break;
            }
            last_boundary = prev;
            if size_so_far > limits.limit {
                break;
            }
        }
        size_so_far += undo_element_bytes(element);
        prev = next;
        next = next.cons_cdr();
    }

    if next.is_nil() {
        // The whole list fits; leave it exactly as it is.
        undo_list
    } else if last_boundary.is_cons() {
        last_boundary.set_cdr(Value::NIL);
        undo_list
    } else {
        // Nothing was worth keeping.
        Value::NIL
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
#[path = "undo_test.rs"]
mod tests;
