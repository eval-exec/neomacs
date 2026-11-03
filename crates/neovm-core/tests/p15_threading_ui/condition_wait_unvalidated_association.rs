#[path = "../p15_threading_support/actual_payload.rs"]
mod actual_payload;
use actual_payload::{tagged, threading};
#[path = "../p15_threading_support/actual_condition_neighbors.rs"]
mod actual_condition_neighbors;
use actual_condition_neighbors::{
    ConditionId, MutexId, ThreadKey, Wait, api, emacs_core, heap_types,
};
#[path = "../../src/emacs_core/runtime/eval/green/condition_wait.rs"]
mod condition_wait;

fn unchecked_wait(
    condition: emacs_core::threads::CondVarHandle,
    mutex: emacs_core::threads::MutexHandle,
    depth: threading::RecursionDepth,
) -> condition_wait::ConditionWait {
    condition_wait::ConditionWait::Waiting(condition_wait::ValidatedWait {
        condition,
        mutex,
        depth,
    })
}

fn main() {}
