use super::epoch::{EpochCounters, reset_epoch_counters_for_test};
use crate::emacs_core::eval::Context;
use crate::emacs_core::intern::intern;
use crate::emacs_core::symbol::FunctionEpochBump;
use crate::emacs_core::value::Value;

/// The counters since `base`, and the epoch distance they must explain.
fn delta(base: &EpochCounters) -> EpochCounters {
    EpochCounters::snapshot().since(base)
}

/// Each Lisp-level writer is attributed to its own reason, and the per-reason
/// counts account for every move of `function_epoch`.
#[test]
fn jit_fn_epoch_bumps_are_attributed_per_writer() {
    reset_epoch_counters_for_test();
    let mut ev = Context::new();
    let epoch0 = ev.obarray.function_epoch();
    let base = EpochCounters::snapshot();
    let sym = Value::symbol("jit-epoch-test-fn");
    let def_a = Value::make_int(1);
    let def_b = Value::make_int(2);

    crate::emacs_core::builtins::builtin_fset_2(&mut ev, sym, def_a).expect("fset");
    assert_eq!(delta(&base).bumps_for(FunctionEpochBump::Fset), 1);

    crate::emacs_core::builtins::misc_eval::builtin_defalias(&mut ev, vec![sym, def_b])
        .expect("defalias");
    assert_eq!(delta(&base).bumps_for(FunctionEpochBump::Defalias), 1);

    ev.defalias_value(sym, def_a).expect("defalias_value");
    assert_eq!(delta(&base).bumps_for(FunctionEpochBump::Defalias), 2);

    crate::emacs_core::builtins::builtin_fmakunbound(&mut ev, vec![sym]).expect("fmakunbound");
    assert_eq!(delta(&base).bumps_for(FunctionEpochBump::Fmakunbound), 1);

    // A cell write to the value already there bumps nothing.
    crate::emacs_core::builtins::builtin_fset_2(&mut ev, sym, def_a).expect("fset");
    let before_same = ev.obarray.function_epoch();
    let unchanged_before = delta(&base).unchanged_writes;
    crate::emacs_core::builtins::builtin_fset_2(&mut ev, sym, def_a).expect("same fset");
    assert_eq!(ev.obarray.function_epoch(), before_same, "no epoch move");
    assert_eq!(delta(&base).unchanged_writes, unchanged_before + 1);
    assert_eq!(delta(&base).bumps_for(FunctionEpochBump::Fset), 2);

    let id = sym.as_symbol_id().expect("symbol");
    ev.obarray.clear_function_silent_id(id);
    assert_eq!(delta(&base).bumps_for(FunctionEpochBump::SilentClear), 1);

    let gone = intern("jit-epoch-test-unintern");
    ev.obarray.set_symbol_function_id(gone, Value::make_int(3));
    assert_eq!(
        delta(&base).bumps_for(FunctionEpochBump::InternalCellWrite),
        1,
        "the unattributed setter is an internal cell write"
    );
    assert!(ev.obarray.unintern_id(gone), "was a member");
    assert_eq!(delta(&base).bumps_for(FunctionEpochBump::Unintern), 1);

    let overrides = intern(crate::emacs_core::eval::INTERNAL_COMPILER_FUNCTION_OVERRIDES);
    ev.sync_cached_runtime_binding_by_id(overrides, Value::cons(Value::NIL, Value::NIL));
    assert_eq!(
        delta(&base).bumps_for(FunctionEpochBump::CompilerOverrides),
        1
    );

    let subr_base = delta(&base).bumps_for(FunctionEpochBump::SubrRewrite);
    crate::emacs_core::eval::register_public_subrs(&mut ev);
    assert!(
        delta(&base).bumps_for(FunctionEpochBump::SubrRewrite) > subr_base,
        "install_subr bumps without a cell write"
    );

    let d = delta(&base);
    assert_eq!(
        ev.obarray.function_epoch() - epoch0,
        d.total(),
        "every epoch move is attributed exactly once: {}",
        d.render()
    );
    let rendered = d.render();
    assert!(rendered.starts_with(&format!("total={} fset=2 defalias=2 ", d.total())));
    assert!(rendered.contains(" unintern=1 "), "{rendered}");
    assert!(rendered.contains(" compiler-overrides=1 "), "{rendered}");
    assert!(d.unchanged_writes >= 1, "{rendered}");
}

/// The attribution does not change the epoch arithmetic: `u64::MAX` (the
/// spec DISARMED sentinel) is still skipped on the wrap.
#[test]
fn jit_fn_epoch_bump_still_skips_the_disarmed_sentinel() {
    let mut ev = Context::new();
    ev.obarray.set_function_epoch_for_test(u64::MAX - 1);
    ev.obarray
        .bump_function_epoch(FunctionEpochBump::SubrRewrite);
    assert_eq!(ev.obarray.function_epoch(), 0);
    ev.obarray.set_function_epoch_for_test(u64::MAX - 1);
    crate::emacs_core::builtins::builtin_fset_2(
        &mut ev,
        Value::symbol("jit-epoch-test-wrap"),
        Value::make_int(9),
    )
    .expect("fset");
    assert_eq!(ev.obarray.function_epoch(), 0);
}

/// Under the stats knob the most-redefined symbols are named in the report.
#[test]
fn jit_fn_epoch_top_redefined_names_symbols() {
    let mut ev = Context::new();
    reset_epoch_counters_for_test();
    super::force_observe_for_test(super::ObserveOverride {
        stats: true,
        ..Default::default()
    });
    let hot = Value::symbol("jit-epoch-test-hot");
    let cold = Value::symbol("jit-epoch-test-cold");
    for i in 0..3 {
        crate::emacs_core::builtins::builtin_fset_2(&mut ev, hot, Value::make_int(i))
            .expect("fset");
    }
    crate::emacs_core::builtins::builtin_fset_2(&mut ev, cold, Value::make_int(0)).expect("fset");
    assert_eq!(
        super::epoch::top_redefined(2),
        "jit-epoch-test-hot=3,jit-epoch-test-cold=1"
    );
}
