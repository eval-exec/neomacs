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

fn main() {
    // This resolves to the complete real wait implementation, including its
    // private validated unlock, phase transitions, reacquire and Drop paths.
    let _: fn(
        &mut emacs_core::eval::Context,
        emacs_core::threads::CondVarHandle,
    ) -> emacs_core::error::EvalResult = condition_wait::wait;
    let _completed = condition_wait::ConditionWait::Completed;
}
