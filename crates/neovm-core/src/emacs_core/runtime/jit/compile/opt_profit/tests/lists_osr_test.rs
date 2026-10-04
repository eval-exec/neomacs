//! The real OSR cache keeps numeric baseline emission and selects list loops.
//! Threading: each test owns its context, source and leaves; no cache mutation
//! occurs while a leaf pointer is inspected or a native entry is running.

use super::frontend_tests::{Settings, count_loop};
use super::lists_frontend_tests::{list_loop, normalize};
use super::*;
use crate::emacs_core::jit::cache;
use crate::emacs_core::jit::compile::compile_pipeline_tests::captured_clif;
use crate::emacs_core::jit::compile::opt_census::SelectedTier;
use crate::emacs_core::jit::compile::{
    Context, NativeRun, OptAdmit, OptProfitMode, Value, force_opt_for_test,
    force_opt_profit_for_test,
};

#[test]
fn opt_profit_lists_numeric_osr_retains_legacy_baseline_clif_and_result() {
    let _settings = Settings::enter();
    let mut context = Context::new();
    force_opt_profit_for_test(Some(OptProfitMode::Lists));
    let snapshot = [Value::make_int(2000), Value::make_int(42)];
    let mut clif = Vec::new();
    for mode in [OptMode::Opt, OptMode::Legacy] {
        force_opt_for_test(Some(mode), Some(OptAdmit::ALL));
        let f = count_loop(0);
        context.bc_buf.clear();
        context.bc_buf.extend_from_slice(&snapshot);
        let emitted = captured_clif(|| {
            assert_eq!(
                cache::try_run_osr(&mut context, &f, 1, &snapshot, &[]),
                Some(NativeRun::Ok(Value::make_int(2000).bits()))
            );
        });
        let leaf = cache::osr_leaf_ptr_for_test(&f, 1).expect("installed numeric OSR leaf");
        assert_eq!(unsafe { &*leaf }.selected_tier(), SelectedTier::Baseline);
        assert!(!emitted.is_empty());
        clif.push(normalize(emitted));
        cache::clear();
    }
    assert_eq!(clif[0], clif[1]);
    assert_eq!(context.jit_root_stack_top, 0);
}

#[test]
fn opt_profit_lists_list_osr_preserves_snapshot_guards_result_and_mutation() {
    let _settings = Settings::enter();
    let mut context = Context::new();
    force_opt_profit_for_test(Some(OptProfitMode::Lists));
    let f = list_loop();
    let pair = Value::cons(Value::make_int(-1), Value::NIL);
    crate::emacs_core::eval::push_scratch_gc_root(pair);
    let snapshot = [pair, Value::make_int(4), Value::make_int(2)];
    context.bc_buf.extend_from_slice(&snapshot);
    assert_eq!(
        cache::try_run_osr(&mut context, &f, 1, &snapshot, &[]),
        Some(NativeRun::Ok(Value::make_int(4).bits()))
    );
    let leaf = cache::osr_leaf_ptr_for_test(&f, 1).expect("installed list OSR leaf");
    assert_eq!(unsafe { &*leaf }.selected_tier(), SelectedTier::Opt);
    assert_eq!(pair.cons_car(), Value::make_int(3));
    let bad = [pair, Value::make_int(4), Value::NIL];
    context.bc_buf.clear();
    context.bc_buf.extend_from_slice(&bad);
    let Some(NativeRun::DeoptAt(frame)) = cache::try_run_osr(&mut context, &f, 1, &bad, &[]) else {
        panic!("OSR must preserve the invalid counter snapshot");
    };
    assert_eq!(frame.pc, 1);
    assert_eq!(frame.stack.as_slice(), &bad);
    assert_eq!(context.jit_root_stack_top, 0);
    cache::clear();
}
