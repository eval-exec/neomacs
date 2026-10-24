//! Installed native leaves, never prepared again after actual C5 relocation.
use super::compile::VARREF_SHIM_CALLS;
use super::*;

fn run_installed(ctx: &mut Context, source: Value) -> usize {
    let f = source.get_bytecode_data().expect("retained native source");
    // Direct observation: no pending-result preparation or compilation helper.
    match cache::try_run_compiled(ctx, f, source, &[]) {
        Ok(Some(bits)) => bits,
        result => panic!("existing installed C5 native entry required: {result:?}"),
    }
}
fn identity(source: Value) -> u64 {
    source
        .get_bytecode_data()
        .unwrap()
        .jit_runtime()
        .compiled_id_or_assign()
}
fn leaf(id: u64) -> Option<usize> {
    cache::compiled_leaf_ptr_for_test(id).map(|pointer| pointer as usize)
}

#[test]
fn p3sym_c5_jit_plain_reader_and_caller_survive_intern_relocation() {
    let _profit = Profit::bypass();
    let _backend = compile::opt_mode_scope_for_test(compile::OptMode::Off);
    let _stamps = Context::c5_function_stamps_for_test();
    let mut ctx = Context::new_minimal_vm_harness();
    ctx.set_variable("window-dead-windows-table", Value::NIL);
    ctx.gc_collect_exact();
    let global = intern("p3sym-c5-native-old-value");
    let symbol = intern("p3sym-c5-native-old-function");
    ctx.obarray.set_symbol_value_id(global, Value::fixnum(73));
    assert!(ctx.obarray.is_plain_value_cell_id(global));
    let old = function(vec![Op::Constant(0), Op::Return], vec![Value::fixnum(73)]);
    ctx.obarray.set_symbol_function_id(symbol, old);
    let reader = function(
        vec![Op::VarRef(0), Op::Return],
        vec![Value::from_sym_id(global)],
    );
    let caller = function(
        vec![Op::Constant(0), Op::Call(0), Op::Return],
        vec![Value::from_sym_id(symbol)],
    );
    let _roots = ctx.retain_gc_roots([old, reader, caller]);
    let expected: Vec<_> = include_str!(
        "../../../../../tagged/gc/mutator_registration/symbol_intern_cache_oracle.expected"
    )
    .lines()
    .collect();
    assert_eq!(
        format!(
            "({} {})",
            printed(native(&mut ctx, reader).unwrap()),
            printed(native(&mut ctx, caller).unwrap())
        ),
        expected[0]
    );
    test_support::settle(&mut ctx);
    let ids = [identity(reader), identity(caller)];
    let leaves = ids.map(leaf);
    assert!(leaves.iter().all(Option::is_some));
    let clock = ctx.dispatch_projection().function_epoch();
    assert!(ctx.obarray.fn_unchanged_since(symbol, clock));
    let holders = ctx.c5_prepare_intern_growth_for_test("p3sym-c5-native-growth");
    assert_eq!(ids.map(leaf), leaves);
    ctx.c5_intern_growth_era_for_test(holders);

    // Cache identity and clocks are checked BEFORE entry, with no prepare.
    coherent(&ctx, clock);
    assert_eq!(ids.map(leaf), leaves);
    assert!(ctx.obarray.fn_unchanged_since(symbol, clock));
    VARREF_SHIM_CALLS.with(|calls| calls.set(0));
    assert_eq!(
        format!(
            "({} {})",
            printed(run_installed(&mut ctx, reader)),
            printed(run_installed(&mut ctx, caller))
        ),
        expected[1]
    );
    assert_eq!(VARREF_SHIM_CALLS.with(|calls| calls.get()), 0);
    ctx.obarray.set_symbol_value_id(global, Value::fixnum(97));
    VARREF_SHIM_CALLS.with(|calls| calls.set(0));
    assert_eq!(
        format!(
            "({} {})",
            printed(run_installed(&mut ctx, reader)),
            printed(run_installed(&mut ctx, caller))
        ),
        expected[2]
    );
    assert_eq!(VARREF_SHIM_CALLS.with(|calls| calls.get()), 0);
    assert_eq!(ids.map(leaf), leaves);
    coherent(&ctx, clock);
    eprintln!(
        "P3SYM_C5_NATIVE_GROWTH relocated=1 leaves_preserved=2 function_clock_delta=0 stamps_unchanged=1 varref_shims=0 rows=3"
    );
}

#[test]
fn p3sym_c5_jit_inline_dependent_survives_intern_relocation() {
    if !super::super::inline::jit_inline_on() {
        eprintln!("P3SYM_C5_INLINE_GROWTH_INAPPLICABLE inline=off");
        return;
    }
    let _profit = Profit::bypass();
    let _backend = compile::opt_mode_scope_for_test(compile::OptMode::Legacy);
    let _stamps = Context::c5_function_stamps_for_test();
    let (mut ctx, symbols, targets, callers, replacement) = fixture("p3sym-c5-inline-growth");
    let _roots = ctx.retain_gc_roots(targets.into_iter().chain(callers).chain([replacement]));
    native(&mut ctx, callers[0]).unwrap();
    test_support::settle(&mut ctx);
    let id = identity(callers[0]);
    let installed = leaf(id).expect("actual installed inline caller");
    assert_eq!(cache::inline_dependent_count_for_test(symbols[0]), 1);
    let clock = ctx.dispatch_projection().function_epoch();
    assert!(ctx.obarray.fn_unchanged_since(symbols[0], clock));
    let holders = ctx.c5_prepare_intern_growth_for_test("p3sym-c5-inline-growth");
    ctx.c5_intern_growth_era_for_test(holders);

    coherent(&ctx, clock);
    assert_eq!(leaf(id), Some(installed));
    assert_eq!(cache::inline_dependent_count_for_test(symbols[0]), 1);
    assert!(ctx.obarray.fn_unchanged_since(symbols[0], clock));
    let expected: Vec<_> = include_str!(
        "../../../../../tagged/gc/mutator_registration/symbol_intern_cache_oracle.expected"
    )
    .lines()
    .collect();
    assert_eq!(
        format!(
            "({} {})",
            printed(run_installed(&mut ctx, callers[0])),
            direct(&mut ctx, Value::from_sym_id(symbols[0]))
        ),
        expected[1]
    );
    assert_eq!(leaf(id), Some(installed));
    coherent(&ctx, clock);
    eprintln!(
        "P3SYM_C5_INLINE_GROWTH relocated=1 leaf_preserved=1 dependents_preserved=1 function_clock_delta=0 stamps_unchanged=1"
    );
}
