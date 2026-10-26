//! The grouped cells remain canonical through construction, writes and cold
//! replacement. Borrow-model controls live in tmp/codex/p0s1/model.
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
    assert!(std::ptr::eq(pair, &obarray.dispatch_cells));
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
    let before = obarray.dispatch_cells.function_epoch();
    obarray.set_symbol_function_id(id, Value::fixnum(41));
    let written = obarray.function_epoch();
    assert!(written > before);
    assert_eq!(obarray.dispatch_cells.function_epoch(), written);
    obarray.invalidate_all_function_bindings(FunctionEpochBump::InternalCellWrite);
    assert_eq!(
        obarray.dispatch_cells.function_epoch(),
        obarray.function_epoch()
    );
    let clone = obarray.clone();
    assert_eq!(
        clone.dispatch_cells.function_epoch(),
        obarray.function_epoch()
    );
    let restored =
        Obarray::from_dump(Vec::new(), Vec::new(), Vec::new(), 777).expect("empty dump restores");
    assert_eq!(restored.dispatch_cells.function_epoch(), 777);
    assert!(restored.dispatch_cells.debug_on_next_call_armed_fast());
}

#[test]
fn canonical_pair_pointer_from_allocation_root_survives_cold_replacement() {
    let storage = Box::new(UnsafeCell::new(Obarray::new()));
    let manager = storage.get();
    // SAFETY: the designator comes directly from the final UnsafeCell allocation
    // root, not a temporary field reference; storage owns its lifetime.
    let cells = unsafe { std::ptr::addr_of_mut!((*manager).dispatch_cells) };
    let cells = NonNull::new(cells).expect("live allocation subfield");
    // SAFETY: one owner, short exclusive phase, no canonical reader is live.
    unsafe { &mut *manager }.set_function_epoch_for_test(7);
    // SAFETY: the completed mutable phase precedes this canonical read.
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
    let mut obarray = Obarray::new();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        obarray.set_function_epoch_for_test(17);
        panic!("permitted canonical-writer panic");
    }));
    assert!(result.is_err());
    assert_eq!(obarray.dispatch_cells.function_epoch(), 17);
}

#[test]
fn canonical_pair_follows_live_boolean_and_clone_owns_its_descriptor() {
    let mut obarray = Obarray::new();
    let id = intern("debug-on-next-call");
    assert!(obarray.dispatch_cells.debug_on_next_call_armed_fast());
    assert!(obarray.debug_on_next_call_bool_fwd(id).is_none());
    assert!(!obarray.dispatch_cells.debug_on_next_call_armed_fast());
    obarray.define_bool_variable("debug-on-next-call", false, ByteBooleanVars::Listed);
    let source = obarray.bool_forwarder(id).expect("installed Boolean cell");
    source.set(true);
    assert!(obarray.dispatch_cells.debug_on_next_call_armed_fast());
    let mut cloned = obarray.clone();
    assert!(cloned.dispatch_cells.debug_on_next_call_armed_fast());
    let clone_cell = cloned
        .debug_on_next_call_bool_fwd(id)
        .expect("clone resolves its own forwarded cell");
    assert!(!std::ptr::eq(source, clone_cell));
    clone_cell.set(false);
    assert!(obarray.dispatch_cells.debug_on_next_call_armed_fast());
    assert!(!cloned.dispatch_cells.debug_on_next_call_armed_fast());
    source.set(false);
    assert!(!obarray.dispatch_cells.debug_on_next_call_armed_fast());
}

#[test]
fn canonical_pair_debugger_discovery_is_read_only_until_publication() {
    crate::test_utils::init_test_tracing();
    let mut obarray = Obarray::new();
    let id = intern("debug-on-next-call");
    let before = obarray.dispatch_cells.admission_debugger();
    let interrupted = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        assert!(obarray.discover_debug_on_next_call_descriptor(id).is_none());
        assert_eq!(obarray.dispatch_cells.admission_debugger(), before);
        panic!("permitted panic after pure discovery, before publication");
    }));
    assert!(interrupted.is_err());
    assert_eq!(obarray.dispatch_cells.admission_debugger(), before);
    assert!(obarray.debug_on_next_call_armed_fast());

    assert!(obarray.debug_on_next_call_bool_fwd(id).is_none());
    assert!(!obarray.debug_on_next_call_armed_fast());
    assert_ne!(obarray.dispatch_cells.admission_debugger(), before);
}
