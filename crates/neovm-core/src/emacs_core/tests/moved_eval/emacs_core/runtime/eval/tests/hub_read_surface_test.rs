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

    // The arithmetic kind table is nameable and iterates in its dense
    // discriminant order.
    let kinds: Vec<neovm_eval::emacs_core::bytecode::ArithGenericKind> =
        neovm_eval::emacs_core::bytecode::ArithGenericKind::iter().collect();
    assert!(!kinds.is_empty());
    assert_eq!(
        kinds[0],
        neovm_eval::emacs_core::bytecode::ArithGenericKind::Add
    );

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

    // A fresh serial process holds no Green native state, and evicting an id
    // that was never assigned is a no-op through the signed fence.
    assert!(!neovm_eval::emacs_core::jit::cache::green_has_native_state());
    neovm_eval::test_support::authority::evict_compiled_for_test(u64::MAX);
}

/// The one explicit green constructor (#1877(2)): a canonical Context whose
/// World threading mode is Green at construction, with ThreadingMode and
/// new_with_threading staying private.
#[cfg(feature = "jit")]
#[test]
fn hub_green_constructor_builds_a_green_context() {
    crate::test_utils::init_test_tracing();
    let ctx = crate::test_utils::new_green_context();
    assert!(ctx.threading_mode_is_green_for_test());
}

#[cfg(feature = "jit")]
#[test]
fn hub_token_surface_saves_and_restores_the_private_knobs() {
    use neovm_eval::test_support::authority::{
        TestFlonumMode, TestInline2Mode, TestProfitPolicy, force_flonum_mode_token_for_test,
        jit_inline2_mode_token_for_test, replace_profit_gate_token_for_test,
    };

    crate::test_utils::init_test_tracing();

    // The profit-gate token round-trips: the return value is the previous
    // policy, so a test can restore exactly what it found.
    let previous = replace_profit_gate_token_for_test(TestProfitPolicy::Bypass);
    assert_eq!(previous, TestProfitPolicy::Runtime);
    let previous = replace_profit_gate_token_for_test(TestProfitPolicy::Runtime);
    assert_eq!(previous, TestProfitPolicy::Bypass);

    // The inline-v2 read token names the private enum through its mirror.
    let mode = jit_inline2_mode_token_for_test();
    assert!(matches!(
        mode,
        TestInline2Mode::Off
            | TestInline2Mode::Named
            | TestInline2Mode::Closure
            | TestInline2Mode::Hof
            | TestInline2Mode::All
    ));

    // The flonum token writes each private mode and then restores the
    // environment's answer.
    force_flonum_mode_token_for_test(Some(TestFlonumMode::Resident));
    force_flonum_mode_token_for_test(Some(TestFlonumMode::Off));
    force_flonum_mode_token_for_test(None);

    // The handle-taking fences name their public shapes; constructing a VM
    // or a bytecode function is the slice's job, not the hub smoke test's.
    let _ = neovm_eval::test_support::authority::force_interpreter_only_for_test
        as fn(&mut neovm_eval::emacs_core::bytecode::Vm);
    let _ = neovm_eval::test_support::authority::publish_numeric_feedback_scope_for_test
        as fn(
            &neovm_eval::emacs_core::bytecode::ByteCodeFunction,
        ) -> neovm_eval::test_support::authority::NumericFeedbackScopeHandle;
    let _ = neovm_eval::test_support::authority::prepare_entry_for_test
        as fn(
            &mut neovm_eval::emacs_core::eval::Context,
            &neovm_eval::emacs_core::bytecode::ByteCodeFunction,
        );
}
