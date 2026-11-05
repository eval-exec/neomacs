//! Reusable scratch-buffer lifecycle for GNU-compatible code conversion.
//!
//! GNU centralizes this in `code_conversion_save` / `code_conversion_restore`
//! (`src/coding.c`).  Keeping the lifecycle in one Rust owner prevents coding
//! call sites from inventing buffer names, retention policies, or nested-call
//! cleanup independently.

use neovm_edit::buffer::{BufferId, BufferManager};
use neovm_value::heap_types::LispString;
use neovm_value::value::Value;

pub(crate) const CODE_CONVERSION_WORK_BUFFER_NAME: &str = " *code-conversion-work*";

/// The representation expected by one conversion's destination buffer.
///
/// A named enum keeps GNU's `coding->dst_multibyte` decision at the caller;
/// passing a bare bool here makes it too easy to reverse that meaning.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConversionBufferEncoding {
    Unibyte,
    Multibyte,
}

impl ConversionBufferEncoding {
    const fn is_multibyte(self) -> bool {
        matches!(self, Self::Multibyte)
    }
}

/// The only valid states of GNU's reusable buffer plus
/// `reused_workbuf_in_use` flag.
///
/// Modeling the pair as an enum rules out contradictory combinations such as
/// "busy but no reusable buffer" at compile time.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum ReusableBufferState {
    #[default]
    Unallocated,
    Available(BufferId),
    InUse(BufferId),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LeaseKind {
    Reusable,
    NestedTemporary,
}

/// Capability returned for one active code conversion.
///
/// The private kind forces release to distinguish the persistent reusable
/// buffer from a nested temporary; callers cannot accidentally kill the
/// former or retain the latter.
#[derive(Debug)]
pub struct CodeConversionBufferLease {
    buffer: BufferId,
    kind: LeaseKind,
}

impl CodeConversionBufferLease {
    #[doc(hidden)]
    pub const fn buffer_id(&self) -> BufferId {
        self.buffer
    }
}

/// Evaluator-owned state for GNU's code-conversion scratch buffers.
#[derive(Debug, Default)]
#[doc(hidden)]
pub struct CodeConversionWorkspace {
    reusable: ReusableBufferState,
}

impl CodeConversionWorkspace {
    /// Acquire and initialize a work buffer for one conversion.
    ///
    /// The first/top-level conversion reuses the canonical hidden buffer.
    /// Reentrant conversions receive uniquely named temporary buffers, just
    /// as GNU does while `reused_workbuf_in_use` is set.
    #[doc(hidden)]
    pub fn acquire(
        &mut self,
        buffers: &mut BufferManager,
        encoding: ConversionBufferEncoding,
    ) -> CodeConversionBufferLease {
        let previous = std::mem::take(&mut self.reusable);
        let (buffer, kind, next_state) = match previous {
            ReusableBufferState::Unallocated => {
                let id = buffers
                    .find_buffer_by_name(CODE_CONVERSION_WORK_BUFFER_NAME)
                    .unwrap_or_else(|| {
                        buffers.create_buffer_with_hook_inhibition(
                            CODE_CONVERSION_WORK_BUFFER_NAME,
                            true,
                        )
                    });
                (id, LeaseKind::Reusable, ReusableBufferState::InUse(id))
            }
            ReusableBufferState::Available(id) if buffers.get(id).is_some() => {
                (id, LeaseKind::Reusable, ReusableBufferState::InUse(id))
            }
            ReusableBufferState::Available(_) => {
                let id = buffers
                    .find_buffer_by_name(CODE_CONVERSION_WORK_BUFFER_NAME)
                    .unwrap_or_else(|| {
                        buffers.create_buffer_with_hook_inhibition(
                            CODE_CONVERSION_WORK_BUFFER_NAME,
                            true,
                        )
                    });
                (id, LeaseKind::Reusable, ReusableBufferState::InUse(id))
            }
            ReusableBufferState::InUse(id) => {
                let name = buffers.generate_new_buffer_name(CODE_CONVERSION_WORK_BUFFER_NAME);
                let temporary = buffers.create_buffer_with_hook_inhibition(&name, true);
                (
                    temporary,
                    LeaseKind::NestedTemporary,
                    ReusableBufferState::InUse(id),
                )
            }
        };
        self.reusable = next_state;
        prepare_work_buffer(buffers, buffer, encoding);
        CodeConversionBufferLease { buffer, kind }
    }

    /// Release one active conversion, retaining only the canonical buffer.
    #[doc(hidden)]
    pub fn release(&mut self, buffers: &mut BufferManager, lease: CodeConversionBufferLease) {
        let buffer = lease.buffer_id();
        match lease.kind {
            LeaseKind::Reusable => {
                debug_assert!(matches!(
                    self.reusable,
                    ReusableBufferState::InUse(id) if id == buffer
                ));
                self.reusable = ReusableBufferState::Available(buffer);
            }
            LeaseKind::NestedTemporary => {
                let _ = buffers.kill_buffer(buffer);
            }
        }
    }

    #[doc(hidden)]
    pub fn replace_contents(
        &self,
        buffers: &mut BufferManager,
        lease: &CodeConversionBufferLease,
        text: &LispString,
    ) {
        buffers
            .replace_buffer_contents_lisp_string(lease.buffer_id(), text)
            .expect("an active code-conversion lease owns a live buffer");
    }
}

fn prepare_work_buffer(
    buffers: &mut BufferManager,
    buffer: BufferId,
    encoding: ConversionBufferEncoding,
) {
    let multibyte = encoding.is_multibyte();
    if let Some(work) = buffers.get_mut(buffer) {
        // Hidden buffers start with undo disabled; repeat the assignment for a
        // user-created canonical buffer and every reuse.
        work.set_undo_list(Value::T);
    }
    buffers
        .replace_buffer_contents(buffer, "")
        .expect("newly acquired code-conversion buffer must be live");
    buffers
        .set_buffer_multibyte_flag(buffer, multibyte)
        .expect("newly acquired code-conversion buffer must be live");
}

#[cfg(test)]
#[path = "code_conversion_workspace/tests/code_conversion_workspace_test.rs"]
mod tests;
