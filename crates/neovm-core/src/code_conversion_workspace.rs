//! Reusable scratch-buffer lifecycle for GNU-compatible code conversion.
//!
//! GNU centralizes this in `code_conversion_save` / `code_conversion_restore`
//! (`src/coding.c`).  Keeping the lifecycle in one Rust owner prevents coding
//! call sites from inventing buffer names, retention policies, or nested-call
//! cleanup independently.

use crate::buffer::{BufferId, BufferManager};
use crate::emacs_core::value::Value;
use crate::heap_types::LispString;

pub(crate) const CODE_CONVERSION_WORK_BUFFER_NAME: &str = " *code-conversion-work*";

/// The representation expected by one conversion's destination buffer.
///
/// A named enum keeps GNU's `coding->dst_multibyte` decision at the caller;
/// passing a bare bool here makes it too easy to reverse that meaning.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ConversionBufferEncoding {
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
pub(crate) struct CodeConversionBufferLease {
    buffer: BufferId,
    kind: LeaseKind,
}

impl CodeConversionBufferLease {
    pub(crate) const fn buffer_id(&self) -> BufferId {
        self.buffer
    }
}

/// Evaluator-owned state for GNU's code-conversion scratch buffers.
#[derive(Debug, Default)]
pub(crate) struct CodeConversionWorkspace {
    reusable: ReusableBufferState,
}

impl CodeConversionWorkspace {
    /// Acquire and initialize a work buffer for one conversion.
    ///
    /// The first/top-level conversion reuses the canonical hidden buffer.
    /// Reentrant conversions receive uniquely named temporary buffers, just
    /// as GNU does while `reused_workbuf_in_use` is set.
    pub(crate) fn acquire(
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
    pub(crate) fn release(
        &mut self,
        buffers: &mut BufferManager,
        lease: CodeConversionBufferLease,
    ) {
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

    pub(crate) fn replace_contents(
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

impl crate::emacs_core::eval::Context {
    /// Acquire through the evaluator so GNU's `Fmake_local_variable` side of
    /// `code_conversion_save` updates both the buffer and the symbol's
    /// localized forwarding metadata.
    pub(crate) fn acquire_code_conversion_buffer(
        &mut self,
        encoding: ConversionBufferEncoding,
    ) -> Result<CodeConversionBufferLease, crate::emacs_core::error::Flow> {
        let lease = self
            .code_conversion_workspace
            .acquire(&mut self.buffers, encoding);
        if let Err(flow) = self.set_buffer_local_binding_by_id(
            lease.buffer_id(),
            crate::emacs_core::intern::intern("inhibit-modification-hooks"),
            Value::T,
        ) {
            self.code_conversion_workspace
                .release(&mut self.buffers, lease);
            return Err(flow);
        }
        Ok(lease)
    }

    pub(crate) fn set_code_conversion_buffer_contents(
        &mut self,
        lease: &CodeConversionBufferLease,
        text: &LispString,
    ) {
        self.code_conversion_workspace
            .replace_contents(&mut self.buffers, lease, text);
    }

    pub(crate) fn release_code_conversion_buffer(&mut self, lease: CodeConversionBufferLease) {
        self.code_conversion_workspace
            .release(&mut self.buffers, lease);
    }
}

#[cfg(test)]
#[path = "code_conversion_workspace/tests/code_conversion_workspace_test.rs"]
mod tests;
