//! Re-homed from the edit buffer storage oracle
//! (`crates/neovm-core/.../moved_edit/buffer/buffer/tests/world_storage_test.rs`).
//!
//! `Context::new_minimal_vm_harness` is a V1 constructor: it stays
//! `#[cfg(test)]` in this crate and is never widened for dependency builds, so
//! its oracle lives in eval's own test tree (LEAD 05:4xZ(1)). The test body and
//! its selection/storage assertion helper are byte-identical to the original
//! apart from imports, the explicit `EvalHooksInstalled` construction proof,
//! and paths.

use crate::emacs_core::eval::Context;
use crate::emacs_core::eval::hooks_proof::EvalHooksInstalled;
use neovm_edit::buffer::buffer::BufferBacking;
use neovm_edit::buffer::buffer::BufferManager;
use neovm_edit::buffer::buffer::jit_layout;

#[test]
fn minimal_runtime_keeps_buffers_in_its_world_allocation() {
    let context = Context::new_minimal_vm_harness(EvalHooksInstalled::mint());
    assert!(matches!(
        context.edit.buffers.backing,
        BufferBacking::Leased { detached: None, .. }
    ));
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
        manager.current_raw,
        manager.current_buffer_id().map_or(0, |id| id.0)
    );
}
