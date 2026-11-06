//! The obarray-only symbol-value string claim, split out of value's
//! tagged/gc/tests/ownership_test.rs by the GC module-cluster ruling: this is
//! the file's one test that constructs a Context (value has no unit runtime,
//! so every value BOOT test re-homes to core). The other 56 tests in the file
//! are CONTEXT-FREE (their chain is init_test_tracing -> ... ->
//! with_test_writer, tracing's fmt builder, not an era fixture) and stay.

use super::*;

#[test]
fn concurrent_claim_reaches_obarray_symbol_value_strings() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::new_context();
    set_tagged_heap(unsafe { ev.tagged_heap_mut() });

    // Interval-free string reachable ONLY via the symbol value cell.
    let s = unsafe { ev.tagged_heap_mut() }
        .alloc_string(crate::heap_types::LispString::from_utf8("obarray-only"));
    ev.obarray_mut()
        .set_symbol_value("neovm--str-claim-probe", s);

    // Admit and begin before staging the obarray snapshot, as the actual
    // handshake does. No direct root seed may replace this obarray-only sink.
    // SAFETY: this Context is the heap/obarray's sole writer; there is no
    // callback, safepoint or ownership handoff during start and publication.
    // SAFETY: this Context is the heap/obarray's sole writer; there is no
    // callback, safepoint or ownership handoff during start and publication.
    // The fence returns DISJOINT borrows of the two fields, so the obarray may
    // be read while the heap view is live (no raw pointer, no aliasing).
    let (obarray, heap) =
        unsafe { neovm_eval::test_support::authority::obarray_and_tagged_heap_mut(&mut ev) };
    // SAFETY: the same admitted owner starts the handshake below.
    let permit = unsafe { heap.permit_concurrent_mark() }.unwrap();
    let mut capture = permit.begin();
    let snap = {
        // SAFETY: the same admitted owner captures its matching obarray
        // before worker publication; no other writer or callback can enter.
        let world = unsafe { scan_contract::SingleMutatorWorld::from_heap(capture.heap_mut()) };
        obarray.scan_snapshot(&world)
    };
    capture.heap_mut().set_pending_obarray_scan(snap);
    capture.launch().unwrap();
    while !ev.tagged_heap().concurrent_mark_done() {
        std::thread::yield_now();
    }
    unsafe { ev.tagged_heap_mut() }.join_concurrent_mark();

    let stats = ev.tagged_heap().sweep_stats();
    assert!(
        stats.last_concurrent_str_claimed >= 1,
        "the obarray-value string must be claimed via the symbol-cell scan \
         (claimed={})",
        stats.last_concurrent_str_claimed,
    );
    // The claimed string is black — at THIS cycle's parity (the raw bit
    // value alone is meaningless under parity marks).
    assert!(unsafe {
        (*(s.as_string_ptr().unwrap()))
            .header
            .is_marked_at(ev.tagged_heap().mark_parity_for_test())
    });
    // No sweep here: this bare-heap driver does not re-seed the Context
    // roots at termination, so sweeping would free live Context objects.
    // Claim + mark are the assertions under test (survival-under-sweep is
    // covered by the vector-sink test); the heap frees everything at drop.
}
