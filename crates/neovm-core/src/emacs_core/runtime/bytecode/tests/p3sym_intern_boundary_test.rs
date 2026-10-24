//! Reuse real VM/named function caches after a relocated initial-obarray spine.
use super::*;

#[test]
fn p3sym_c5_vm_readers_and_function_caches_survive_intern_relocation() {
    let _stamps = Context::c5_function_stamps_for_test();
    let mut ctx = Context::new_minimal_vm_harness();
    ctx.set_variable("window-dead-windows-table", Value::NIL);
    ctx.gc_collect_exact();
    let global = intern("p3sym-c5-vm-old-value");
    let symbol = intern("p3sym-c5-vm-old-function");
    ctx.obarray.set_symbol_value_id(global, Value::fixnum(73));
    assert!(ctx.obarray.is_plain_value_cell_id(global));
    let old = constant(73);
    ctx.obarray.set_symbol_function_id(symbol, old);
    let mut reader = ByteCodeFunction::new(LambdaParams::simple(vec![]));
    reader.lexical = true;
    reader.ops = vec![Op::VarRef(0), Op::Return];
    reader.constants = vec![Value::from_sym_id(global)].into();
    reader.max_stack = crate::emacs_core::bytecode::StackDepth::for_test(4);
    let reader = Value::make_bytecode(reader);
    let mut caller = ByteCodeFunction::new(LambdaParams::simple(vec![]));
    caller.lexical = true;
    caller.ops = vec![Op::Constant(0), Op::Call(0), Op::Return];
    caller.constants = vec![Value::from_sym_id(symbol)].into();
    caller.max_stack = crate::emacs_core::bytecode::StackDepth::for_test(4);
    let caller = Value::make_bytecode(caller);
    let _roots = ctx.retain_gc_roots([old, reader, caller]);
    let expected: Vec<_> = include_str!(
        "../../../../tagged/gc/mutator_registration/symbol_intern_cache_oracle.expected"
    )
    .lines()
    .collect();
    assert_eq!(
        format!(
            "({} {})",
            printed(call(&mut ctx, reader)),
            printed(call(&mut ctx, caller))
        ),
        expected[0]
    );
    {
        let mut vm = Vm::from_context(&mut ctx);
        assert!(
            matches!(vm.resolve_stack_call_target(Value::from_sym_id(symbol)), ResolvedStackCallTarget::ByteCode { callee } if callee.value() == old)
        );
    }
    let clock = ctx.dispatch_projection().function_epoch();
    let named = ctx.c4_resolve_named_for_test(symbol);
    assert_eq!(named, (clock, Some(old.bits())));
    assert!(ctx.obarray.fn_unchanged_since(symbol, clock));
    #[cfg(feature = "jit")]
    crate::emacs_core::jit::test_support::settle(&mut ctx);
    let holders = ctx.c5_prepare_intern_growth_for_test("p3sym-c5-vm-growth");
    assert!(
        matches!(ctx.symbol_bytecode_call_cache.get(symbol, clock), Some(ResolvedStackCallTarget::ByteCode { callee }) if callee.value() == old)
    );
    assert_eq!(ctx.c4_named_cache_for_test(symbol), Some(named));
    ctx.c5_intern_growth_era_for_test(holders);

    // Inspect installed entries BEFORE any serial resolver can refill them.
    coherent(&ctx, clock);
    assert!(ctx.obarray.fn_unchanged_since(symbol, clock));
    assert!(
        matches!(ctx.symbol_bytecode_call_cache.get(symbol, clock), Some(ResolvedStackCallTarget::ByteCode { callee }) if callee.value() == old)
    );
    assert_eq!(ctx.c4_named_cache_for_test(symbol), Some(named));
    assert_eq!(
        format!(
            "({} {})",
            printed(call(&mut ctx, reader)),
            printed(call(&mut ctx, caller))
        ),
        expected[1]
    );
    // A plain VM VarRef has no persistent variable-cache entry. Reusing the
    // SAME bytecode must still read a subsequent serial value-cell write.
    ctx.obarray.set_symbol_value_id(global, Value::fixnum(97));
    assert_eq!(
        format!(
            "({} {})",
            printed(call(&mut ctx, reader)),
            printed(call(&mut ctx, caller))
        ),
        expected[2]
    );
    coherent(&ctx, clock);
    eprintln!(
        "P3SYM_C5_VM_GROWTH relocated=1 vm_cache_preserved=1 named_cache_preserved=1 function_clock_delta=0 stamps_unchanged=1 rows=3"
    );
}
