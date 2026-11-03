//! Edit-domain runtime state embedded by value in the evaluator context.
//!
//! Stage-3 inversion of the buffer/eval split: the buffer manager and the
//! insdel.c change-combining state are edit-owned data. `eval::Context`
//! contains this struct inline, so every access stays a single field offset:
//! no dispatch, no allocation, and no thread-local.

use super::overlay::OverlayModificationHook;
use super::{BufferId, BufferManager};
use crate::emacs_core::value::Value;

/// Edit-owned runtime state carried by the evaluator.
///
/// `#[repr(C)]` with `buffers` as its first field: containing it in
/// `#[repr(C)] Context` keeps the `BufferManager` at exactly the offset the
/// field had before the regrouping, so the JIT-read Context offsets stay
/// valid; `context_layout_test` re-pins the offsets of the regrouped fields
/// and the containing struct.
#[derive(Debug)]
#[repr(C)]
pub struct EditRuntime {
    /// Buffer manager — owns all live buffers and tracks the current buffer.
    pub buffers: BufferManager,
    /// GNU `combine_after_change_buffer` (insdel.c): the buffer whose deferred
    /// after-change records are being accumulated.
    pub(crate) combine_after_change_buffer: Option<BufferId>,
    /// GNU `combine_after_change_list` (insdel.c): deferred after-change
    /// records, each a GNU `(charpos - BEG, Z - (charpos - lendel + lenins),
    /// lenins - lendel)` triple in 1-based character coordinates.
    pub(crate) combine_after_change_list: Vec<(i64, i64, i64)>,
    /// GNU `last_overlay_modification_hooks` (buffer.c): hook-list/overlay
    /// pairs recorded by the before-change overlay scan and replayed by the
    /// after-change scan.
    pub(crate) last_overlay_modification_hooks: Vec<OverlayModificationHook>,
    /// GNU `interval_insert_behind_hooks` (textprop.c): hook list recorded by
    /// `verify_interval_modification` before an insertion.
    pub(crate) interval_insert_behind_hooks: Value,
    /// GNU `interval_insert_in_front_hooks` (textprop.c): hook list recorded
    /// by `verify_interval_modification` before an insertion.
    pub(crate) interval_insert_in_front_hooks: Value,
}

impl EditRuntime {
    /// Contain the buffer manager by value.
    pub fn new(buffers: BufferManager) -> Self {
        Self {
            buffers,
            combine_after_change_buffer: None,
            combine_after_change_list: Vec::new(),
            last_overlay_modification_hooks: Vec::new(),
            interval_insert_behind_hooks: Value::NIL,
            interval_insert_in_front_hooks: Value::NIL,
        }
    }
}
