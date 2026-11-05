use neovm_core::buffer::buffer::BufferManager;

fn main() {
    // Post-split the factory is `pub unsafe fn` + `#[doc(hidden)]` (EDIT
    // ledger row BufferManager::from_world_parts) because the evaluator's
    // World owner now lives in neovm-eval. The remaining refusal is the
    // unsafe call: a safe caller cannot forge a manager without an unsafe
    // block and a sealed ColdWorldLease.
    let _factory = BufferManager::from_world_parts(todo!(), todo!(), todo!());
}
