//! Deletions and replacements re-measured after their modification callbacks.
//!
//! GNU runs `prepare_to_modify_buffer (from, to, &from)` and only then
//! computes the range it mutates (`del_range_1`, insdel.c:1884-1892;
//! `replace_range`, insdel.c:1502-1513): the callbacks run arbitrary Lisp,
//! which may insert, delete, narrow or select another buffer. A range measured
//! before the callbacks is therefore stale, and reusing its byte positions can
//! reach past the text or split a multibyte sequence. These types make the
//! stale range unusable: the only way from a pending range to a lease that
//! mutates storage is [`PendingTextEdit::prepare`].
//!
//! Everything here belongs to one mutator's Context for the duration of one
//! primitive; nothing is cached or shared.

use super::{BufferChangeKind, ChangeStartPolicy, prepare_buffer_change};
use crate::buffer::{
    Buffer, CharPos0, CharRange, PreparedBufferEdit, TextEditRange, TextMeasurement,
};
use neovm_value::error::Flow;

/// How a deletion or replacement range is re-measured once its modification
/// callbacks returned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[doc(hidden)]
pub enum RemeasureRule {
    /// GNU `del_range_1`: TO = min (ZV, FROM + length). FROM itself is not
    /// clamped again.
    DeleteRange,
    /// GNU `replace_range`: TO = FROM + length, then FROM is raised to BEGV
    /// and TO lowered to ZV.
    ReplaceRange,
}

/// A deletion or replacement measured before its modification callbacks ran.
#[derive(Clone, Copy, Debug)]
#[must_use = "prepare the pending edit: its range is stale once Lisp has run"]
#[doc(hidden)]
pub struct PendingTextEdit {
    range: TextEditRange,
    rule: RemeasureRule,
}

/// A range the before-change protocol has run for, measured in the buffer
/// that was current when the callbacks returned. Its constructor is private
/// to this module, so holding one proves the callbacks already ran.
#[derive(Clone, Copy, Debug)]
#[must_use = "lease and mutate the prepared range, then signal its after-change"]
#[doc(hidden)]
pub struct PreparedTextEdit {
    range: TextEditRange,
    measured_at: TextMeasurement,
}

impl PendingTextEdit {
    /// RANGE is measured in the current buffer and already validated against
    /// the accessible region by the caller (GNU's `validate_region`).
    #[doc(hidden)]
    pub fn new(range: TextEditRange, rule: RemeasureRule) -> Self {
        Self { range, rule }
    }

    /// Run GNU `prepare_to_modify_buffer (from, to, &from)` and re-measure.
    ///
    /// Returns None when no live buffer is current afterwards; there is
    /// nothing left to edit then.
    #[doc(hidden)]
    pub fn prepare<H: crate::emacs_core::edit_host::EditHost>(
        self,
        host: &mut H,
    ) -> Result<Option<PreparedTextEdit>, Flow> {
        let before = {
            let (edit, _vars) = host.edit_and_vars();
            edit.buffers
                .current_buffer_id()
                .and_then(|buffer| TextMeasurement::of(&edit.buffers, buffer))
        };
        let from = prepare_buffer_change(
            host,
            self.range,
            BufferChangeKind::Characters,
            ChangeStartPolicy::Preserved,
        )?;
        let Some(buffer) = host.edit_and_vars().0.buffers.current_buffer_id() else {
            return Ok(None);
        };
        let Some(measured_at) = ({
            let (edit, _vars) = host.edit_and_vars();
            TextMeasurement::of(&edit.buffers, buffer)
        }) else {
            return Ok(None);
        };
        let range = {
            let (edit, _vars) = host.edit_and_vars();
            let Some(live) = edit.buffers.get(buffer) else {
                return Ok(None);
            };
            let chars = self.remeasured_chars(live, from);
            // The callback-free path: same text, same FROM, same TO. The bytes
            // measured before preparation are still exact, so no text walk.
            if before == Some(measured_at) && chars == self.range.char_range() {
                self.range
            } else {
                live.edit_range_for_char_range(chars)
            }
        };
        Ok(Some(PreparedTextEdit { range, measured_at }))
    }

    fn remeasured_chars(self, live: &Buffer, from: CharPos0) -> CharRange {
        let length = self.range.char_len();
        let zv = live.point_max_char_pos();
        let (from, to) = match self.rule {
            RemeasureRule::DeleteRange => (from, from.add_len(length).min(zv)),
            RemeasureRule::ReplaceRange => (
                from.max(live.point_min_char_pos()),
                from.add_len(length).min(zv),
            ),
        };
        // A callback can leave FROM beyond ZV, where GNU would go on with a
        // negative length. Keep the range ordered and inside the text.
        let total = live.total_char_end_pos();
        let from = from.min(total);
        CharRange::new(from, to.clamp(from, total))
    }
}

impl PreparedTextEdit {
    #[doc(hidden)]
    pub fn range(self) -> TextEditRange {
        self.range
    }

    /// The exclusive lease that mutates the prepared range.
    #[doc(hidden)]
    pub fn lease(
        self,
        buffers: &mut crate::buffer::BufferManager,
    ) -> Option<PreparedBufferEdit<'_>> {
        buffers.prepare_measured_buffer_edit(self.range, self.measured_at)
    }
}

/// What a deletion hands back (GNU `del_range_1`'s RET_STRING).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[doc(hidden)]
pub enum DeletedText {
    Discard,
    Return,
}

/// GNU `del_range_1 (from, to, true, ret_string)` for a range the caller has
/// measured inside the accessible region: run the modification callbacks,
/// re-measure, delete, and signal the after-change for what was deleted.
///
/// With `DeletedText::Return` the deleted text is returned (empty when the
/// callbacks left nothing to delete); with `Discard` the result is None.
#[doc(hidden)]
pub fn delete_text_range<H: crate::emacs_core::edit_host::EditHost>(
    host: &mut H,
    range: TextEditRange,
    deleted: DeletedText,
) -> Result<Option<neovm_value::heap_types::LispString>, Flow> {
    let Some(prepared) = PendingTextEdit::new(range, RemeasureRule::DeleteRange).prepare(host)?
    else {
        return Ok(None);
    };
    let mut deleted_range = prepared.range();
    let text = {
        let (edit, _vars) = host.edit_and_vars();
        match prepared.lease(&mut edit.buffers) {
            Some(lease) => {
                deleted_range = lease.range();
                match deleted {
                    DeletedText::Discard => {
                        let _ = lease.delete();
                        None
                    }
                    DeletedText::Return => lease.delete_and_extract(),
                }
            }
            None => None,
        }
    };
    super::signal_after_text_change(host, crate::buffer::TextChange::deletion(deleted_range))?;
    Ok(match deleted {
        DeletedText::Discard => None,
        DeletedText::Return => {
            Some(text.unwrap_or_else(|| empty_buffer_string(&host.edit_and_vars().0.buffers)))
        }
    })
}

/// The empty string `make_buffer_string` returns for an empty range: it
/// takes the current buffer's multibyteness.
fn empty_buffer_string(
    buffers: &crate::buffer::BufferManager,
) -> neovm_value::heap_types::LispString {
    if buffers.current_buffer().is_some_and(Buffer::get_multibyte) {
        neovm_value::heap_types::LispString::from_emacs_bytes(Vec::new())
    } else {
        neovm_value::heap_types::LispString::from_unibyte(Vec::new())
    }
}
