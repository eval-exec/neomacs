use super::*;
use crate::emacs_core::value::LambdaParams;

fn function(ops: Vec<Op>, constants: Vec<Value>, arity: usize) -> ByteCodeFunction {
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: (0..arity).map(|i| SymId(i as u32 + 1)).collect(),
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = ops;
    f.constants = constants.into();
    f.max_stack = 16;
    f.seal_hand_assembled_ops();
    f
}

fn install(ev: &mut Context, name: &str, f: ByteCodeFunction) -> Value {
    let sym = Value::symbol(name);
    ev.obarray
        .set_symbol_function_id(sym.as_symbol_id().unwrap(), Value::make_bytecode(f));
    sym
}

#[test]
fn mir_call_retains_native_callee_slots_and_revalidates_redefinition() {
    force_profit_gate_for_test(false);
    let mut ev = Context::new();
    // A multi-block callee is not eligible for the small MIR inliner.
    let sym = install(
        &mut ev,
        "mir-call-step",
        function(
            vec![
                Op::StackRef(0),
                Op::GotoIfNil(5),
                Op::StackRef(0),
                Op::Sub1,
                Op::Return,
                Op::Constant(0),
                Op::Return,
            ],
            vec![Value::make_int(0)],
            1,
        ),
    );
    let f = function(
        vec![Op::Constant(0), Op::StackRef(1), Op::Call(1), Op::Return],
        vec![sym],
        1,
    );
    let leaf = compile_bytecode_function_with(&f, Some(&ev.obarray)).unwrap();
    assert_eq!(leaf.tier, leaf::LeafTier::Mir);
    assert_eq!(leaf.spec_slots.len(), 1);
    assert!(leaf.inline_epoch().is_none());
    let ctx = &mut ev as *mut Context as *mut u8;
    let before = SPEC_SHIM_FAST_COUNT.load(Ordering::Relaxed);
    for _ in 0..6000 {
        assert_eq!(
            leaf.call(ctx, &[Value::make_int(5)]),
            NativeRun::Ok(Value::make_int(4).bits())
        );
    }
    if !jit_force_slow_spec() {
        assert!(SPEC_SHIM_FAST_COUNT.load(Ordering::Relaxed) > before);
        assert!(!leaf.spec_slots[0].leaf_ptr().is_null());
    }
    ev.eval_str("(fset 'mir-call-step (lambda (n) (+ n 20)))")
        .unwrap();
    assert_eq!(
        leaf.call(ctx, &[Value::make_int(5)]),
        NativeRun::Ok(Value::make_int(25).bits())
    );
    assert!(leaf.spec_slots[0].leaf_ptr().is_null());
    // A changed arity must produce the same signal payload as the interpreter.
    ev.eval_str("(fset 'mir-call-step (lambda () 7))").unwrap();
    assert_eq!(leaf.call(ctx, &[Value::make_int(5)]), NativeRun::Signal);
    let actual = take_pending_flow().unwrap();
    let expected = Vm::from_context(&mut ev)
        .execute(&f, vec![Value::make_int(5)])
        .unwrap_err();
    let payload = |flow| match flow {
        Flow::Signal(s) => (
            s.symbol,
            s.data
                .iter()
                .map(crate::emacs_core::print::print_value)
                .collect::<Vec<_>>(),
        ),
        other => panic!("expected a signal: {other:?}"),
    };
    assert_eq!(payload(actual), payload(expected));
}

#[test]
fn mir_call_predicate_and_bytecode_fallbacks_preserve_live_heap_roots() {
    force_profit_gate_for_test(false);
    let mut ev = Context::new();
    ev.eval_str("(fset 'mir-call-recordp (symbol-function 'recordp))")
        .unwrap();
    let collect = install(
        &mut ev,
        "mir-call-collect",
        function(
            vec![
                Op::CallBuiltinSym(intern("garbage-collect"), 0),
                Op::Pop,
                Op::Constant(0),
                Op::Nil,
                Op::CallBuiltinSym(intern("make-list"), 2),
                Op::Pop,
                Op::Nil,
                Op::Return,
            ],
            vec![Value::make_int(4096)],
            0,
        ),
    );
    let f = function(
        vec![
            Op::Constant(0),
            Op::Constant(1),
            Op::Cons,
            Op::Constant(2),
            Op::Constant(0),
            Op::Call(1),
            Op::Pop,
            Op::Constant(3),
            Op::Call(0),
            Op::Pop,
            Op::Return,
        ],
        vec![
            Value::make_int(7),
            Value::make_int(9),
            Value::symbol("mir-call-recordp"),
            collect,
        ],
        0,
    );
    let leaf = compile_bytecode_function_with(&f, Some(&ev.obarray)).unwrap();
    assert_eq!(leaf.tier, leaf::LeafTier::Mir);
    assert_eq!(leaf.spec_slots.len(), 2);
    for redefine in [false, true] {
        if redefine {
            ev.eval_str(
                "(fset 'mir-call-recordp (lambda (_) (garbage-collect) (make-list 4096 0) t))",
            )
            .unwrap();
        }
        for _ in 0..3 {
            let NativeRun::Ok(bits) = leaf.call(&mut ev as *mut Context as *mut u8, &[]) else {
                panic!("call must return the live pair")
            };
            let pair = Value::from_bits(bits);
            assert!(pair.is_cons());
            assert_eq!(pair.cons_car(), Value::make_int(7));
            assert_eq!(pair.cons_cdr(), Value::make_int(9));
            assert_eq!(ev.jit_root_stack_top, 0);
        }
    }
}

#[test]
fn mir_call_loop_observes_subr_redefinition_from_a_gc_hook() {
    let mut ev = Context::new();
    ev.eval_str(
        "(insert \"abc\") (goto-char 2)
        (fset 'mir-call-point (symbol-function 'point))",
    )
    .unwrap();
    let f = function(
        vec![
            Op::Constant(0),
            Op::StackRef(1),
            Op::Constant(0),
            Op::Gtr,
            Op::GotoIfNil(12),
            Op::Constant(1),
            Op::Call(0),
            Op::StackSet(1),
            Op::StackRef(1),
            Op::Sub1,
            Op::StackSet(2),
            Op::Goto(1),
            Op::Return,
        ],
        vec![Value::make_int(0), Value::symbol("mir-call-point")],
        1,
    );
    let leaf = compile_bytecode_function_with(&f, Some(&ev.obarray)).unwrap();
    assert_eq!(leaf.tier, leaf::LeafTier::Mir);
    assert_eq!(leaf.spec_slots.len(), 1);
    assert_eq!(
        leaf.call(&mut ev as *mut Context as *mut u8, &[Value::make_int(10)]),
        NativeRun::Ok(Value::make_int(2).bits())
    );
    ev.eval_str("(setq post-gc-hook (list (lambda () (fset 'mir-call-point (lambda () 9)))))")
        .unwrap();
    ev.gc_stress = true;
    assert_eq!(
        leaf.call(&mut ev as *mut Context as *mut u8, &[Value::make_int(600)]),
        NativeRun::Ok(Value::make_int(9).bits())
    );
    ev.gc_stress = false;
    assert_eq!(ev.jit_root_stack_top, 0);
}

#[test]
fn mir_call_precise_exit_does_not_repeat_the_callee_side_effect() {
    let mut ev = Context::new();
    ev.eval_str("(setq mir-call-v 0 mir-call-count 0)
        (add-variable-watcher 'mir-call-v (lambda (&rest _) (setq mir-call-count (1+ mir-call-count))))").unwrap();
    let callee = install(
        &mut ev,
        "mir-call-effect",
        function(
            vec![Op::Constant(0), Op::VarSet(1), Op::Constant(2), Op::Return],
            vec![
                Value::make_int(7),
                Value::symbol("mir-call-v"),
                Value::make_float(1.5),
            ],
            0,
        ),
    );
    let f = function(
        vec![Op::Constant(0), Op::Call(0), Op::Add1, Op::Return],
        vec![callee],
        0,
    );
    let leaf = compile_bytecode_function_with(&f, Some(&ev.obarray)).unwrap();
    assert_eq!(leaf.tier, leaf::LeafTier::Mir);
    assert_eq!(leaf.spec_slots.len(), 1);
    let result = leaf.call(&mut ev as *mut Context as *mut u8, &[]);
    let NativeRun::DeoptAt(ref frame) = result else {
        panic!("{result:?}")
    };
    assert_eq!(frame.pc, 2);
    assert_eq!(frame.stack.len(), 1);
    assert_eq!(frame.stack[0].as_float(), Some(1.5));
    assert_eq!(
        mir_inline_guards::resume(&mut ev, &f, result).as_float(),
        Some(2.5)
    );
    assert_eq!(ev.eval_str("mir-call-count").unwrap(), Value::make_int(1));
    assert_eq!(ev.jit_root_stack_top, 0);
}

#[test]
fn mir_call_observability_matches_the_interpreter() {
    force_profit_gate_for_test(false);
    for variable in [
        "internal--compiler-function-overrides",
        "debug-on-next-call",
        "quit-flag",
        "max-lisp-eval-depth",
    ] {
        let mut observations = Vec::new();
        for native in [false, true] {
            let mut ev = Context::new();
            ev.eval_str("(insert \"abc\") (goto-char 2)
                (fset 'mir-call-state-point (symbol-function 'point))
                (setq mir-call-debug-count 0
                      debugger (lambda (&rest _) (setq mir-call-debug-count (1+ mir-call-debug-count))))").unwrap();
            let value = match variable {
                "internal--compiler-function-overrides" => ev
                    .eval_str("'((mir-call-state-point . (lambda () 99)))")
                    .unwrap(),
                "max-lisp-eval-depth" => Value::make_int(0),
                _ => Value::T,
            };
            let f = function(
                vec![
                    Op::Constant(0),
                    Op::VarSet(1),
                    Op::Constant(2),
                    Op::Call(0),
                    Op::Return,
                ],
                vec![
                    value,
                    Value::symbol(variable),
                    Value::symbol("mir-call-state-point"),
                ],
                0,
            );
            let leaf = compile_bytecode_function_with(&f, Some(&ev.obarray)).unwrap();
            assert_eq!(leaf.tier, leaf::LeafTier::Mir);
            assert_eq!(leaf.spec_slots.len(), 1);
            if variable == "max-lisp-eval-depth" {
                ev.depth = 100;
            }
            let outcome = if native {
                match leaf.call(&mut ev as *mut Context as *mut u8, &[]) {
                    NativeRun::Ok(bits) => Ok(Value::from_bits(bits)),
                    NativeRun::Signal => Err(take_pending_flow().unwrap()),
                    other => panic!("{variable}: {other:?}"),
                }
            } else {
                Vm::from_context(&mut ev).execute(&f, vec![])
            };
            let outcome = match outcome {
                Ok(v) => format!("ok:{}", crate::emacs_core::print::print_value(&v)),
                Err(Flow::Signal(s)) => format!(
                    "{}:{:?}",
                    s.symbol_name(),
                    s.data
                        .iter()
                        .map(crate::emacs_core::print::print_value)
                        .collect::<Vec<_>>()
                ),
                Err(other) => panic!("{variable}: {other:?}"),
            };
            let debug_calls = ev
                .obarray
                .symbol_value("mir-call-debug-count")
                .unwrap()
                .as_fixnum()
                .unwrap();
            observations.push((outcome, debug_calls));
            assert_eq!(ev.jit_root_stack_top, 0);
        }
        assert_eq!(observations[0], observations[1], "{variable}");
    }
}
