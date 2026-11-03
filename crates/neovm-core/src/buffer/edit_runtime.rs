//! Edit-domain runtime state embedded by value in the evaluator context.
//!
//! Stage-3 inversion of the buffer/eval split: the buffer manager is
//! edit-owned data. `eval::Context` contains this struct inline, so every
//! access stays a single field offset: no dispatch, no allocation, and no
//! thread-local.

use super::BufferManager;

/// Edit-owned runtime state carried by the evaluator.
///
/// `#[repr(C)]` with one field: containing it in `#[repr(C)] Context` keeps
/// the `BufferManager` at exactly the offset the field had before the
/// inversion, so the JIT-read Context offsets and `context_layout_test` stay
/// valid.
#[derive(Debug)]
#[repr(C)]
pub struct EditRuntime {
    /// Buffer manager — owns all live buffers and tracks the current buffer.
    pub buffers: BufferManager,
}

impl EditRuntime {
    /// Contain the buffer manager by value.
    pub fn new(buffers: BufferManager) -> Self {
        Self { buffers }
    }
}
