//! Evaluator-thread text conversion; platform byte counts never index Lisp text directly.

mod surrounding;

#[cfg(test)]
mod tests;

use crate::buffer::{BufferId, EmacsBytePos, EmacsByteRange};
use neovm_host_abi::ime::{ImeOperation, ImeSessionId};

/// A revision-qualified insertion anchor. Any unrelated edit invalidates it;
/// we never guess where an old platform offset belongs in a newer buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct InsertionAnchor {
    buffer: BufferId,
    window: Option<crate::window::WindowId>,
    point: EmacsBytePos,
    accessible: crate::buffer::AccessibleEmacsByteRange,
    revision: i64,
    selection_anchor: Option<EmacsBytePos>,
    multibyte: bool,
}

#[derive(Debug)]
struct ActiveComposition {
    anchor: InsertionAnchor,
    start: EmacsBytePos,
}

/// Transient VM state, deliberately excluded from runtime images.
#[derive(Debug, Default)]
pub(crate) struct CompositionState {
    latest: u64,
    operation_revision: u64,
    active: Option<ActiveComposition>,
    surrounding: surrounding::SurroundingTextState,
}

impl crate::Context {
    fn ime_anchor(&self) -> Option<InsertionAnchor> {
        let buffer = self.buffers.current_buffer()?;
        Some(InsertionAnchor {
            buffer: buffer.id(),
            window: self
                .frames
                .selected_frame()
                .map(|frame| frame.selected_window),
            point: buffer.point_emacs_byte_pos(),
            accessible: buffer.accessible_emacs_byte_region(),
            revision: buffer.chars_modified_tick(),
            selection_anchor: buffer
                .get_buffer_local("mark-active")
                .filter(|active| active.is_truthy())
                .and_then(|_| buffer.mark_emacs_byte_pos()),
            multibyte: buffer.get_multibyte(),
        })
    }

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
        let Some(active) = self.composition.active.take() else {
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
        if self.ime_anchor() != Some(active.anchor) || after_bytes != 0 {
            return Ok(Value::NIL);
        }
        let buffer = self
            .buffers
            .current_buffer()
            .expect("validated insertion anchor");
        let mut start = active.anchor.point;
        while before_bytes > 0 && start > active.start {
            let Some(ch) = buffer.char_before_emacs_byte_pos(start) else {
                return Ok(Value::NIL);
            };
            let Some(remaining) = before_bytes.checked_sub(ch.len_utf8()) else {
                return Ok(Value::NIL);
            };
            let Some(length) = buffer.char_before_emacs_byte_len(start) else {
                return Ok(Value::NIL);
            };
            start = start.saturating_sub_len(length);
            before_bytes = remaining;
        }
        // Only the text this session inserted is available to the platform.
        if before_bytes != 0 || start < active.start {
            return Ok(Value::NIL);
        }
        if editfns::buffer_read_only_active_in_state(&self.obarray, &[], buffer) {
            return Err(signal(
                "buffer-read-only",
                vec![Value::make_buffer(active.anchor.buffer)],
            ));
        }
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
            return Ok(Value::NIL);
        }
        self.buffers.replace_buffer_measured_region_lisp_string(
            active.anchor.buffer,
            change.old_range(),
            &replacement,
        );
        if let Some(buffer) = self.buffers.get_mut(active.anchor.buffer) {
            buffer.goto_emacs_byte_pos(
                start.add_len(crate::buffer::EmacsByteLen::new(replacement.sbytes())),
            );
        }
        let updated = self.ime_anchor();
        editfns::signal_after_text_change(self, change)?;
        if let Some(anchor) = updated
            && self.ime_anchor() == Some(anchor)
            && self.composition.operation_revision == operation_revision
        {
            self.composition.active = Some(ActiveComposition {
                anchor,
                start: active.start,
            });
        }
        let mut edits = Vec::new();
        let buffer_value = Value::make_buffer(active.anchor.buffer);
        if !text.is_empty() {
            edits.push(Value::list(vec![
                buffer_value,
                Value::fixnum(start_char),
                Value::fixnum(start_char + replacement.schars() as i64),
                Value::heap_string(replacement),
            ]));
        }
        if !removed.is_empty() {
            edits.push(Value::list(vec![
                buffer_value,
                Value::fixnum(start_char),
                Value::fixnum(start_char),
                Value::heap_string(if target_multibyte {
                    crate::heap_types::LispString::from_emacs_bytes(removed)
                } else {
                    crate::heap_types::LispString::from_unibyte(removed)
                }),
            ]));
        }
        if edits.is_empty() {
            return Ok(Value::NIL);
        }
        // GNU read_char returns this command event after conversion edits.
        // Lisp's analyze-text-conversion owns post-self-insert hooks, mode
        // integration, and undo amalgamation; do not duplicate those here.
        self.assign("text-conversion-edits", Value::list(edits));
        Ok(Value::symbol("text-conversion"))
    }
}
