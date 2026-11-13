#[path = "../p15_threading_support/actual_payload.rs"]
mod actual_payload;
use actual_payload::{tagged, threading, value};
#[path = "../p15_threading_support/actual_condition_neighbors.rs"]
mod actual_condition_neighbors;
use actual_condition_neighbors::{
    ConditionId, MutexId, ThreadKey, Wait, api, emacs_core, heap_types,
};
#[path = "../../../neovm-eval/src/emacs_core/runtime/eval/green/condition_wait.rs"]
mod condition_wait;

fn unchecked_wait(
    condition: emacs_core::threads::CondVarHandle,
    mutex: emacs_core::threads::MutexHandle,
    // The real condition_wait.rs imports the real neovm_value recursion
    // depth. Using the fixture-local mock type here made the construction
    // fail on a type mismatch (E0308) before rustc could report the private
    // fields (E0451), which would hide a field-privacy regression.
    depth: neovm_value::heap_types::threading::RecursionDepth,
) -> condition_wait::ConditionWait {
    condition_wait::ConditionWait::Waiting(condition_wait::ValidatedWait {
        condition,
        mutex,
        depth,
    })
}

fn main() {}
