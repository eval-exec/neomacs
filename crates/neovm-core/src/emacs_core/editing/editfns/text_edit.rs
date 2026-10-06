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
use crate::emacs_core::error::Flow;

/// How a deletion or replacement range is re-measured once its modification
/// callbacks returned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RemeasureRule {
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
pub(crate) struct PendingTextEdit {
    range: TextEditRange,
    rule: RemeasureRule,
}

/// A range the before-change protocol has run for, measured in the buffer
/// that was current when the callbacks returned. Its constructor is private
/// to this module, so holding one proves the callbacks already ran.
#[derive(Clone, Copy, Debug)]
#[must_use = "lease and mutate the prepared range, then signal its after-change"]
pub(crate) struct PreparedTextEdit {
    range: TextEditRange,
    measured_at: TextMeasurement,
}

impl PendingTextEdit {
    /// RANGE is measured in the current buffer and already validated against
    /// the accessible region by the caller (GNU's `validate_region`).
    pub(crate) fn new(range: TextEditRange, rule: RemeasureRule) -> Self {
        Self { range, rule }
    }

    /// Run GNU `prepare_to_modify_buffer (from, to, &from)` and re-measure.
    ///
    /// Returns None when no live buffer is current afterwards; there is
    /// nothing left to edit then.
    pub(crate) fn prepare(
        self,
        ctx: &mut crate::emacs_core::eval::Context,
    ) -> Result<Option<PreparedTextEdit>, Flow> {
        let before = ctx
            .buffers
            .current_buffer_id()
            .and_then(|buffer| TextMeasurement::of(&ctx.buffers, buffer));
        let from = prepare_buffer_change(
            ctx,
            self.range,
            BufferChangeKind::Characters,
            ChangeStartPolicy::Preserved,
        )?;
        let Some(buffer) = ctx.buffers.current_buffer_id() else {
            return Ok(None);
        };
        let Some(measured_at) = TextMeasurement::of(&ctx.buffers, buffer) else {
            return Ok(None);
        };
        let Some(live) = ctx.buffers.get(buffer) else {
            return Ok(None);
        };
        let chars = self.remeasured_chars(live, from);
        // The callback-free path: same text, same FROM, same TO. The bytes
        // measured before preparation are still exact, so no text walk.
        let range = if before == Some(measured_at) && chars == self.range.char_range() {
            self.range
        } else {
            live.edit_range_for_char_range(chars)
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
    pub(crate) fn range(self) -> TextEditRange {
        self.range
    }

    /// The exclusive lease that mutates the prepared range.
    pub(crate) fn lease(
        self,
        buffers: &mut crate::buffer::BufferManager,
    ) -> Option<PreparedBufferEdit<'_>> {
        buffers.prepare_measured_buffer_edit(self.range, self.measured_at)
    }
}

/// What a deletion hands back (GNU `del_range_1`'s RET_STRING).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DeletedText {
    Discard,
    Return,
}

/// GNU `del_range_1 (from, to, true, ret_string)` for a range the caller has
/// measured inside the accessible region: run the modification callbacks,
/// re-measure, delete, and signal the after-change for what was deleted.
///
/// With `DeletedText::Return` the deleted text is returned (empty when the
/// callbacks left nothing to delete); with `Discard` the result is None.
pub(crate) fn delete_text_range(
    ctx: &mut crate::emacs_core::eval::Context,
    range: TextEditRange,
    deleted: DeletedText,
) -> Result<Option<crate::heap_types::LispString>, Flow> {
    let Some(prepared) = PendingTextEdit::new(range, RemeasureRule::DeleteRange).prepare(ctx)?
    else {
        return Ok(None);
    };
    let mut deleted_range = prepared.range();
    let text = match prepared.lease(&mut ctx.buffers) {
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
    };
    super::signal_after_text_change(ctx, crate::buffer::TextChange::deletion(deleted_range))?;
    Ok(match deleted {
        DeletedText::Discard => None,
        DeletedText::Return => Some(text.unwrap_or_else(|| empty_buffer_string(ctx))),
    })
}

/// The empty string `make_buffer_string` returns for an empty range: it
/// takes the current buffer's multibyteness.
fn empty_buffer_string(ctx: &crate::emacs_core::eval::Context) -> crate::heap_types::LispString {
    if ctx
        .buffers
        .current_buffer()
        .is_some_and(Buffer::get_multibyte)
    {
        crate::heap_types::LispString::from_emacs_bytes(Vec::new())
    } else {
        crate::heap_types::LispString::from_unibyte(Vec::new())
    }
}
