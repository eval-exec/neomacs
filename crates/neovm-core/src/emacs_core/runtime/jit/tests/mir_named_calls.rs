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
    f
}

#[test]
fn mir_named_reads_follow_buffer_state_and_ignore_function_redefinition() {
    let mut ev = Context::new();
    let point = intern("point");
    let f = function(
        vec![Op::CallBuiltinSym(point, 0), Op::Add1, Op::Return],
        vec![],
        0,
    );
    let m = mir::build_mir(&f.ops, &f.constants, None, 0).unwrap();
    let leaf = lower_mir_pure(&m).unwrap();
    ev.eval_str("(insert \"abc\") (goto-char 2)").unwrap();
    let ctx = &mut ev as *mut Context as *mut u8;
    assert_eq!(
        leaf.call(ctx, &[]),
        NativeRun::Ok(Value::make_int(3).bits())
    );
    ev.eval_str("(fset 'point (lambda () 99)) (goto-char 4)")
        .unwrap();
    assert_eq!(
        leaf.call(ctx, &[]),
        NativeRun::Ok(Value::make_int(5).bits())
    );
    ev.eval_str("(narrow-to-region 2 3) (goto-char 2)").unwrap();
    assert_eq!(
        leaf.call(ctx, &[]),
        NativeRun::Ok(Value::make_int(3).bits())
    );
}

#[test]
fn mir_named_calls_signal_with_the_same_payload_as_the_interpreter() {
    use crate::emacs_core::print::print_value;
    for (name, args) in [
        ("match-end", vec![Value::make_int(-1)]),
        ("match-beginning", vec![Value::NIL]),
        ("car", vec![Value::make_int(7)]),
    ] {
        let mut ev = Context::new();
        let mut ops: Vec<_> = (0..args.len()).map(|i| Op::Constant(i as u16)).collect();
        ops.extend([
            Op::CallBuiltinSym(intern(name), args.len() as u8),
            Op::Return,
        ]);
        let f = function(ops, args, 0);
        let m = mir::build_mir(&f.ops, &f.constants, None, 0).unwrap();
        let leaf = lower_mir_pure(&m).unwrap();
        assert_eq!(
            leaf.call(&mut ev as *mut Context as *mut u8, &[]),
            NativeRun::Signal
        );
        let actual = take_pending_flow().unwrap();
        let expected = Vm::from_context(&mut ev).execute(&f, vec![]).unwrap_err();
        let payload = |flow| match flow {
            Flow::Signal(s) => (s.symbol, s.data.iter().map(print_value).collect::<Vec<_>>()),
            other => panic!("expected a signal: {other:?}"),
        };
        assert_eq!(payload(actual), payload(expected), "{name}");
    }
}

#[test]
fn mir_named_read_loop_observes_gc_hook_changes() {
    let mut ev = Context::new();
    ev.eval_str(
        "(insert \"abcd\") (goto-char 1) (setq post-gc-hook (list (lambda () (goto-char 4))))",
    )
    .unwrap();
    // Count down n, returning the last observed point. The read must remain
    // inside the loop: a service poll can move point through post-gc-hook.
    let f = function(
        vec![
            Op::Constant(0),
            Op::StackRef(1),
            Op::Constant(0),
            Op::Gtr,
            Op::GotoIfNil(11),
            Op::CallBuiltinSym(intern("point"), 0),
            Op::StackSet(1),
            Op::StackRef(1),
            Op::Sub1,
            Op::StackSet(2),
            Op::Goto(1),
            Op::Return,
        ],
        vec![Value::make_int(0)],
        1,
    );
    let leaf = compile_bytecode_function_with(&f, Some(&ev.obarray)).unwrap();
    assert_eq!(leaf.tier, leaf::LeafTier::Mir);
    ev.gc_stress = true;
    let result = leaf.call(&mut ev as *mut Context as *mut u8, &[Value::make_int(600)]);
    ev.gc_stress = false;
    assert_eq!(result, NativeRun::Ok(Value::make_int(4).bits()));
    assert_eq!(ev.jit_root_stack_top, 0);
}

#[test]
fn named_call_effects_keep_fast_reads_separate_from_fallbacks() {
    use super::calls::Effects;
    let _ev = Context::new();
    let read = named_builtin_call(&Op::CallBuiltinSym(intern("point"), 0)).unwrap();
    assert!(read.fast_effects.contains(Effects::READ_BUFFER));
    assert!(read.fast_effects.is_read_only());
    assert!(!read.fast_effects.contains(Effects::MAY_GC));
    assert!(read.effects.contains(Effects::MAY_GC));
    assert!(read.effects.contains(Effects::MAY_REENTER));
    assert!(read.effects.contains(Effects::WRITE_BUFFER));
    assert!(!read.effects.is_read_only());
    let mutation = named_builtin_call(&Op::CallBuiltinSym(intern("insert"), 1)).unwrap();
    assert!(!mutation.fast_effects.is_read_only());
    assert!(mutation.effects.contains(Effects::WRITE_BUFFER));
    assert!(named_builtin_call(&Op::CallBuiltinSym(intern("eval"), 1)).is_none());

    let m = mir::build_mir(
        &[
            Op::StackRef(0),
            Op::Car,
            Op::Constant(0),
            Op::Add,
            Op::Return,
        ],
        &[Value::make_int(1)],
        None,
        1,
    )
    .unwrap();
    let car = m.blocks[0]
        .insts
        .iter()
        .find(|i| matches!(i.op, mir::MirOp::CarCdr { .. }))
        .unwrap();
    assert!(car.effect.contains(Effects::READ_HEAP));
    assert!(car.effect.contains(Effects::MAY_DEOPT));
    let add = m.blocks[0]
        .insts
        .iter()
        .find(|i| matches!(i.op, mir::MirOp::Bin(..)))
        .unwrap();
    assert!(add.effect.contains(Effects::MAY_DEOPT));
}
