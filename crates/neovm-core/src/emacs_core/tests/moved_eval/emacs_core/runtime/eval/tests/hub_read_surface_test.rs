//! Cross-crate smoke test for the widening-hub read surface (#1871).
//!
//! The hub lands A-prime read views before the slice move commits that
//! consume them, so this file is the hub's own consumer: it exercises each
//! observed read on a canonical Context, proving the widened routes compile
//! and answer from the real runtime without exposing the crate-private
//! fields. Slice tests keep their own assertions; this only pins the shape.

use neovm_value::intern::intern;

#[test]
fn hub_context_read_surface_observes_a_canonical_context() {
    crate::test_utils::init_test_tracing();
    let mut ctx = crate::test_utils::new_context();

    // These four reads use the routes that were already public (peer #1903
    // dropped the hub getters as redundant): the tagged-heap accessor
    // auto-derefs to TaggedHeap, specpdl_len_for_test is the emptiness test,
    // and the obarray accessor exposes the function cell and the projection.
    let _ = ctx.tagged_heap().tagged_heap_identity();
    let _ = ctx.specpdl_len_for_test();
    assert!(ctx.obarray().symbol_function_id(intern("car")).is_some());
    let _ = ctx.obarray().thread_projection_of(intern("car"));

    // Projections exist only for variables the World projects; exercise the
    // owner-slot observed read when `car` is one.
    if let Some(projection) = ctx.obarray().thread_projection_of(intern("car")) {
        let _ = ctx.owner_slot_value_is_none_for_test(projection);
    }

    // Copied flags and stacks answer without handing out a field.
    let _ = ctx.gc_driver_active_for_test();
    let _ = ctx.jit_root_stack_for_test();
    let _ = ctx.eval_temp_roots_for_test();

    // The construction seam flips the one observation the moved tests need;
    // ThreadingMode itself stays private.
    ctx.set_green_threading_mode_at_construction_for_test();
    assert!(ctx.threading_mode_is_green_for_test());

    // The World-lifetime probe observes a live World without extending it.
    let probe = ctx.world_lifetime_probe_for_test();
    assert!(probe());

    // The parked-root group census answers with named groups.
    for name in ctx.private_root_group_names() {
        assert!(!name.is_empty());
    }
}
