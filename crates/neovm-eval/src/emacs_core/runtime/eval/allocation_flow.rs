//! Context-aware conversion of allocation failures.
//!
//! `AllocationFailure::into_flow` stays in the allocation layer; resolving
//! GNU's `memory-signal-data` binding needs the evaluator, so that form of
//! the conversion lives above both.

use neovm_value::alloc::AllocationFailure;
use neovm_value::error::Flow;
use neovm_value::intern::intern;

use super::Context;

#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) trait AllocationFailureContextExt {
    fn into_flow_in_context(self, context: &Context) -> Flow;
}

impl AllocationFailureContextExt for AllocationFailure {
    fn into_flow_in_context(self, context: &Context) -> Flow {
        match self {
            Self::Lisp(flow) => flow,
            Self::MemoryExhausted(_) | Self::NullAllocation | Self::InvalidLayout(_) => context
                .special_variable_value_by_id(intern("memory-signal-data"))
                .map(|binding| {
                    neovm_value::error::memory_signal_from_binding_value(
                        binding,
                        context.symbol_mode(),
                    )
                })
                .unwrap_or_else(neovm_value::error::memory_exhausted_error),
        }
    }
}
