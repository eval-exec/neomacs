//! Context-aware conversion of allocation failures.
//!
//! `AllocationFailure::into_flow` stays in the allocation layer; resolving
//! GNU's `memory-signal-data` binding needs the evaluator, so that form of
//! the conversion lives above both.

use crate::emacs_core::alloc::AllocationFailure;
use crate::emacs_core::error::Flow;
use crate::emacs_core::intern::intern;

use super::Context;

impl AllocationFailure {
    pub(crate) fn into_flow_in_context(self, context: &Context) -> Flow {
        match self {
            Self::Lisp(flow) => flow,
            Self::MemoryExhausted(_) | Self::NullAllocation | Self::InvalidLayout(_) => context
                .special_variable_value_by_id(intern("memory-signal-data"))
                .map(|binding| {
                    crate::emacs_core::error::memory_signal_from_binding_value(
                        binding,
                        context.symbol_mode(),
                    )
                })
                .unwrap_or_else(crate::emacs_core::error::memory_exhausted_error),
        }
    }
}
