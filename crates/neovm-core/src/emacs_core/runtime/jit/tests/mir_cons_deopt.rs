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

#[test]
fn mir_cons_precise_frames_preserve_aliases_and_completed_effects() {
    let mut ev = Context::new();
    ev.eval_str("(setq mir-rebuild-v 0 mir-rebuild-count 0) (add-variable-watcher 'mir-rebuild-v (lambda (&rest _) (setq mir-rebuild-count (1+ mir-rebuild-count))))").unwrap();
    let f = function(
        vec![
            Op::Constant(0),
            Op::VarSet(1),
            Op::StackRef(1),
            Op::Add1,
            Op::StackRef(1),
            Op::Cons,
            Op::Dup,
            Op::StackRef(0),
            Op::Car,
            Op::StackRef(3),
            Op::Cons,
            Op::StackRef(3),
            Op::Add1,
            Op::Pop,
            Op::Car,
            Op::Pop,
            Op::Car,
            Op::Pop,
            Op::Car,
            Op::Return,
        ],
        vec![Value::make_int(7), Value::symbol("mir-rebuild-v")],
        2,
    );
    let m = mir::build_mir(&f.ops, &f.constants, 2).unwrap();
    let plan = plan_mir_leaf(&m);
    assert!(plan.precise);
    assert_eq!(plan.cons_repl.iter().filter(|c| c.is_some()).count(), 2);
    let leaf = lower_mir_pure(&m).unwrap();
    let float = ev.eval_str("1.5").unwrap();
    let result = leaf.call(
        &mut ev as *mut Context as *mut u8,
        &[Value::make_int(7), float],
    );
    let NativeRun::DeoptAt(resume) = result else {
        panic!("expected precise float deopt: {result:?}")
    };
    assert_eq!(resume.pc, 12);
    assert_eq!(resume.stack.len(), 6);
    let pair = resume.stack[2];
    assert!(pair.is_cons());
    assert_eq!(
        pair.bits(),
        resume.stack[3].bits(),
        "aliases must share one reconstructed cons"
    );
    assert_eq!(pair.cons_car(), Value::make_int(8), "retag the raw car");
    assert_eq!(pair.cons_cdr(), float, "preserve the heap-valued cdr");
    let other = resume.stack[4];
    assert_ne!(
        pair.bits(),
        other.bits(),
        "distinct virtual conses stay distinct"
    );
    assert_eq!(other.cons_car(), pair.cons_car());
    assert_eq!(other.cons_cdr(), pair.cons_cdr());
    let DeoptResume {
        pc,
        stack,
        handlers,
        binds,
        spec_base,
        cond_base,
    } = *resume;
    let value = Vm::from_context(&mut ev)
        .run_resumed_frame(
            &f,
            Value::NIL,
            pc,
            &stack,
            handlers,
            &binds,
            spec_base,
            cond_base,
        )
        .expect("resume the sealed bytecode frame");
    assert_eq!(value, Value::make_int(8));
    assert_eq!(
        ev.eval_str("mir-rebuild-count").unwrap(),
        Value::make_int(1)
    );
    assert_eq!(ev.jit_root_stack_top, 0);
}

#[test]
fn mir_cons_block_local_loop_allocation_count() {
    let mut ev = Context::new();
    let f = function(
        vec![
            Op::Constant(0),
            Op::StackRef(1),
            Op::Constant(0),
            Op::Gtr,
            Op::GotoIfNil(16),
            Op::StackRef(0),
            Op::StackRef(2),
            Op::Nil,
            Op::Cons,
            Op::Car,
            Op::Add,
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
    // Fewer than 255 back edges avoids a service poll, so the allocation
    // counter directly observes this loop's conses without GC bookkeeping.
    let before = ev.tagged_heap.allocated_count;
    let result = leaf.call(&mut ev as *mut Context as *mut u8, &[Value::make_int(100)]);
    assert_eq!(result, NativeRun::Ok(Value::make_int(5050).bits()));
    assert_eq!(ev.tagged_heap.allocated_count, before);
}

#[test]
fn mir_singleton_precise_frames_preserve_aliases_and_completed_effects() {
    let mut ev = Context::new();
    ev.eval_str("(setq mir-list-v 0 mir-list-count 0) (add-variable-watcher 'mir-list-v (lambda (&rest _) (setq mir-list-count (1+ mir-list-count))))").unwrap();
    let f = function(
        vec![
            Op::Constant(0),
            Op::VarSet(1),
            Op::StackRef(1),
            Op::Add1,
            Op::List(1),
            Op::Dup,
            Op::StackRef(0),
            Op::Car,
            Op::List(1),
            Op::StackRef(3),
            Op::Add1,
            Op::Pop,
            Op::Car,
            Op::Pop,
            Op::Car,
            Op::Pop,
            Op::Car,
            Op::Return,
        ],
        vec![Value::make_int(7), Value::symbol("mir-list-v")],
        2,
    );
    let m = mir::build_mir(&f.ops, &f.constants, 2).unwrap();
    let leaf = lower_mir_pure(&m).unwrap();
    let float = ev.eval_str("1.5").unwrap();
    let result = leaf.call(
        &mut ev as *mut Context as *mut u8,
        &[Value::make_int(7), float],
    );
    let NativeRun::DeoptAt(ref resume) = result else {
        panic!("expected precise float deopt: {result:?}")
    };
    assert_eq!(resume.pc, 10);
    assert_eq!(resume.stack.len(), 6);
    let pair = resume.stack[2];
    assert!(pair.is_cons());
    assert_eq!(pair.bits(), resume.stack[3].bits(), "preserve aliases");
    let other = resume.stack[4];
    assert_ne!(pair.bits(), other.bits(), "preserve distinct objects");
    for list in [pair, other] {
        assert_eq!(list.cons_car(), Value::make_int(8));
        assert_eq!(list.cons_cdr(), Value::NIL);
    }
    assert_eq!(resume.stack[5], float);
    assert_eq!(
        mir_inline_guards::resume(&mut ev, &f, result),
        Value::make_int(8)
    );
    assert_eq!(ev.eval_str("mir-list-count").unwrap(), Value::make_int(1));
    assert_eq!(ev.jit_root_stack_top, 0);
}

#[test]
fn mir_singleton_escaping_lists_preserve_freshness_elements_and_mutation() {
    let mut ev = Context::new();
    let element = ev.eval_str("(list \"payload\")").unwrap();
    let f = function(vec![Op::StackRef(0), Op::List(1), Op::Return], vec![], 1);
    let m = mir::build_mir(&f.ops, &f.constants, 1).unwrap();
    let leaf = lower_mir_pure(&m).unwrap();
    let ctx = &mut ev as *mut Context as *mut u8;
    let mut lists = Vec::new();
    for _ in 0..2 {
        let NativeRun::Ok(bits) = leaf.call(ctx, &[element]) else {
            panic!("singleton construction must run natively")
        };
        let list = Value::from_bits(bits);
        assert!(list.is_cons());
        assert_eq!(list.cons_car().bits(), element.bits());
        assert!(list.cons_cdr().is_nil());
        lists.push(list);
    }
    assert_ne!(lists[0].bits(), lists[1].bits());

    // Keep one alias while setcar mutates the other, then return the first.
    let f = function(
        vec![
            Op::StackRef(1),
            Op::List(1),
            Op::Dup,
            Op::StackRef(2),
            Op::Setcar,
            Op::Pop,
            Op::Return,
        ],
        vec![],
        2,
    );
    let m = mir::build_mir(&f.ops, &f.constants, 2).unwrap();
    let leaf = lower_mir_pure(&m).unwrap();
    let NativeRun::Ok(bits) = leaf.call(ctx, &[Value::NIL, element]) else {
        panic!("mutating an escaping singleton must run natively")
    };
    let list = Value::from_bits(bits);
    assert_eq!(list.cons_car().bits(), element.bits());
    assert!(list.cons_cdr().is_nil());
}

#[test]
fn mir_empty_list_preserves_the_residual_stack() {
    let mut ev = Context::new();
    let element = ev.eval_str("\"payload\"").unwrap();
    for (ops, expected) in [
        (vec![Op::List(0), Op::Return], Value::NIL),
        (vec![Op::List(0), Op::Pop, Op::Return], element),
    ] {
        let m = mir::build_mir(&ops, &[], 1).unwrap();
        let leaf = lower_mir_pure(&m).unwrap();
        assert_eq!(
            leaf.call(&mut ev as *mut Context as *mut u8, &[element]),
            NativeRun::Ok(expected.bits())
        );
    }
}
