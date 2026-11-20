//! Owner allocation cursors. The bump writes the arena, not `heap.jit`.

use super::frames::FrameArena;
use crate::emacs_core::eval::Context;
use crate::test_utils::unit_runtime;
use neovm_value::tagged::gc::FLOAT_SLOT_BYTES;
use std::mem::size_of;

#[test]
fn two_owners_bump_disjoint_runs_and_leave_the_heap_window_closed() {
    let mut context = Context::new(unit_runtime());
    // SAFETY: this test thread owns the context. Closing the serial regions
    // is an in-place heap operation; the view is not moved.
    unsafe { context.tagged_heap_mut().close_alloc_regions() };
    let heap = context.tagged_heap();
    assert_eq!(heap.jit_cons_window_for_test(), (0, 0));
    assert_eq!(heap.jit_float_window_for_test(), (0, 0));

    let mut owner_a = FrameArena::new(1);
    let mut owner_b = FrameArena::new(1);
    let base_a = 0x1_0000;
    let base_b = 0x2_0000;
    let float_a = 0x3_0000;
    let float_b = 0x4_0000;
    owner_a.install_cons_free_list(base_a, 4);
    owner_b.install_cons_block_tail(base_b, 4, 7);
    owner_a.install_float_page_tail(float_a, 2, 0);
    owner_b.install_float_page_tail(float_b, 2, 1);

    let cons_a = owner_a.take_cons_cell().expect("owner A cons");
    let cons_b = owner_b.take_cons_cell().expect("owner B cons");
    let cons_a_next = owner_a.take_cons_cell().expect("owner A second cons");
    let float_first = owner_a.take_float_slot().expect("owner A float");
    let float_other = owner_b.take_float_slot().expect("owner B float");

    assert_eq!(cons_a, base_a);
    assert_eq!(cons_b, base_b);
    assert_ne!(cons_a, cons_b);
    assert_eq!(cons_a_next, base_a + size_of::<usize>() * 2);
    assert_eq!(float_first, float_a);
    assert_eq!(float_other, float_b);
    assert_eq!(
        owner_b.take_float_slot().expect("owner B second float"),
        float_b + FLOAT_SLOT_BYTES
    );
    // A bump that wrote `heap.jit` would move these windows or return the
    // heap cursor instead of the installed bases.
    assert_eq!(heap.jit_cons_window_for_test(), (0, 0));
    assert_eq!(heap.jit_float_window_for_test(), (0, 0));
    assert!(owner_b.take_cons_cell().expect("owner B still has its run") != cons_a_next);
}
