//! Snapshot-qualified selection using GNU point/mark operations.

use neovm_host_abi::ime::{ImeSelection, ImeSelectionOutcome};

impl crate::Context {
    /// Apply point/mark positions only to the exact context that was observed.
    ///
    /// A matching request consumes the snapshot before Lisp can run, including
    /// on invalid offsets or a Lisp error. Replays cannot reuse the observation.
    /// GNU `really_set_point_and_mark` uses `goto-char`, `push-mark`, and
    /// `deactivate-mark`; retain their Lisp behavior instead of copying it here.
    pub fn ime_set_selection(
        &mut self,
        request: ImeSelection,
    ) -> Result<ImeSelectionOutcome, crate::emacs_core::error::Flow> {
        use crate::Value;
        let Some(captured) = self.composition.surrounding.captured.as_ref() else {
            return Ok(ImeSelectionOutcome::StaleSnapshot);
        };
        if captured.id != request.snapshot {
            return Ok(ImeSelectionOutcome::StaleSnapshot);
        }
        let captured = self.composition.surrounding.captured.take().unwrap();
        if self.ime_anchor() != Some(captured.source) {
            return Ok(ImeSelectionOutcome::StaleSnapshot);
        }
        let locate = |offset| {
            captured
                .positions
                .iter()
                .find(|(byte, _)| *byte == offset)
                .map(|(_, pos)| *pos)
        };
        let (Some(cursor), Some(anchor)) = (locate(request.cursor), locate(request.anchor)) else {
            return Ok(ImeSelectionOutcome::InvalidSelection);
        };
        let buffer = self
            .buffers
            .current_buffer()
            .expect("validated snapshot context");
        let point = buffer.emacs_byte_pos_to_lisp_char_pos(cursor).as_i64();
        let mark = buffer.emacs_byte_pos_to_lisp_char_pos(anchor).as_i64();
        self.composition.active = None;
        self.composition.operation_revision = self.composition.operation_revision.wrapping_add(1);
        let operation_revision = self.composition.operation_revision;
        self.apply1(Value::symbol("goto-char"), Value::fixnum(point))?;
        let expected = super::InsertionAnchor {
            point: cursor,
            ..captured.source
        };
        if self.ime_anchor() != Some(expected)
            || self.composition.operation_revision != operation_revision
        {
            return Ok(ImeSelectionOutcome::StaleSnapshot);
        }
        if cursor == anchor && captured.source.selection_anchor.is_some() {
            self.apply0(Value::symbol("deactivate-mark"))?;
        } else {
            self.apply1(Value::symbol("push-mark"), Value::fixnum(mark))?;
        }
        Ok(ImeSelectionOutcome::Applied)
    }
}
