//! Snapshot-qualified replacement through the VM's normal change machinery.

use crate::Value;
use crate::buffer::{EmacsByteLen, EmacsBytePos, EmacsByteRange};
use crate::emacs_core::{editfns, error::signal, textprop};
use crate::heap_types::LispString;
use neovm_host_abi::ime::{ImeReplacement, ImeReplacementOutcome};

impl crate::Context {
    pub(super) fn ime_replace(
        &mut self,
        request: ImeReplacement,
    ) -> Result<(ImeReplacementOutcome, Option<Value>), crate::Flow> {
        use ImeReplacementOutcome::{Applied, InvalidCursor, InvalidRange, StaleSnapshot};
        let Some(captured) = self.composition.surrounding.captured.as_ref() else {
            return Ok((StaleSnapshot, None));
        };
        if captured.id != request.snapshot {
            return Ok((StaleSnapshot, None));
        }
        // Consume authority before calling Lisp. Neither an error nor reentry
        // permits replaying this request against a different editor state.
        let captured = self.composition.surrounding.captured.take().unwrap();
        if self.ime_export_revision() != Some(captured.export_revision)
            || self.ime_anchor() != Some(captured.source)
        {
            return Ok((StaleSnapshot, None));
        }
        let locate = |offset| {
            captured
                .positions
                .iter()
                .find(|(byte, _)| *byte == offset)
                .map(|(_, position)| *position)
        };
        let (Some(start), Some(end)) = (locate(request.start), locate(request.end)) else {
            return Ok((InvalidRange, None));
        };
        if start > end {
            return Ok((InvalidRange, None));
        }
        let buffer = self
            .buffers
            .current_buffer()
            .expect("validated snapshot context");
        let multibyte = buffer.get_multibyte();
        let encode = |text: &str| {
            crate::emacs_core::buffer::buffer_insert_lisp_string_from_lisp_string(
                &LispString::from_utf8(text),
                multibyte,
            )
        };
        let replacement = encode(&request.text);
        let Some(insert_end) = request.start.checked_add(request.text.len()) else {
            return Ok((InvalidCursor, None));
        };
        // Resolve the projected UTF-8 coordinate against the original mapping
        // and target buffer encoding. UTF-8 length need not equal Emacs bytes,
        // especially for a unibyte target.
        let cursor: Option<EmacsBytePos> = if request.cursor < request.start {
            locate(request.cursor)
        } else if request.cursor <= insert_end {
            let prefix = request.cursor - request.start;
            request
                .text
                .get(..prefix)
                .map(|text| start.add_len(EmacsByteLen::new(encode(text).sbytes())))
        } else {
            request
                .cursor
                .checked_sub(insert_end)
                .and_then(|suffix| request.end.checked_add(suffix))
                .and_then(locate)
                .map(|original| {
                    start.add_len(EmacsByteLen::new(
                        replacement.sbytes() + original.get() - end.get(),
                    ))
                })
        };
        let Some(cursor) = cursor else {
            return Ok((InvalidCursor, None));
        };
        if editfns::buffer_read_only_active_in_state(&self.obarray, &[], buffer) {
            return Err(signal(
                "buffer-read-only",
                vec![Value::make_buffer(captured.source.buffer)],
            ));
        }
        textprop::verify_text_read_only_in_state(
            &self.obarray,
            &self.buffers,
            captured.source.buffer,
            start.get(),
            end.get(),
        )?;
        let range = EmacsByteRange::new(start, end);
        let start_char = buffer.emacs_byte_pos_to_lisp_char_pos(start).as_i64();
        let change = editfns::text_change_for_lisp_string_replacement_in_manager(
            &self.buffers,
            captured.source.buffer,
            range,
            &replacement,
        )?;
        self.composition.active = None;
        self.composition.operation_revision = self.composition.operation_revision.wrapping_add(1);
        let operation_revision = self.composition.operation_revision;
        // GNU really_replace_text deactivates mark through Lisp, then changes
        // text and point. Revalidate after every pre-change Lisp boundary.
        self.apply0(Value::symbol("deactivate-mark"))?;
        let expected = super::InsertionAnchor {
            selection_anchor: None,
            ..captured.source
        };
        if self.ime_anchor() != Some(expected)
            || self.ime_export_revision() != Some(captured.export_revision)
            || self.composition.operation_revision != operation_revision
        {
            return Ok((StaleSnapshot, None));
        }
        editfns::signal_before_text_change(self, change)?;
        if self.ime_anchor() != Some(expected)
            || self.ime_export_revision() != Some(captured.export_revision)
            || self.composition.operation_revision != operation_revision
        {
            return Ok((StaleSnapshot, None));
        }
        self.buffers
            .replace_buffer_measured_region_lisp_string(
                captured.source.buffer,
                change.old_range(),
                &replacement,
            )
            .expect("validated replacement range");
        crate::emacs_core::buffer::apply_inherited_text_properties(
            &self.obarray,
            &[],
            &mut self.buffers,
            captured.source.buffer,
            start.get(),
            replacement.sbytes(),
        );
        self.buffers
            .get_mut(captured.source.buffer)
            .expect("validated replacement buffer")
            .goto_emacs_byte_pos(cursor);
        use super::conversion::ConversionEdit;
        let at = crate::buffer::LispCharPos1::new(start_char);
        let deletion = (start != end).then_some(ConversionEdit::EphemeralDeletion { at });
        let insertion = (!replacement.is_empty()).then_some(ConversionEdit::Insertion {
            at,
            text: replacement,
        });
        let event = self.publish_conversion_edits(
            captured.source.buffer,
            deletion.into_iter().chain(insertion),
        );
        editfns::signal_after_text_change(self, change)?;
        Ok((Applied, (!event.is_nil()).then_some(event)))
    }
}
