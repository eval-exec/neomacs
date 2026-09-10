//! Shared VM insertion context and transient input-method state.

use super::{composition::ActiveComposition, surrounding::SurroundingTextState};
use crate::buffer::{BufferId, EmacsBytePos};

/// A revision-qualified insertion anchor. Any unrelated edit invalidates it;
/// we never guess where an old platform offset belongs in a newer buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct InsertionAnchor {
    pub(super) buffer: BufferId,
    pub(super) window: Option<crate::window::WindowId>,
    pub(super) point: EmacsBytePos,
    pub(super) accessible: crate::buffer::AccessibleEmacsByteRange,
    pub(super) revision: i64,
    pub(super) selection_anchor: Option<EmacsBytePos>,
    pub(super) multibyte: bool,
}

/// Transient VM state, deliberately excluded from runtime images.
#[derive(Debug, Default)]
pub(crate) struct CompositionState {
    pub(super) latest: u64,
    pub(super) operation_revision: u64,
    pub(super) active: Option<ActiveComposition>,
    pub(super) surrounding: SurroundingTextState,
}

impl crate::Context {
    pub(super) fn ime_anchor(&self) -> Option<InsertionAnchor> {
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
}
