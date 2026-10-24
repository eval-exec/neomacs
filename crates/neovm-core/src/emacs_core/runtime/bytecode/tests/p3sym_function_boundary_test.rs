//! Actual VM/named cache hits retire at the owned-era admission boundary.
use super::*;
use crate::emacs_core::eval::Context;
use crate::tagged::gc::function_boundary_testkit::{FunctionEraEdit, FunctionEraReceipt};
use crate::tagged::transport::SharedRoot;

#[path = "p3sym_intern_boundary_test.rs"]
mod intern_growth;

fn constant(answer: i64) -> Value {
    let mut f = ByteCodeFunction::new(LambdaParams::simple(vec![]));
    f.lexical = true;
    f.ops = vec![Op::Constant(0), Op::Return];
    f.constants = vec![Value::fixnum(answer)].into();
    f.max_stack = super::super::function_slots::StackDepth::for_test(4);
    Value::make_bytecode(f)
}
fn fixture() -> (Context, [SymId; 2], Value, Value, Value) {
    let mut ctx = Context::new_minimal_vm_harness();
    // The reduced constructor leaves unreachable default weak-table residue.
    // Drain it before creating any test definition/root, retaining the strict
    // production admission fence. This native cache fixture is not a Miri row.
    ctx.set_variable("window-dead-windows-table", Value::NIL);
    ctx.gc_collect_exact();
    let ids = [
        intern("p3sym-c4-vm-cache-leaf"),
        intern("p3sym-c4-vm-cache-other"),
    ];
    let old = constant(73);
    ctx.obarray.set_symbol_function_id(ids[0], old);
    ctx.obarray.set_symbol_function_id(ids[1], constant(73));
    let new = Value::list(vec![Value::symbol("lambda"), Value::NIL, Value::fixnum(97)]);
    let mut caller = ByteCodeFunction::new(LambdaParams::simple(vec![]));
    caller.lexical = true;
    caller.ops = vec![Op::Constant(0), Op::Call(0), Op::Return];
    caller.constants = vec![Value::from_sym_id(ids[0])].into();
    caller.max_stack = super::super::function_slots::StackDepth::for_test(4);
    (ctx, ids, old, new, Value::make_bytecode(caller))
}
fn call(ctx: &mut Context, caller: Value) -> EvalResult {
    let f = caller.get_bytecode_data().expect("rooted serial caller");
    let mut vm = Vm::from_context(ctx);
    #[cfg(feature = "jit")]
    vm.force_interpreter_only_for_test();
    vm.execute(f, vec![])
}
fn printed(result: EvalResult) -> String {
    crate::emacs_core::print_value(&result.expect("serial constant call"))
}
fn root(ctx: &mut Context, value: Value) -> SharedRoot {
    // SAFETY: the sole serial Context owns this actual heap, before admission.
    unsafe { SharedRoot::new(&mut ctx.tagged_heap, value) }
}
fn oracle() -> Vec<&'static str> {
    let rows: Vec<_> = include_str!(
        "../../../../tagged/gc/mutator_registration/symbol_function_cache_oracle.expected"
    )
    .lines()
    .collect();
    assert_eq!(rows.len(), 6, "GNU31.1 compiled-call rows");
    rows
}
fn assert_boundary(receipt: &FunctionEraReceipt, changed: Option<usize>) {
    let clock = receipt.before.facts[0].0;
    assert_eq!(receipt.before.facts[1].0, clock);
    assert_eq!(receipt.writer.words, receipt.after.words);
    assert_eq!(receipt.writer.facts, receipt.after.facts);
    for i in 0..2 {
        let after = receipt.after.facts[i];
        assert_eq!(after.0, clock + u64::from(changed.is_some()));
        assert_eq!(
            after.1,
            if changed == Some(i) {
                after.0
            } else {
                receipt.before.facts[i].1
            }
        );
        assert_eq!(after.2, changed.is_some());
    }
}
fn coherent(ctx: &Context, expected: u64) {
    assert_eq!(
        ctx.obarray().function_epoch(),
        expected,
        "canonical original clock"
    );
    assert_eq!(
        ctx.dispatch_projection().function_epoch(),
        expected,
        "first returning admission clock"
    );
    assert!(ctx.obarray.admission_matches_canonical());
}

#[test]
fn p3sym_c4_vm_and_named_cache_across_owned_era() {
    let (mut ctx, ids, old, new, caller) = fixture();
    let _roots = ctx.retain_gc_roots([old, new, caller]);
    let expected = oracle();
    let initial = ctx.dispatch_projection().function_epoch();
    let warm = printed(call(&mut ctx, caller));
    assert_eq!(
        format!(
            "({warm} {})",
            printed(ctx.apply0(Value::from_sym_id(ids[0])))
        ),
        expected[0]
    );
    // Warm the real VM classifier and named resolver, independently. No
    // synthetic cache insertion can substitute for either engagement check.
    {
        let mut vm = Vm::from_context(&mut ctx);
        assert!(
            matches!(vm.resolve_stack_call_target(Value::from_sym_id(ids[0])), ResolvedStackCallTarget::ByteCode { callee } if callee.value() == old)
        );
    }
    assert!(
        ctx.symbol_bytecode_call_cache
            .get(ids[0], initial)
            .is_some()
    );
    assert_eq!(
        ctx.c4_resolve_named_for_test(ids[0]),
        (initial, Some(old.bits()))
    );
    assert_eq!(
        ctx.c4_resolve_named_for_test(ids[0]),
        (initial, Some(old.bits()))
    );

    for (edit, changed, row) in [
        (FunctionEraEdit::Same { symbol: 0 }, None, 1),
        (FunctionEraEdit::Replace { symbol: 1 }, Some(1), 2),
        (FunctionEraEdit::Replace { symbol: 0 }, Some(0), 3),
        (FunctionEraEdit::Unbind { symbol: 0 }, Some(0), 4),
        (FunctionEraEdit::Replace { symbol: 0 }, Some(0), 5),
    ] {
        let previous = ctx.dispatch_projection().function_epoch();
        let named = ctx
            .c4_named_cache_for_test(ids[0])
            .expect("warmed actual named cache");
        let had_vm_hit = ctx
            .symbol_bytecode_call_cache
            .get(ids[0], previous)
            .is_some();
        let input = root(&mut ctx, new);
        let receipt = ctx.c4_function_era_for_test(ids, edit, input, |_| {});
        assert_boundary(&receipt, changed);
        assert_eq!(
            receipt.writer_answer,
            match row {
                1 => old.bits(),
                4 => Value::from_sym_id(ids[0]).bits(),
                _ => new.bits(),
            },
            "actual rooted writer answer row{row}"
        );
        let clock = previous + u64::from(changed.is_some());
        coherent(&ctx, clock);
        // Admission retirement updates the header, not the saved cache tag.
        // The old actual named entry remains old until the resolver retries.
        assert_eq!(ctx.c4_named_cache_for_test(ids[0]), Some(named));
        if changed.is_some() {
            assert!(
                ctx.symbol_bytecode_call_cache.get(ids[0], clock).is_none(),
                "old VM target was not relabeled with a new clock"
            );
            if had_vm_hit {
                assert!(
                    ctx.symbol_bytecode_call_cache
                        .get(ids[0], previous)
                        .is_some()
                );
            }
        } else {
            assert!(had_vm_hit && ctx.symbol_bytecode_call_cache.get(ids[0], clock).is_some());
        }
        let target = if row < 3 {
            Some(old.bits())
        } else if row == 4 {
            None
        } else {
            Some(new.bits())
        };
        assert_eq!(ctx.c4_resolve_named_for_test(ids[0]), (clock, target));
        let first = call(&mut ctx, caller);
        let actual = match row {
            1 => format!(
                "({} {} {})",
                crate::emacs_core::print_value(&Value::bool_val(
                    receipt.writer_answer == old.bits()
                )),
                printed(first),
                printed(ctx.apply0(Value::from_sym_id(ids[0])))
            ),
            2 => format!(
                "({} {})",
                printed(first),
                printed(ctx.apply0(Value::from_sym_id(ids[1])))
            ),
            3 => format!(
                "({} {} {})",
                printed(ctx.apply0(old)),
                printed(first),
                printed(ctx.apply0(Value::from_sym_id(ids[0])))
            ),
            4 => {
                let condition = match first.kinded_ref() {
                    Err(FlowRef::Signal(data)) => {
                        crate::emacs_core::intern::resolve_sym(data.symbol).to_string()
                    }
                    other => panic!("expected GNU error condition: {other:?}"),
                };
                format!("({condition} {})", printed(ctx.apply0(old)))
            }
            5 => format!(
                "({} {})",
                printed(first),
                printed(ctx.apply0(Value::from_sym_id(ids[0])))
            ),
            _ => unreachable!("fixture row"),
        };
        assert_eq!(
            actual, expected[row],
            "first serial call after era row{row}"
        );
        eprintln!("P3SYM_C4_SERIAL_CACHE_ROW row={row} clock={clock} actual={actual}");
    }
}

#[test]
fn p3sym_c4_cache_boundary_observer_failure_has_total_teardown() {
    let (mut ctx, ids, old, new, caller) = fixture();
    let _roots = ctx.retain_gc_roots([old, new, caller]);
    let before = ctx.dispatch_projection().function_epoch();
    let input = root(&mut ctx, new);
    let callbacks = std::cell::Cell::new(0);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        ctx.c4_function_era_for_test(ids, FunctionEraEdit::Same { symbol: 0 }, input, |_| {
            callbacks.set(callbacks.get() + 1);
            panic!("intentional cache metadata assertion");
        });
    }));
    assert_eq!(
        callbacks.get(),
        2,
        "control reached both real stopped observer points"
    );
    let error = result.expect_err("control reached its metadata assertion");
    let message = error
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| error.downcast_ref::<&str>().copied())
        .unwrap_or("non-text assertion");
    assert!(
        message.contains("cache metadata assertion failed after clean reconciliation"),
        "wrong control assertion: {message}"
    );
    coherent(&ctx, before);
    assert_eq!(
        format!(
            "({} {})",
            printed(call(&mut ctx, caller)),
            printed(ctx.apply0(Value::from_sym_id(ids[0])))
        ),
        oracle()[0]
    );
    eprintln!("P3SYM_C4_CACHE_BOUNDARY_FAILURE_CLEANUP_OK");
}
