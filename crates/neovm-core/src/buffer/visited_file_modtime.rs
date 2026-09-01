//! A buffer's record of its visited file's modification time, and the one
//! place GNU reads a *different* buffer's copy of it.
//!
//! GNU keeps `modtime` in `struct buffer` (`src/buffer.h:645-655`), so a base
//! buffer and each of its indirect buffers have their own.  Only two readers
//! exist, and they disagree on purpose:
//!
//! * `Fvisited_file_modtime` (`src/fileio.c:6165-6175`) reports
//!   `current_buffer`'s own, which for an indirect buffer is the unknown
//!   sentinel `reset_buffer` left there (`src/buffer.c:1092`) -- an indirect
//!   buffer visits no file.
//! * `record_first_change` (`src/undo.c:209-223`) redirects to the BASE buffer
//!   before reading, because the `(t . TIME)` entry it conses exists to name
//!   the save the undo would return the *text* to, and the text's file belongs
//!   to the base.
//!
//! Both facts are observable under GNU Emacs 31.0.90: in an indirect buffer
//! whose base visits a file, `(visited-file-modtime)` is `0` while the
//! first-change entry holds the base's timestamp -- and it still holds the
//! base's timestamp after `(set-visited-file-modtime '(1 2 3 4))` gives the
//! indirect buffer a modtime of its own.  The redirect is unconditional, not a
//! fallback for a missing value.
//!
//! [`VisitedFileModtimeSlot`] is that pair of readers as a type: the buffer's
//! own cell, plus the base's cell when this is an indirect buffer.  The undo
//! recorder can only obtain a [`FirstChangeModtime`], which only
//! [`VisitedFileModtimeSlot::for_first_change`] mints, so recording the
//! current buffer's own modtime where GNU records its base's is not something
//! a call site can express.

use std::cell::Cell;
use std::rc::Rc;

use crate::emacs_core::value::Value;

/// GNU `struct buffer`'s `modtime` (`src/buffer.h:645-655`).
///
/// GNU stores it as a `struct timespec` and hides two non-times in the
/// nanoseconds field: `UNKNOWN_MODTIME_NSECS` (-2) and
/// `NONEXISTENT_MODTIME_NSECS` (-1) (`src/buffer.h:314-315`).
/// `buffer_visited_file_modtime` (`src/fileio.c:6156-6163`) turns a negative
/// `tv_nsec` back into the fixnum `UNKNOWN_MODTIME_NSECS - ns`, which is `0`
/// for unknown and `-1` for "the visited file does not exist" -- the two
/// values `visited-file-modtime` documents alongside a real timestamp.
///
/// Naming the three cases is what keeps a half-set modtime -- seconds without
/// nanoseconds, which the previous pair of `Option`s could express -- and the
/// silent collapse of "does not exist" into "unknown" off the table.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum VisitedFileModtime {
    /// GNU `UNKNOWN_MODTIME_NSECS`: no recorded modification time.
    /// `visited-file-modtime` reports `0`.
    #[default]
    Unknown,
    /// GNU `NONEXISTENT_MODTIME_NSECS`: the visited file does not exist --
    /// what `insert-file-contents` records when it could not open the file it
    /// was told to visit (`src/fileio.c:3971-3978,4200`), i.e. every
    /// `find-file` of a new file.  `visited-file-modtime` reports `-1`.
    Nonexistent,
    /// A recorded timestamp; `visited-file-modtime` reports a Lisp timestamp.
    Known { sec: i64, nsec: i32 },
}

impl VisitedFileModtime {
    /// GNU `buffer_visited_file_modtime` (`src/fileio.c:6156-6163`): the Lisp
    /// value `visited-file-modtime` returns and the datum
    /// `record_first_change` conses into `(t . TIME)`.  Both go through here so
    /// `primitive-undo`'s `(time-equal-p time (visited-file-modtime))`
    /// (`lisp/simple.el:3672-3688`) can ever match.
    pub fn to_lisp_value(self) -> Value {
        match self {
            Self::Unknown => Value::fixnum(0),
            Self::Nonexistent => Value::fixnum(-1),
            // GNU `make_lisp_time`: (HIGH LOW USEC PSEC).
            Self::Known { sec, nsec } => Value::list(vec![
                Value::fixnum(sec >> 16),
                Value::fixnum(sec & 0xFFFF),
                Value::fixnum((nsec / 1000) as i64),
                Value::fixnum(((nsec % 1000) as i64) * 1000),
            ]),
        }
    }

    /// GNU `Fset_visited_file_modtime`'s integer arm (`src/fileio.c:6190-6194`):
    /// `check_integer_range (time_flag, -1, 0)` then
    /// `make_timespec (0, UNKNOWN_MODTIME_NSECS - flag)`.  The accepted flags
    /// are exactly the two non-timestamps `visited-file-modtime` can return,
    /// so this is that function read backwards; anything else is out of range
    /// and the caller must signal.
    pub fn from_lisp_flag(flag: i64) -> Option<Self> {
        match flag {
            0 => Some(Self::Unknown),
            -1 => Some(Self::Nonexistent),
            _ => None,
        }
    }

    /// GNU `time_error_value` (`src/fileio.c:3971-3978`): a failed `stat`/open
    /// of a file being visited records "does not exist" for ENOENT/ENOTDIR and
    /// "unknown" for every other errno.
    pub fn from_open_error(err: &std::io::Error) -> Self {
        match err.kind() {
            std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory => Self::Nonexistent,
            _ => Self::Unknown,
        }
    }

    /// The `(seconds, nanoseconds)` pair, or `None` when this is one of GNU's
    /// two non-times. For callers that must compare against a `stat` result.
    pub fn seconds_and_nanos(self) -> Option<(i64, i32)> {
        match self {
            Self::Unknown | Self::Nonexistent => None,
            Self::Known { sec, nsec } => Some((sec, nsec)),
        }
    }

    /// Rebuild from the pdump's two optional halves. A dump that carries only
    /// one half is as unknown as one that carries neither.  GNU's negative
    /// `tv_nsec` sentinels ride along in the nanoseconds half, exactly as they
    /// do in `struct timespec`.
    pub(crate) fn from_dump_halves(sec: Option<i64>, nsec: Option<i32>) -> Self {
        match sec.zip(nsec) {
            Some((0, NONEXISTENT_MODTIME_NSECS)) => Self::Nonexistent,
            Some((0, UNKNOWN_MODTIME_NSECS)) => Self::Unknown,
            Some((sec, nsec)) => Self::Known { sec, nsec },
            None => Self::Unknown,
        }
    }

    /// The two optional halves the pdump image format stores.
    pub(crate) fn to_dump_halves(self) -> (Option<i64>, Option<i32>) {
        match self {
            Self::Unknown => (None, None),
            Self::Nonexistent => (Some(0), Some(NONEXISTENT_MODTIME_NSECS)),
            Self::Known { sec, nsec } => (Some(sec), Some(nsec)),
        }
    }
}

/// GNU `src/buffer.h:314`.
const NONEXISTENT_MODTIME_NSECS: i32 = -1;
/// GNU `src/buffer.h:315`.
const UNKNOWN_MODTIME_NSECS: i32 = -2;

/// The modtime a first-change undo entry records.
///
/// GNU `record_first_change` (`src/undo.c:209-223`) resolves the base buffer
/// first and reads the modtime from *that* buffer:
///
/// ```c
///   struct buffer *base_buffer = current_buffer;
///   ...
///   if (base_buffer->base_buffer)
///     base_buffer = base_buffer->base_buffer;
///   bset_undo_list (current_buffer,
///                   Fcons (Fcons (Qt, buffer_visited_file_modtime (base_buffer)),
///                          BVAR (current_buffer, undo_list)));
/// ```
///
/// The wrapper exists so the recorder cannot be handed the wrong buffer's
/// answer: its field is private to this module and
/// [`VisitedFileModtimeSlot::for_first_change`] is the only constructor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FirstChangeModtime(VisitedFileModtime);

impl FirstChangeModtime {
    /// The `(t . TIME)` datum.
    pub fn to_lisp_value(self) -> Value {
        self.0.to_lisp_value()
    }
}

/// A live handle on a base buffer's own modtime cell: GNU's
/// `b->base_buffer->modtime` as a value.  Only
/// [`VisitedFileModtimeSlot::share_own`] mints one, and it shares the pointer
/// rather than copying the timestamp, so an indirect buffer linked through it
/// sees the base's later `save-buffer` and `set-visited-file-modtime`.
pub struct BaseVisitedFileModtime(Rc<Cell<VisitedFileModtime>>);

/// A buffer's visited-file modtime storage, plus the link an indirect buffer
/// needs to its base's -- GNU's `b->base_buffer->modtime` dereference.
///
/// The own cell is an `Rc` because that dereference has to stay live: the base
/// buffer's modtime changes under `save-buffer`, `insert-file-contents` and
/// `set-visited-file-modtime`, and an indirect buffer's first change must
/// record whatever it says *then*, not a copy taken when the indirect buffer
/// was created.
pub struct VisitedFileModtimeSlot {
    /// This buffer's own modtime -- what `visited-file-modtime` reports.
    own: Rc<Cell<VisitedFileModtime>>,
    /// The base buffer's cell, when this is an indirect buffer.
    base: Option<Rc<Cell<VisitedFileModtime>>>,
}

/// Cloning a `Buffer` must not make the clone share the original's modtime
/// storage: the clone is a different buffer, and GNU's `struct buffer` copy
/// would copy the `struct timespec` by value.  A fresh cell with the same
/// value is what that means here.  The base link is carried over, because it
/// names another buffer's storage -- a clone of an indirect buffer is still an
/// indirect buffer over the same base.
impl Clone for VisitedFileModtimeSlot {
    fn clone(&self) -> Self {
        Self {
            own: Rc::new(Cell::new(self.own.get())),
            base: self.base.clone(),
        }
    }
}

impl Default for VisitedFileModtimeSlot {
    fn default() -> Self {
        Self::new(VisitedFileModtime::Unknown)
    }
}

impl VisitedFileModtimeSlot {
    /// A buffer that visits no file yet: GNU `reset_buffer`'s
    /// `b->modtime = make_timespec (0, UNKNOWN_MODTIME_NSECS)`
    /// (`src/buffer.c:1092`).
    pub fn new(modtime: VisitedFileModtime) -> Self {
        Self {
            own: Rc::new(Cell::new(modtime)),
            base: None,
        }
    }

    /// GNU `Fvisited_file_modtime` (`src/fileio.c:6165-6175`): this buffer's
    /// own recorded time, never its base's.
    pub fn own(&self) -> VisitedFileModtime {
        self.own.get()
    }

    /// Record a modtime for this buffer. `set-visited-file-modtime` writes
    /// `current_buffer`'s slot even in an indirect buffer
    /// (`src/fileio.c:6177-6215`), which is why the own cell stays distinct
    /// from the base link.
    pub fn set_own(&self, modtime: VisitedFileModtime) {
        self.own.set(modtime);
    }

    /// GNU `record_first_change`'s base-buffer redirect (`src/undo.c:213-214`):
    /// the base buffer's modtime for an indirect buffer, this buffer's own
    /// otherwise.
    pub fn for_first_change(&self) -> FirstChangeModtime {
        FirstChangeModtime(match &self.base {
            Some(base) => base.get(),
            None => self.own.get(),
        })
    }

    /// Hand out a live handle on this buffer's own cell, for an indirect
    /// buffer to follow.  This is a pointer share, never a copy -- that is the
    /// whole difference between it and `clone()`, and it is why the two steps
    /// of linking (take the handle, install it) can be written against a map
    /// that cannot be borrowed mutably twice.
    pub(in crate::buffer) fn share_own(&self) -> BaseVisitedFileModtime {
        BaseVisitedFileModtime(Rc::clone(&self.own))
    }

    /// Make this an indirect buffer's slot over `base`: GNU's `reset_buffer`
    /// leaves the indirect buffer's own modtime unknown (`src/buffer.c:1092`,
    /// reached from `Fmake_indirect_buffer`, `src/buffer.c:896`) and
    /// `b->base_buffer` is what `record_first_change` follows.
    pub(in crate::buffer) fn reset_as_indirect_of(&mut self, base: BaseVisitedFileModtime) {
        self.own.set(VisitedFileModtime::Unknown);
        // GNU flattens double indirection (`src/buffer.c:868-871`), so a base
        // buffer's own cell is always the one to follow.
        self.base = Some(base.0);
    }

    /// True when this slot follows the cell `base` names.  Test-facing: it is
    /// how "the link is live" is stated without reaching into private fields.
    #[cfg(test)]
    pub(crate) fn follows(&self, base: &BaseVisitedFileModtime) -> bool {
        self.base
            .as_ref()
            .is_some_and(|linked| Rc::ptr_eq(linked, &base.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The link is a live dereference, not a snapshot: GNU reads
    /// `base_buffer->modtime` at the moment of the change (`src/undo.c:221`),
    /// so a `save-buffer` in the base between `make-indirect-buffer` and the
    /// first change must be visible.
    #[test]
    fn an_attached_slot_reads_the_bases_current_modtime() {
        let base = VisitedFileModtimeSlot::new(VisitedFileModtime::Known { sec: 1, nsec: 2 });
        let mut indirect = VisitedFileModtimeSlot::default();
        indirect.reset_as_indirect_of(base.share_own());

        assert_eq!(
            indirect.for_first_change(),
            base.for_first_change(),
            "an indirect buffer's first change records its base's modtime"
        );
        assert_eq!(
            indirect.own(),
            VisitedFileModtime::Unknown,
            "the indirect buffer visits no file of its own"
        );

        base.set_own(VisitedFileModtime::Known { sec: 9, nsec: 8 });
        assert_eq!(
            indirect.for_first_change(),
            FirstChangeModtime(VisitedFileModtime::Known { sec: 9, nsec: 8 }),
            "the base's later modtime must be visible through the link"
        );
    }

    /// Setting an indirect buffer's own modtime does not change what its first
    /// change records -- GNU's redirect is unconditional (`src/undo.c:213-214`),
    /// confirmed against GNU 31.0.90 with `(set-visited-file-modtime '(1 2 3 4))`
    /// in the indirect buffer.
    #[test]
    fn an_own_modtime_does_not_displace_the_bases_for_first_change() {
        let base = VisitedFileModtimeSlot::new(VisitedFileModtime::Known { sec: 7, nsec: 0 });
        let mut indirect = VisitedFileModtimeSlot::default();
        indirect.reset_as_indirect_of(base.share_own());
        indirect.set_own(VisitedFileModtime::Known { sec: 1, nsec: 2 });

        assert_eq!(
            indirect.own(),
            VisitedFileModtime::Known { sec: 1, nsec: 2 }
        );
        assert_eq!(
            indirect.for_first_change(),
            FirstChangeModtime(VisitedFileModtime::Known { sec: 7, nsec: 0 })
        );
    }

    /// A cloned buffer is a different buffer: writing its modtime must not
    /// write the original's.
    #[test]
    fn cloning_a_slot_copies_the_value_and_not_the_cell() {
        let original = VisitedFileModtimeSlot::new(VisitedFileModtime::Known { sec: 5, nsec: 6 });
        let clone = original.clone();
        assert_eq!(clone.own(), original.own());

        clone.set_own(VisitedFileModtime::Unknown);
        assert_eq!(
            original.own(),
            VisitedFileModtime::Known { sec: 5, nsec: 6 },
            "the clone must not share the original's storage"
        );
    }
}
