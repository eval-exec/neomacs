//! Heap-pointer admission for hidden list callback loops.
//! Threading: this reads the compiling mutator's immutable fused annotations;
//! no heap pointer or Lisp value is cached across compilations or mutators.

use crate::emacs_core::bytecode::opcode::Op;
use crate::emacs_core::jit::inline;

/// A guarded callback store runs inside the generated mapping loop, even
/// when its physical caller has neither a store nor a bytecode backedge.
/// This admits only hoisting Context's immutable heap box pointer; barrier
/// windows and allocation cursors remain fresh at their individual sites.
pub(crate) fn hoist_callback_heap_ptr() -> bool {
    inline::active_fused()
        .and_then(|body| {
            body.v2.as_ref().map(|side| {
                side.hof_at.values().any(|site| {
                    site.callback.get_bytecode_data().is_some_and(|code| {
                        code.executable_ops()
                            .iter()
                            .any(|op| matches!(op, Op::Setcar | Op::Setcdr))
                    })
                })
            })
        })
        .unwrap_or(false)
}

#[cfg(test)]
#[path = "../../tests/inline_hof_heap_policy_test.rs"]
mod tests;
