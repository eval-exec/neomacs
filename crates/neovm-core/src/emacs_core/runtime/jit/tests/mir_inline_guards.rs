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

pub(super) fn resume(ev: &mut Context, f: &ByteCodeFunction, result: NativeRun) -> Value {
    let NativeRun::DeoptAt(frame) = result else {
        panic!("expected a precise call deopt: {result:?}");
    };
    let DeoptResume {
        pc,
        stack,
        handlers,
        binds,
        spec_base,
        cond_base,
    } = *frame;
    Vm::from_context(ev)
        .run_resumed_frame(
            f,
            Value::NIL,
            pc,
            &stack,
            handlers,
            &binds,
            spec_base,
            cond_base,
        )
        .expect("resume original call")
}

#[test]
fn mir_inline_identity_rechecks_after_a_variable_watcher() {
    let mut ev = Context::new();
    ev.eval_str(
        "(setq mir-inline-v 0 mir-inline-effects 0)
        (add-variable-watcher 'mir-inline-v (lambda (&rest _)
          (setq mir-inline-effects (1+ mir-inline-effects))
          (fset 'mir-inline-id (lambda (x) (+ x 10)))))",
    )
    .unwrap();
    let id = Value::symbol("mir-inline-id");
    let callee = function(vec![Op::StackRef(0), Op::Return], vec![], 1);
    ev.obarray
        .set_symbol_function_id(id.as_symbol_id().unwrap(), Value::make_bytecode(callee));
    let f = function(
        vec![
            Op::Constant(0),
            Op::VarSet(1),
            Op::Constant(2),
            Op::StackRef(1),
            Op::Call(1),
            Op::Return,
        ],
        vec![Value::make_int(1), Value::symbol("mir-inline-v"), id],
        1,
    );
    let leaf = compile_bytecode_function_with(&f, Some(&ev.obarray)).unwrap();
    assert_eq!(leaf.tier, leaf::LeafTier::Mir);
    let _ = ev.debug_on_next_call_is_armed();
    let result = leaf.call(&mut ev as *mut Context as *mut u8, &[Value::make_int(5)]);
    let NativeRun::DeoptAt(frame) = &result else {
        panic!("{result:?}")
    };
    assert_eq!(frame.pc, 4);
    assert_eq!(
        frame.stack,
        vec![Value::make_int(5), id, Value::make_int(5)]
    );
    assert_eq!(resume(&mut ev, &f, result), Value::make_int(15));
    assert_eq!(
        ev.eval_str("mir-inline-effects").unwrap(),
        Value::make_int(1)
    );
    assert_eq!(ev.jit_root_stack_top, 0);
}

#[test]
fn mir_inline_identity_honors_debug_override_quit_and_depth() {
    for variable in [
        "debug-on-next-call",
        "internal--compiler-function-overrides",
        "quit-flag",
        "max-lisp-eval-depth",
    ] {
        let mut ev = Context::new();
        let id = Value::symbol("mir-inline-flags-id");
        let callee = function(vec![Op::StackRef(0), Op::Return], vec![], 1);
        ev.obarray
            .set_symbol_function_id(id.as_symbol_id().unwrap(), Value::make_bytecode(callee));
        let value = match variable {
            "max-lisp-eval-depth" => Value::make_int(0),
            "internal--compiler-function-overrides" => ev
                .eval_str("'((mir-inline-flags-id . (lambda (x) 99)))")
                .unwrap(),
            _ => Value::T,
        };
        let f = function(
            vec![
                Op::Constant(0),
                Op::VarSet(1),
                Op::Constant(2),
                Op::StackRef(1),
                Op::Call(1),
                Op::Return,
            ],
            vec![value, Value::symbol(variable), id],
            1,
        );
        let leaf = compile_bytecode_function_with(&f, Some(&ev.obarray)).unwrap();
        assert_eq!(leaf.tier, leaf::LeafTier::Mir);
        if variable == "max-lisp-eval-depth" {
            ev.depth = 100;
        }
        let epoch = ev.obarray.function_epoch();
        let _ = ev.debug_on_next_call_is_armed();
        let result = leaf.call(&mut ev as *mut Context as *mut u8, &[Value::make_int(5)]);
        let NativeRun::DeoptAt(frame) = result else {
            panic!("{variable}: {result:?}")
        };
        assert_eq!(frame.pc, 4, "{variable}");
        assert_eq!(
            frame.stack,
            vec![Value::make_int(5), id, Value::make_int(5)]
        );
        if variable != "internal--compiler-function-overrides" {
            assert_eq!(
                ev.obarray.function_epoch(),
                epoch,
                "state changes without redefinition"
            );
        }
        assert_eq!(ev.jit_root_stack_top, 0);
    }
}

#[test]
fn mir_inline_guard_reconstructs_a_virtual_argument() {
    let mut ev = Context::new();
    ev.eval_str(
        "(setq mir-inline-pair-v 0 mir-inline-pair-effects 0)
        (add-variable-watcher 'mir-inline-pair-v (lambda (&rest _)
          (setq mir-inline-pair-effects (1+ mir-inline-pair-effects))
          (fset 'mir-inline-car (lambda (x) (cdr x)))))",
    )
    .unwrap();
    let sym = Value::symbol("mir-inline-car");
    let callee = function(vec![Op::StackRef(0), Op::Car, Op::Return], vec![], 1);
    ev.obarray
        .set_symbol_function_id(sym.as_symbol_id().unwrap(), Value::make_bytecode(callee));
    let f = function(
        vec![
            Op::Constant(0),
            Op::VarSet(1),
            Op::Constant(2),
            Op::StackRef(1),
            Op::Add1,
            Op::Constant(3),
            Op::Cons,
            Op::Call(1),
            Op::Return,
        ],
        vec![
            Value::make_int(1),
            Value::symbol("mir-inline-pair-v"),
            sym,
            Value::make_int(10),
        ],
        1,
    );
    let leaf = compile_bytecode_function_with(&f, Some(&ev.obarray)).unwrap();
    assert_eq!(leaf.tier, leaf::LeafTier::Mir);
    let _ = ev.debug_on_next_call_is_armed();
    let result = leaf.call(&mut ev as *mut Context as *mut u8, &[Value::make_int(5)]);
    let NativeRun::DeoptAt(frame) = &result else {
        panic!("{result:?}")
    };
    assert_eq!(frame.pc, 7);
    assert_eq!(frame.stack.len(), 3);
    assert_eq!(frame.stack[2].cons_car(), Value::make_int(6));
    assert_eq!(frame.stack[2].cons_cdr(), Value::make_int(10));
    assert_eq!(resume(&mut ev, &f, result), Value::make_int(10));
    assert_eq!(
        ev.eval_str("mir-inline-pair-effects").unwrap(),
        Value::make_int(1)
    );
    assert_eq!(ev.jit_root_stack_top, 0);
}

#[test]
fn mir_inline_keeps_a_callees_non_fixnum_feedback_on_the_native_call_path() {
    use crate::emacs_core::jit::NumericFeedback;
    for (feedback, literal) in [
        (NumericFeedback::Float, false),
        (NumericFeedback::Other, false),
        (NumericFeedback::FixnumOnly, true),
    ] {
        let mut ev = Context::new();
        let sym = Value::symbol("mir-inline-float-step");
        let callee = function(
            vec![
                Op::StackRef(0),
                if literal {
                    Op::Constant(0)
                } else {
                    Op::StackRef(0)
                },
                Op::Add,
                Op::Return,
            ],
            vec![Value::make_float(0.5)],
            1,
        );
        callee
            .jit_runtime()
            .record_numeric(2, callee.ops.len(), feedback);
        ev.obarray
            .set_symbol_function_id(sym.as_symbol_id().unwrap(), Value::make_bytecode(callee));
        let f = function(
            vec![
                Op::StackRef(1),
                Op::Constant(0),
                Op::Gtr,
                Op::GotoIfNil(12),
                Op::Constant(1),
                Op::StackRef(1),
                Op::Call(1),
                Op::StackSet(1),
                Op::StackRef(1),
                Op::Sub1,
                Op::StackSet(2),
                Op::Goto(0),
                Op::Return,
            ],
            vec![Value::make_int(0), sym],
            2,
        );
        let leaf = compile_bytecode_function_with(&f, Some(&ev.obarray)).unwrap();
        assert_eq!(leaf.tier, leaf::LeafTier::Mir, "{feedback:?}");
        assert_eq!(leaf.spec_slots.len(), 1, "retain the ordinary native call");
        assert!(leaf.inline_epoch().is_none());
        let result = leaf.call(
            &mut ev as *mut Context as *mut u8,
            &[Value::make_int(5), Value::make_float(0.5)],
        );
        let NativeRun::Ok(bits) = result else {
            panic!("{feedback:?}: {result:?}")
        };
        assert_eq!(
            Value::from_bits(bits).as_float(),
            Some(if literal { 3.0 } else { 16.0 })
        );
        assert_eq!(ev.jit_root_stack_top, 0);
    }
}
