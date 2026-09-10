//! GNU text-conversion records, rooted before Lisp change hooks can run.

use crate::Value;
use crate::buffer::{BufferId, LispCharPos1};
use crate::heap_types::LispString;

/// Distinguish committed insertion, backspace, and replacement bookkeeping.
/// GNU's analyzer intentionally ignores ephemeral replacement deletions.
pub(super) enum ConversionEdit {
    Insertion { at: LispCharPos1, text: LispString },
    DeletionBeforePoint { at: LispCharPos1, text: LispString },
    EphemeralDeletion { at: LispCharPos1 },
}

impl crate::Context {
    /// Changes arrive in edit order; Lisp consumes the resulting reverse list.
    /// Positions are live markers, not integers that go stale after a hook.
    pub(super) fn publish_conversion_edits(
        &mut self,
        buffer: BufferId,
        edits: impl IntoIterator<Item = ConversionEdit>,
    ) -> Value {
        use crate::emacs_core::marker::make_registered_buffer_marker;
        let mut records = Value::NIL;
        for edit in edits {
            let (start, end, payload) = match edit {
                ConversionEdit::Insertion { at, text } => (
                    at,
                    LispCharPos1::new(at.as_i64() + text.schars() as i64),
                    Value::heap_string(text),
                ),
                ConversionEdit::DeletionBeforePoint { at, text } => {
                    (at, at, Value::heap_string(text))
                }
                ConversionEdit::EphemeralDeletion { at } => (at, at, Value::T),
            };
            let beginning = make_registered_buffer_marker(&mut self.buffers, buffer, start, false);
            let ending = if start == end {
                beginning
            } else {
                make_registered_buffer_marker(&mut self.buffers, buffer, end, true)
            };
            records = Value::cons(
                Value::list(vec![Value::make_buffer(buffer), beginning, ending, payload]),
                records,
            );
        }
        if records.is_nil() {
            return Value::NIL;
        }
        self.assign("text-conversion-edits", records);
        Value::symbol("text-conversion")
    }
}
