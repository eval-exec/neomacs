//! Tree-sitter boundary for hosts without native grammar modules.

use crate::buffer::{Buffer, BufferId, EmacsBytePos, EmacsByteRange};
use crate::emacs_core::value::Value;

#[derive(Default)]
pub(crate) struct TreeSitterManager;

impl TreeSitterManager {
    pub(crate) fn new() -> Self {
        Self
    }

    pub(crate) fn roots(&self) -> Vec<Value> {
        Vec::new()
    }

    pub(crate) fn has_editable_tree(&self, _buffer_id: BufferId) -> bool {
        false
    }

    pub(crate) fn begin_buffer_edit(
        &mut self,
        _buffer_id: BufferId,
        _buffer: &Buffer,
        _old_range: EmacsByteRange,
    ) {
    }

    pub(crate) fn note_buffer_change(&mut self, _buffer_id: BufferId, _beg: EmacsBytePos) {}

    pub(crate) fn has_pending_edit(&self, _buffer_id: BufferId) -> bool {
        false
    }

    pub(crate) fn finish_buffer_edit(
        &mut self,
        _buffer_id: BufferId,
        _buffer: &Buffer,
        _end: EmacsBytePos,
    ) {
    }
}
