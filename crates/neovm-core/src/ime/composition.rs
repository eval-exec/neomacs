//! Accepted composition edits through the normal VM change machinery.

use super::context::InsertionAnchor;
use crate::buffer::{EmacsBytePos, EmacsByteRange};
use neovm_host_abi::ime::{ImeOperation, ImeSessionId};

#[derive(Debug)]
pub(super) struct ActiveComposition {
    pub(super) anchor: InsertionAnchor,
    pub(super) start: EmacsBytePos,
}

impl crate::Context {
    pub(crate) fn handle_ime_operation(
        &mut self,
        session: ImeSessionId,
        operation: ImeOperation,
    ) -> crate::emacs_core::error::EvalResult {
        use crate::Value;
        use crate::emacs_core::{editfns, error::signal, textprop};
        self.composition.surrounding.invalidate();
        self.composition.operation_revision = self.composition.operation_revision.wrapping_add(1);
        let operation_revision = self.composition.operation_revision;
        if matches!(operation, ImeOperation::Begin) {
            if session.0 > self.composition.latest {
                self.composition.latest = session.0;
                self.composition.active = self.ime_anchor().map(|anchor| ActiveComposition {
                    start: anchor.point,
                    anchor,
                });
            }
            return Ok(Value::NIL);
        }
        if session.0 != self.composition.latest {
            return Ok(Value::NIL);
        }
        let Some(mut active) = self.composition.active.take() else {
            return Ok(Value::NIL);
        };
        let ImeOperation::Replace {
            mut before_bytes,
            after_bytes,
            text,
        } = operation
        else {
            return Ok(Value::NIL);
        };
        if self.ime_anchor() != Some(active.anchor) {
            // Ordinary command hooks can edit text and move point without
            // closing the platform input connection. That retires authority
            // over old byte offsets, not subsequent insertion-only input.
            let Some(anchor) = self.ime_anchor().filter(|anchor| {
                anchor.buffer == active.anchor.buffer
                    && anchor.window == active.anchor.window
                    && anchor.multibyte == active.anchor.multibyte
                    && anchor.selection_anchor == active.anchor.selection_anchor
            }) else {
                return Ok(Value::NIL);
            };
            active = ActiveComposition {
                start: anchor.point,
                anchor,
            };
            if before_bytes != 0 || after_bytes != 0 {
                self.composition.active = Some(active);
                return Ok(Value::NIL);
            }
        }
        if after_bytes != 0 {
            // Deleting to the right of the anchor is not supported, but the
            // session itself is untouched -- retiring it would silently kill
            // every later commit. The re-anchored path above already restores
            // for this same condition.
            self.composition.active = Some(active);
            return Ok(Value::NIL);
        }
        // The walk borrows the buffer, so decide first and only then touch
        // `self.composition`.
        //
        // A walk or range failure deliberately RETIRES the session: the
        // platform asked to delete text this session never inserted, which
        // means its view of the buffer has desynced from ours, and
        // `ime_session_cannot_delete_before_its_anchor_or_reopen_a_retired_identity`
        // pins that. Read-only is different -- see below.
        let walk = {
            let buffer = self
                .buffers
                .current_buffer()
                .expect("validated insertion anchor");
            let mut start = active.anchor.point;
            let mut walked = Some(());
            while before_bytes > 0 && start > active.start {
                let Some(ch) = buffer.char_before_emacs_byte_pos(start) else {
                    walked = None;
                    break;
                };
                let Some(remaining) = before_bytes.checked_sub(ch.len_utf8()) else {
                    walked = None;
                    break;
                };
                let Some(length) = buffer.char_before_emacs_byte_len(start) else {
                    walked = None;
                    break;
                };
                start = start.saturating_sub_len(length);
                before_bytes = remaining;
            }
            // Only the text this session inserted is available to the platform.
            let in_range = before_bytes == 0 && start >= active.start;
            let read_only = editfns::buffer_read_only_active_in_state(&self.obarray, &[], buffer);
            walked.filter(|_| in_range).map(|_| (start, read_only))
        };
        let Some((start, read_only)) = walk else {
            return Ok(Value::NIL);
        };
        if read_only {
            // The buffer refused the edit; the composition did not end. One IME
            // keystroke into a read-only buffer must not disable input entirely.
            let buffer_id = active.anchor.buffer;
            self.composition.active = Some(active);
            return Err(signal(
                "buffer-read-only",
                vec![Value::make_buffer(buffer_id)],
            ));
        }
        // Re-acquired after the restore decisions above, which needed `self`
        // mutably.
        let buffer = self
            .buffers
            .current_buffer()
            .expect("validated insertion anchor");
        let range = EmacsByteRange::new(start, active.anchor.point);
        let target_multibyte = buffer.get_multibyte();
        let start_char = buffer.emacs_byte_pos_to_lisp_char_pos(start).as_i64();
        let removed = buffer.buffer_substring_bytes_range(range);
        textprop::verify_text_read_only_in_state(
            &self.obarray,
            &self.buffers,
            active.anchor.buffer,
            start.get(),
            active.anchor.point.get(),
        )?;
        let replacement = crate::emacs_core::buffer::buffer_insert_lisp_string_from_lisp_string(
            &crate::heap_types::LispString::from_utf8(&text),
            target_multibyte,
        );
        let change = editfns::text_change_for_lisp_string_replacement_in_manager(
            &self.buffers,
            active.anchor.buffer,
            range,
            &replacement,
        )?;
        editfns::signal_before_text_change(self, change)?;
        // Lisp hooks can move point, switch/kill buffers, edit text, or reenter
        // input. Never apply the previously measured range after that.
        if self.ime_anchor() != Some(active.anchor)
            || self.composition.operation_revision != operation_revision
        {
            if self.composition.operation_revision == operation_revision {
                self.composition.active = Some(active);
            }
            return Ok(Value::NIL);
        }
        self.buffers.replace_buffer_measured_region_lisp_string(
            active.anchor.buffer,
            change.old_range(),
            &replacement,
        );
        // GNU textconv.c inserts with inheritance, just like ordinary editor
        // insertion and the snapshot-qualified replacement path.
        crate::emacs_core::buffer::apply_inherited_text_properties(
            &self.obarray,
            &[],
            &mut self.buffers,
            active.anchor.buffer,
            start.get(),
            replacement.sbytes(),
        );
        if let Some(buffer) = self.buffers.get_mut(active.anchor.buffer) {
            buffer.goto_emacs_byte_pos(
                start.add_len(crate::buffer::EmacsByteLen::new(replacement.sbytes())),
            );
        }
        let updated = self.ime_anchor();
        use super::conversion::ConversionEdit;
        let at = crate::buffer::LispCharPos1::new(start_char);
        let deletion = (!removed.is_empty()).then(|| ConversionEdit::DeletionBeforePoint {
            at,
            text: if target_multibyte {
                crate::heap_types::LispString::from_emacs_bytes(removed)
            } else {
                crate::heap_types::LispString::from_unibyte(removed)
            },
        });
        let insertion = (!replacement.is_empty()).then_some(ConversionEdit::Insertion {
            at,
            text: replacement,
        });
        let event = self
            .publish_conversion_edits(active.anchor.buffer, deletion.into_iter().chain(insertion));
        // The edit has already landed, so the session survives even if an
        // `after-change-functions` hook signals. Re-arm before propagating:
        // letting `?` jump past this retired the composition permanently, so a
        // single signalling hook disabled input for the rest of the session.
        let after_change = editfns::signal_after_text_change(self, change);
        if let Some(anchor) = updated
            && self.composition.operation_revision == operation_revision
        {
            self.composition.active = Some(ActiveComposition {
                anchor,
                start: active.start,
            });
        }
        after_change?;
        // GNU read_char returns this command event after conversion edits.
        // Lisp's analyze-text-conversion owns post-self-insert hooks, mode
        // integration, and undo amalgamation; do not duplicate those here.
        Ok(event)
    }
}
