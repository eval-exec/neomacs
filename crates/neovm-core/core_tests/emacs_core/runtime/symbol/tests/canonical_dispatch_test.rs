//! The grouped cells remain canonical through construction, writes and cold
//! replacement. Borrow-model controls live in tmp/codex/p0s1/model.
use super::super::intern::intern;
use super::*;
use std::cell::UnsafeCell;
use std::ptr::NonNull;

#[test]
fn canonical_pair_layout_reaches_the_same_clock_and_descriptor_slots() {
    let obarray = Obarray::new();
    let base = std::ptr::from_ref(&obarray).cast::<u8>();
    // SAFETY: all offsets are derived from the actual containing types. This
    // pure read phase borrows the live Obarray and no mutable loan exists.
    let pair = unsafe {
        &*base
            .add(OBARRAY_DISPATCH_CELLS_OFFSET)
            .cast::<CanonicalDispatchCells>()
    };
    assert_eq!(
        std::ptr::from_ref(pair).addr(),
        obarray.dispatch_cells_address_for_test()
    );
    assert_eq!(
        OBARRAY_FUNCTION_EPOCH_OFFSET,
        OBARRAY_DISPATCH_CELLS_OFFSET + DISPATCH_FUNCTION_EPOCH_OFFSET
    );
    assert_eq!(
        OBARRAY_DEBUG_ON_NEXT_CALL_FWD_OFFSET,
        OBARRAY_DISPATCH_CELLS_OFFSET + DISPATCH_DEBUG_FWD_OFFSET
    );
    assert_eq!(pair.function_epoch(), obarray.function_epoch());
    assert_eq!(
        pair.debug_on_next_call_armed_fast(),
        obarray.debug_on_next_call_armed_fast()
    );
}

#[test]
fn canonical_pair_tracks_function_writes_and_restored_clocks() {
    let mut obarray = Obarray::new();
    let id = intern("world-canonical-function-clock");
    let before = obarray.dispatch_cells_function_epoch_for_test();
    obarray.set_symbol_function_id(id, Value::fixnum(41));
    let written = obarray.function_epoch();
    assert!(written > before);
    assert_eq!(obarray.dispatch_cells_function_epoch_for_test(), written);
    obarray.invalidate_all_function_bindings(FunctionEpochBump::InternalCellWrite);
    assert_eq!(
        obarray.dispatch_cells_function_epoch_for_test(),
        obarray.function_epoch()
    );
    let clone = obarray.clone();
    assert_eq!(
        clone.dispatch_cells_function_epoch_for_test(),
        obarray.function_epoch()
    );
    let restored =
        Obarray::from_dump(Vec::new(), Vec::new(), Vec::new(), 777).expect("empty dump restores");
    assert_eq!(restored.dispatch_cells_function_epoch_for_test(), 777);
    assert!(restored.debug_on_next_call_armed_fast());
}

#[test]
fn canonical_pair_pointer_from_allocation_root_survives_cold_replacement() {
    let storage = Box::new(UnsafeCell::new(
        Obarray::from_dump(Vec::new(), Vec::new(), Vec::new(), 6).expect("empty dump restores"),
    ));
    let manager = storage.get();
    // SAFETY: the designator is derived from the final UnsafeCell allocation
    // root through the containing type's layout constant, so no temporary
    // reference to the interior-mutable manager is created; storage owns the
    // allocation for the whole test.
    let cells = NonNull::new(unsafe {
        manager
            .cast::<u8>()
            .add(OBARRAY_DISPATCH_CELLS_OFFSET)
            .cast::<CanonicalDispatchCells>()
    })
    .expect("live allocation subfield");
    let id = intern("world-canonical-cold-replacement");
    // SAFETY: one owner, short exclusive write phase, no canonical reader is
    // live. The real writer moves the restored clock from 6 to 7.
    unsafe { (&mut *manager).set_symbol_function_id(id, Value::fixnum(41)) };
    // SAFETY: the completed mutable write phase precedes this canonical read.
    assert_eq!(unsafe { cells.as_ref() }.function_epoch(), 7);
    // SAFETY: cold full replacement retains the allocation and field address;
    // no generated symbol pointer or manager reference remains active.
    unsafe {
        *manager = Obarray::from_dump(Vec::new(), Vec::new(), Vec::new(), 13)
            .expect("empty dump restores");
    }
    // SAFETY: storage is live; the replacement write phase has completed.
    let pair = unsafe { cells.as_ref() };
    assert_eq!(pair.function_epoch(), 13);
    assert!(pair.debug_on_next_call_armed_fast());
}

#[test]
fn canonical_pair_preserves_completed_writer_updates_after_rust_panic() {
    let mut obarray =
        Obarray::from_dump(Vec::new(), Vec::new(), Vec::new(), 17).expect("empty dump restores");
    let snapshot = neovm_value::test_support::authority::snapshot_function_epoch_for_test(&obarray);
    // A real canonical writer moves the clock, so the restore below is an
    // observable completed update rather than a no-op.
    let id = intern("world-canonical-panic-clock");
    obarray.set_symbol_function_id(id, Value::fixnum(1));
    assert_ne!(obarray.function_epoch(), 17);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        neovm_value::test_support::authority::restore_function_epoch_for_test(
            &mut obarray,
            snapshot,
        );
        panic!("permitted canonical-writer panic");
    }));
    assert!(result.is_err());
    assert_eq!(obarray.dispatch_cells_function_epoch_for_test(), 17);
}

#[test]
fn canonical_pair_follows_live_boolean_and_clone_owns_its_descriptor() {
    crate::test_utils::init_test_tracing();
    let mut obarray = Obarray::new();
    let id = intern("debug-on-next-call");
    assert!(obarray.debug_on_next_call_armed_fast());
    assert!(
        neovm_value::test_support::authority::debug_on_next_call_bool_fwd_for_test(
            &mut obarray,
            id
        )
        .is_none()
    );
    assert!(!obarray.debug_on_next_call_armed_fast());
    obarray.define_bool_variable("debug-on-next-call", false, ByteBooleanVars::Listed);
    let source = obarray.bool_forwarder(id).expect("installed Boolean cell");
    source.set(true);
    assert!(obarray.debug_on_next_call_armed_fast());
    let mut cloned = obarray.clone();
    assert!(cloned.debug_on_next_call_armed_fast());
    let clone_cell =
        neovm_value::test_support::authority::debug_on_next_call_bool_fwd_for_test(&mut cloned, id)
            .expect("clone resolves its own forwarded cell");
    assert!(!std::ptr::eq(source, clone_cell));
    clone_cell.set(false);
    assert!(obarray.debug_on_next_call_armed_fast());
    assert!(!cloned.debug_on_next_call_armed_fast());
    source.set(false);
    assert!(!obarray.debug_on_next_call_armed_fast());
}

#[test]
fn canonical_pair_debugger_discovery_is_read_only_until_publication() {
    crate::test_utils::init_test_tracing();
    let mut obarray = Obarray::new();
    let id = intern("debug-on-next-call");
    let before = obarray.admission_debugger_for_test();
    let interrupted = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        assert!(obarray.discover_debug_on_next_call_descriptor(id).is_none());
        assert_eq!(obarray.admission_debugger_for_test(), before);
        panic!("permitted panic after pure discovery, before publication");
    }));
    assert!(interrupted.is_err());
    assert_eq!(obarray.admission_debugger_for_test(), before);
    assert!(obarray.debug_on_next_call_armed_fast());

    assert!(
        neovm_value::test_support::authority::debug_on_next_call_bool_fwd_for_test(
            &mut obarray,
            id
        )
        .is_none()
    );
    assert!(!obarray.debug_on_next_call_armed_fast());
    assert_ne!(obarray.admission_debugger_for_test(), before);
}
