//! Allocator selection for the native loop of an admitted list HOF.
//!
//! The original bytecode keeps its `Call(2)`, so its shape cannot describe
//! the loop emitted by HOF lowering. Admission and profitability credit are
//! immutable compile-local metadata; this module adds no mutator runtime
//! state and uses the existing scoped allocator selection.

use super::lowering::{RegallocChoice, RegallocPolicy, choose_regalloc, forced_regalloc};
use super::{body_is_call_heavy, inline};
use crate::emacs_core::bytecode::opcode::Op;
use crate::emacs_core::value::Value;

/// Select after publishing the fused body, so its exact HOF admission table
/// supplies the same call credit used by the profitability gate. Bodies with
/// remaining call-heavy work retain their original allocator policy, as do
/// off, legacy and unadmitted bodies. Source-heavy metadata stays unchanged.
pub(crate) fn for_admitted_hof(
    fused: Option<&inline::FusedBody>,
    ops: &[Op],
    constants: &[Value],
) -> Option<RegallocChoice> {
    let side = fused?.v2.as_ref()?;
    if side.hof_at.is_empty() {
        return None;
    }
    select_hof_allocator(body_is_call_heavy(ops, constants), forced_regalloc())
}

fn select_hof_allocator(
    remaining_call_heavy: bool,
    forced: Option<RegallocChoice>,
) -> Option<RegallocChoice> {
    if remaining_call_heavy {
        return None;
    }
    // An admitted map loop runs unboundedly per entry, even when the source
    // bytecode has no backedge. Keep the existing forced-choice precedence.
    Some(choose_regalloc(forced, RegallocPolicy::Auto, true, false))
}

#[cfg(test)]
#[path = "../tests/inline_regalloc.rs"]
mod tests;
