//! Owner allocation cursors. The bump writes the arena, not `heap.jit`.

use super::frames::{FrameArena, OwnerScanJoin, RootGroup};
use crate::emacs_core::eval::Context;
use crate::test_utils::unit_runtime;
use neovm_value::tagged::gc::{ConsTailRefund, FLOAT_SLOT_BYTES, FloatTailRefund, RendezvousError};
use neovm_value::tagged::transport::SharedRoot;
use neovm_value::value::Value;
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

fn cons_stride() -> usize {
    size_of::<usize>() * 2
}

/// A poll that does not block leaves the run. Only the publication that
/// precedes a stop closes it, and an open book does not publish.
#[test]
fn nonblocking_poll_keeps_the_region_and_stop_publication_closes_it() {
    let mut context = Context::new(unit_runtime());
    // SAFETY: this test thread owns the context. Closing the serial regions
    // is an in-place heap operation; the view is not moved.
    unsafe { context.tagged_heap_mut().close_alloc_regions() };
    let mut owner = FrameArena::new(1);
    let base = 0x5_0000;
    let float_base = 0x6_0000;
    owner.install_cons_free_list(base, 4);
    owner.install_float_page_tail(float_base, 3, 2);
    owner
        .roots_mut()
        .push(RootGroup::Misc, Value::fixnum(22))
        .expect("root");

    let first = owner.take_cons_cell().expect("first cons");
    owner.on_nonblocking_poll();
    let second = owner.take_cons_cell().expect("region survived the poll");
    assert_eq!(first, base);
    assert_eq!(second, base + cons_stride());
    assert_eq!(
        owner.take_float_slot().expect("float survived the poll"),
        float_base
    );

    let heap = context.tagged_heap();
    let mut out = Vec::new();
    // SAFETY: the fixnum is immediate. This thread is the heap's mutator.
    out.push(unsafe { SharedRoot::new(heap, Value::fixnum(9)) });
    let prefix = out[0].parallel_word();
    let error = owner.publish_scan(heap, &mut out).expect_err("open book");
    assert_eq!(error, RendezvousError::Publication);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].parallel_word(), prefix);
    assert_eq!(
        owner
            .take_cons_cell()
            .expect("refused publish did not close"),
        base + 2 * cons_stride()
    );

    let refund = owner.close_region_for_stop();
    assert_eq!(
        refund.cons,
        ConsTailRefund::FreeList {
            first: base + 3 * cons_stride(),
            cells: 1,
        }
    );
    assert_eq!(
        refund.float,
        FloatTailRefund::PageTail {
            page: 2,
            first: float_base + FLOAT_SLOT_BYTES,
            slots: 2,
        }
    );
    assert!(owner.take_cons_cell().is_none());
    assert!(owner.take_float_slot().is_none());
    owner
        .publish_scan(heap, &mut out)
        .expect("closed book publishes");
    assert_eq!(out.len(), 2);
    assert_eq!(out[1].parallel_word(), Value::fixnum(22).bits());
    assert_eq!(heap.jit_cons_window_for_test(), (0, 0));
    assert_eq!(heap.jit_float_window_for_test(), (0, 0));
}

#[test]
fn stop_close_rewinds_a_block_tail_and_returns_a_float_free_run() {
    let mut owner = FrameArena::new(1);
    let base = 0x7_0000;
    let float_base = 0x8_0000;
    owner.install_cons_block_tail(base, 3, 11);
    owner.install_float_page_free(float_base, 4, 5);
    assert_eq!(owner.take_cons_cell().expect("one cell"), base);
    let refund = owner.close_region_for_stop();
    assert_eq!(
        refund.cons,
        ConsTailRefund::Rewind {
            block: 11,
            first: base + cons_stride(),
            cells: 2,
        }
    );
    assert_eq!(
        refund.float,
        FloatTailRefund::PageFree {
            page: 5,
            first: float_base,
            slots: 4,
        }
    );
    assert!(owner.take_cons_cell().is_none());
    let again = owner.close_region_for_stop();
    assert_eq!(again.cons, ConsTailRefund::None);
    assert_eq!(again.float, FloatTailRefund::None);
}
