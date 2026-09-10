//! VM-owned rules for disclosing editor text to an input method.

use crate::buffer::{EmacsByteRange, LispCharPos1};
use crate::emacs_core::intern::intern;

/// Property-only changes can move input fields without changing text identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ExportRevision {
    properties: i64,
    overlays: i64,
}

impl crate::Context {
    pub(super) fn ime_export_revision(&self) -> Option<ExportRevision> {
        if self.ime_text_is_private() {
            return None;
        }
        let buffer = self.buffers.current_buffer()?;
        Some(ExportRevision {
            properties: buffer.props_modified_tick(),
            overlays: buffer.overlay_modified_tick(),
        })
    }

    pub(super) fn ime_field_bounds(&self) -> Option<EmacsByteRange> {
        // Use the same field/stickiness/overlay semantics as field-beginning
        // and field-end, without evaluating arbitrary Lisp or changing narrowing.
        let (start, end) = crate::emacs_core::buffer::find_field_bounds_in_state(
            &self.obarray,
            &[],
            &self.buffers,
            None,
            false,
            None,
            None,
        )
        .ok()?;
        let buffer = self.buffers.current_buffer()?;
        Some(EmacsByteRange::new(
            buffer.lisp_pos_to_accessible_emacs_byte_pos(LispCharPos1::new(start)),
            buffer.lisp_pos_to_accessible_emacs_byte_pos(LispCharPos1::new(end)),
        ))
    }

    pub(super) fn ime_text_is_private(&self) -> bool {
        let value = |name| self.special_variable_value_by_id(intern(name));
        // GNU auth-source.el::read-passwd binds both of these dynamically.
        // Consult VM bindings, never a frontend copy of minibuffer state.
        if value("read-hide-char").is_none_or(|value| value.is_truthy()) {
            return true;
        }
        let Some(style) = value("overriding-text-conversion-style") else {
            return true;
        };
        let style = if style.is_symbol_named("lambda") {
            value("text-conversion-style")
        } else {
            Some(style)
        };
        style.is_none_or(|value| value.is_symbol_named("password"))
    }
}
