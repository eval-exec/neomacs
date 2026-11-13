//! Re-homed from eval's test tree (peer ruling S-1/S-2, 06:0xZ).
//!
//! `Context::new_minimal_vm_harness` is a V1 constructor and stays unwidened;
//! it is reached through eval's TA2 authority fence with the REAL boot proof
//! from core's test_utils (`CORE_EVAL_HOOK_SET`), never a forged
//! `EvalHooksInstalled::mint()`. The body and its selection/storage assertion
//! helper are byte-identical to the original apart from imports and the boot.

use crate::test_utils;
use neovm_edit::buffer::buffer::BufferBackingKindForTest;
use neovm_edit::buffer::buffer::BufferManager;
use neovm_edit::buffer::buffer::jit_layout;
use neovm_eval::test_support::authority::new_minimal_vm_harness;

#[test]
fn minimal_runtime_keeps_buffers_in_its_world_allocation() {
    let boot = test_utils::new_context();
    let context = new_minimal_vm_harness(boot.hooks_installed());
    assert_eq!(
        context.edit.buffers.backing_kind_for_test(),
        BufferBackingKindForTest::LeasedAttached
    );
    assert_selection_matches_storage(&context.edit.buffers);
    assert_eq!(context.edit.buffers.buffer_list().len(), 1);
}

fn assert_selection_matches_storage(manager: &BufferManager) {
    let expected = manager.current_buffer().map(std::ptr::from_ref);
    assert_eq!(jit_layout::current_buffer_ptr_via_walk(manager), expected);
    assert_eq!(
        jit_layout::current_buffer_ptr_via_projection(manager),
        expected
    );
    assert_eq!(
        manager.current_raw_for_test(),
        manager.current_buffer_id().map_or(0, |id| id.0)
    );
}
