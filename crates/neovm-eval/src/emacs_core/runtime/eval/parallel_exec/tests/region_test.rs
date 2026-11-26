//! Owner cons allocation cursors. The bump writes the arena, not `heap.jit`.
//!
//! Parallel owner regions are cons-only; parallel float/bignum allocation uses
//! N2 (`NumericStorage`). The serial/JIT float bump window on `TaggedHeap`
//! stays intact and untouched by parallel owner bumps.

use super::frames::{FrameArena, OwnerScanJoin, RootGroup};
use crate::emacs_core::eval::Context;
use crate::test_utils::unit_runtime;
use neovm_value::tagged::gc::{ConsTailRefund, RendezvousError};
use neovm_value::tagged::transport::SharedRoot;
use neovm_value::value::Value;
use std::mem::size_of;

#[test]
fn two_owners_bump_disjoint_cons_runs_and_leave_the_heap_windows_closed() {
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
    owner_a.install_cons_free_list(base_a, 4);
    owner_b.install_cons_block_tail(base_b, 4, 7);

    let cons_a = owner_a.take_cons_cell().expect("owner A cons");
    let cons_b = owner_b.take_cons_cell().expect("owner B cons");
    let cons_a_next = owner_a.take_cons_cell().expect("owner A second cons");

    assert_eq!(cons_a, base_a);
    assert_eq!(cons_b, base_b);
    assert_ne!(cons_a, cons_b);
    assert_eq!(cons_a_next, base_a + size_of::<usize>() * 2);
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
    owner.install_cons_free_list(base, 4);
    owner
        .roots_mut()
        .push(RootGroup::Misc, Value::fixnum(22))
        .expect("root");

    let first = owner.take_cons_cell().expect("first cons");
    owner.on_nonblocking_poll();
    let second = owner.take_cons_cell().expect("region survived the poll");
    assert_eq!(first, base);
    assert_eq!(second, base + cons_stride());

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
    assert!(owner.take_cons_cell().is_none());
    owner
        .publish_scan(heap, &mut out)
        .expect("closed book publishes");
    assert_eq!(out.len(), 2);
    assert_eq!(out[1].parallel_word(), Value::fixnum(22).bits());
    assert_eq!(heap.jit_cons_window_for_test(), (0, 0));
    assert_eq!(heap.jit_float_window_for_test(), (0, 0));
}

#[test]
fn stop_close_rewinds_a_block_tail_and_is_idempotent() {
    let mut owner = FrameArena::new(1);
    let base = 0x7_0000;
    owner.install_cons_block_tail(base, 3, 11);
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
    assert!(owner.take_cons_cell().is_none());
    let again = owner.close_region_for_stop();
    assert_eq!(again.cons, ConsTailRefund::None);
}

#[test]
fn serial_jit_float_window_stays_intact_across_owner_cons_bumps() {
    let mut context = Context::new(unit_runtime());
    // SAFETY: this test thread owns the context.
    unsafe { context.tagged_heap_mut().close_alloc_regions() };
    assert_eq!(context.tagged_heap().jit_float_window_for_test(), (0, 0));

    // SAFETY: this test thread owns the context and allocates one serial float.
    let f = unsafe { context.tagged_heap_mut().alloc_float_inline(3.25) };
    assert_eq!(f.as_float(), Some(3.25));
    let opened = context.tagged_heap().jit_float_window_for_test();
    assert_ne!(
        opened,
        (0, 0),
        "serial float bump window must remain active"
    );
    assert!(opened.0 <= opened.1);

    let mut owner = FrameArena::new(1);
    owner.install_cons_free_list(0x9_0000, 2);
    assert_eq!(owner.take_cons_cell(), Some(0x9_0000));
    assert_eq!(
        context.tagged_heap().jit_float_window_for_test(),
        opened,
        "owner cons bump must not disturb serial/JIT float window"
    );
    let _ = owner.close_region_for_stop();
    assert_eq!(context.tagged_heap().jit_float_window_for_test(), opened);

    // SAFETY: this test thread owns the context.
    unsafe { context.tagged_heap_mut().close_alloc_regions() };
    assert_eq!(context.tagged_heap().jit_float_window_for_test(), (0, 0));
}
