//! GNU `load-force-doc-strings` at the reader's dotted-list completion
//! (`src/lread.c:4294-4301`, `get_lazy_string` `src/lread.c:3495-3550`) and the
//! saved `#@` block ring it reads (`skip_lazy_string` `src/lread.c:3445-3485`).
//!
//! The conversion is admitted only for the exact live `load-file-name` object:
//! GNU compares with `BASE_EQ`, so a separately constructed string with equal
//! bytes is ordinary data. The saved blocks belong to the compiled-file read
//! loop rather than to one reader instance because a `.elc` block is skipped in
//! the read of one top-level form and referenced from the next.

use crate::emacs_core::doc::{DocBytePosition, DocUnquoteError, unquote_saved_doc_block};
use crate::emacs_core::value::Value;

/// The exact live `load-file-name` object read at a dotted-list completion.
///
/// A witness can only be observed from the reader environment; the admission
/// below compares the completed pair's car to this object's bits, never to a
/// string's bytes.
#[derive(Clone, Copy, Debug)]
pub(super) struct LiveLoadFileName(Value);

impl LiveLoadFileName {
    /// GNU requires the live value to be non-nil before the `BASE_EQ` test
    /// (`!NILP (XCAR (obj))`), so a nil binding observes nothing.
    pub(super) fn observe(value: Option<Value>) -> Option<Self> {
        let value = value?;
        (!value.is_nil()).then_some(Self(value))
    }

    pub(super) fn value(self) -> Value {
        self.0
    }
}

/// The reader's view of `load_force_doc_strings`.
///
/// A byte-backed source cannot run Lisp during one read, so its policy is
/// memoized on first consultation and a read with no dotted list pays nothing.
/// A callable source can change either binding at every character, so the
/// reader consults it live at each completion instead of storing this value.
#[derive(Clone, Copy, Debug)]
pub(super) enum LazyDocPolicy {
    /// Not consulted yet.
    Unknown,
    Disabled,
    /// Non-nil; the live `load-file-name` object when that is non-nil too.
    Forced {
        witness: Option<LiveLoadFileName>,
    },
}

impl LazyDocPolicy {
    #[cfg(test)]
    pub(super) fn observe(forced: Option<Value>, load_file_name: Option<Value>) -> Self {
        match forced {
            Some(value) if value.is_truthy() => Self::Forced {
                witness: LiveLoadFileName::observe(load_file_name),
            },
            _ => Self::Disabled,
        }
    }

    pub(super) fn is_forced(self) -> bool {
        matches!(self, Self::Forced { .. })
    }
}

/// A dotted list admitted by GNU's completion hack: `(LIVE-LOAD-FILE-NAME .
/// FIXNUM)`, the whole object's car and cdr.
#[derive(Clone, Copy, Debug)]
pub(super) struct LazyDocRef {
    file: LiveLoadFileName,
    position: DocBytePosition,
}

impl LazyDocRef {
    /// The typed admission. GNU tests `BASE_EQ (XCAR (obj), Vload_file_name)`
    /// on the completed object, so the pair must be two elements with the
    /// witness object as its first; `(x #$ . OFFSET)` is not admitted just
    /// because its final cons resembles one.
    pub(super) fn admit(completed: Value, policy: LazyDocPolicy) -> Option<Self> {
        let LazyDocPolicy::Forced {
            witness: Some(file),
        } = policy
        else {
            return None;
        };
        if !completed.is_cons() {
            return None;
        }
        if completed.cons_car().bits() != file.value().bits() {
            return None;
        }
        let cdr = completed.cons_cdr();
        if !cdr.is_fixnum() {
            return None;
        }
        Some(Self {
            file,
            position: DocBytePosition::from_fixnum(cdr.as_int()?),
        })
    }

    pub(super) fn file_value(self) -> Value {
        self.file.value()
    }

    pub(super) fn position(self) -> DocBytePosition {
        self.position
    }
}

/// One skipped `#@` block: the absolute file position of its first record byte
/// and the raw bytes (still `^A`-quoted, including the `^_` terminator).
struct SavedDocBlock {
    position: DocBytePosition,
    bytes: Vec<u8>,
}

/// GNU `saved_strings` (`src/lread.c:3445-3485`): the two most recent `#@`
/// blocks skipped while `load-force-doc-strings` was non-nil.
#[derive(Default)]
pub(crate) struct SavedDocBlocks {
    slots: [Option<SavedDocBlock>; 2],
}

impl SavedDocBlocks {
    pub(crate) fn new() -> Self {
        Self {
            slots: [None, None],
        }
    }

    /// GNU exchanges the two slots and writes the newest block into slot 0.
    /// The previous bytes are reused, so a forced load pays one allocation per
    /// slot rather than one per record.
    pub(crate) fn record(&mut self, position: DocBytePosition, bytes: &[u8]) {
        self.slots.swap(0, 1);
        match &mut self.slots[0] {
            Some(block) => {
                block.position = position;
                block.bytes.clear();
                block.bytes.extend_from_slice(bytes);
            }
            slot @ None => {
                *slot = Some(SavedDocBlock {
                    position,
                    bytes: bytes.to_vec(),
                });
            }
        }
    }

    /// GNU `get_lazy_string`'s search: the saved block containing `position`,
    /// from `position` to its end. `None` means the file fallback answers.
    pub(super) fn find(&self, position: DocBytePosition) -> Option<&[u8]> {
        self.slots.iter().flatten().find_map(|block| {
            let start = block.position.get();
            let end = start.checked_add(u64::try_from(block.bytes.len()).ok()?)?;
            let pos = position.get();
            if pos < start || pos >= end {
                return None;
            }
            let at = usize::try_from(pos - start).ok()?;
            block.bytes.get(at..)
        })
    }

    /// The unquoted bytes of the saved record at `position`, if a saved block
    /// holds it. GNU's saved-block rules are tolerant of unknown `^A` escapes;
    /// only a structurally malformed block is an error.
    pub(super) fn unquoted(
        &self,
        position: DocBytePosition,
    ) -> Option<Result<Vec<u8>, DocUnquoteError>> {
        self.find(position).map(unquote_saved_doc_block)
    }
}
