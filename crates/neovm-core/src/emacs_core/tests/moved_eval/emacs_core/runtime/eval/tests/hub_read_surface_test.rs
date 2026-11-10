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

#[cfg(feature = "jit")]
#[test]
fn hub_jit_read_surface_observes_the_canonical_knobs() {
    // The extension traits are imported through their post-split paths; `as _`
    // is the documented spelling (a caller never names the trait otherwise).
    use neovm_eval::emacs_core::bytecode::ByteCodeFunctionExt as _;
    use neovm_eval::emacs_core::jit::RuntimeStateTieringExt as _;

    crate::test_utils::init_test_tracing();
    let _ctx = crate::test_utils::new_context();

    // Plain boolean knob reads, direct and behind the private-module fence.
    let _ = neovm_eval::emacs_core::jit::jit_forced_off_for_test();
    let _ = neovm_eval::emacs_core::jit::inline::jit_inline_on();
    let _ = neovm_eval::test_support::authority::jit_inline_arith_on_for_test();

    // The background mode is a public plain enum, and the canonical default
    // is one of its variants.
    let mode = neovm_eval::test_support::authority::bg_mode_for_test();
    assert!(matches!(
        mode,
        neovm_eval::emacs_core::jit::BgMode::Legacy
            | neovm_eval::emacs_core::jit::BgMode::Sync
            | neovm_eval::emacs_core::jit::BgMode::Threaded
    ));

    // The leaf-slot epoch starts nonzero; inline-dependent counts are empty
    // on a fresh process.
    assert!(neovm_eval::emacs_core::jit::cache::leaf_slot_epoch() >= 1);
    assert_eq!(
        neovm_eval::emacs_core::jit::cache::inline_dependent_count_for_test(intern("car")),
        0
    );

    // The flonum census type and its observed reads are nameable, and a
    // fresh process starts at the default census.
    let census = neovm_eval::emacs_core::jit::compile::flonum_census();
    assert_eq!(
        census,
        neovm_eval::emacs_core::jit::compile::FlonumCensus::default()
    );
    assert_eq!(census.results(), 0);
    assert_eq!(census.escape_boxes(), 0);
    assert_eq!(census.cold_boxes(), 0);
}
