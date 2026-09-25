//! Unboxed float slots ("flonums", `lowering::SlotRep::Flonum`): a
//! `Float`-feedback arithmetic result stays a raw `f64` in the baseline
//! model stack and is boxed only where it escapes. Every test records the
//! feedback before the first compile, drives the leaf with a real `Context`
//! and asserts the flonum census, so none can pass on the boxed lowering.
//!
//! GNU makes exactly one float object per arithmetic result
//! (`float_arith_driver` ends in `make_float`): every copy of one result
//! must stay `eq`, and two results must be distinct objects.

use super::*;
use crate::emacs_core::bytecode::Vm;
use crate::emacs_core::eval::Context;
use crate::emacs_core::jit::NumericFeedback as NF;
use crate::emacs_core::value::{LambdaParams, ValueKind};

/// `(lambda (p0 .. pN-1) OPS)` with `Float` recorded at `float_pcs`.
fn float_fn(
    arity: usize,
    ops: Vec<Op>,
    constants: Vec<Value>,
    float_pcs: &[usize],
) -> ByteCodeFunction {
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: (0..arity)
            .map(|i| crate::emacs_core::intern::SymId(i as u32 + 1))
            .collect(),
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = ops;
    f.constants = constants.into();
    f.max_stack = 16;
    f.seal_hand_assembled_ops();
    for &pc in float_pcs {
        f.jit_runtime().record_numeric(pc, f.ops.len(), NF::Float);
        assert_eq!(f.jit_runtime().numeric_feedback(pc), NF::Float);
    }
    f
}

/// Compile `f` under `mode` and return the leaf with its flonum census.
fn compile_in(mode: FlonumMode, f: &ByteCodeFunction) -> (CompiledLeaf, lowering::FlonumCensus) {
    force_flonum_mode_for_test(Some(mode));
    let leaf = compile_bytecode_function(f).expect("compiles");
    let census = lowering::flonum_census();
    force_flonum_mode_for_test(None);
    assert_eq!(
        leaf.tier(),
        leaf::LeafTier::Baseline,
        "a Float-site body is the baseline's"
    );
    (leaf, census)
}

/// Run `leaf` natively and count the objects it allocated.
fn run_counting(ev: &mut Context, leaf: &CompiledLeaf, args: &[Value]) -> (NativeRun, usize) {
    let before = ev.tagged_heap.allocated_count();
    let run = leaf.call(ev as *mut Context as *mut u8, args);
    (run, ev.tagged_heap.allocated_count() - before)
}

fn ok_value(run: NativeRun) -> Value {
    match run {
        NativeRun::Ok(bits) => Value::from_bits(bits),
        other => panic!("expected a native result, got {other:?}"),
    }
}

fn float_of(v: Value) -> f64 {
    assert!(
        matches!(v.kind(), ValueKind::Float),
        "expected a float, got {v:?}"
    );
    v.xfloat()
}

/// `(lambda (a b c) (+ (* a b) c))`: `*` at pc 2, `+` at pc 4.
fn mul_add() -> ByteCodeFunction {
    float_fn(
        3,
        vec![
            Op::StackRef(2),
            Op::StackRef(2),
            Op::Mul,
            Op::StackRef(1),
            Op::Add,
            Op::Return,
        ],
        vec![],
        &[2, 4],
    )
}

#[test]
fn a_float_chain_boxes_only_its_escaping_result() {
    let mut ev = Context::new();
    let (leaf, census) = compile_in(FlonumMode::OpLocal, &mul_add());
    assert_eq!(census.results, 2, "both sites leave their result unboxed");
    assert_eq!(census.escape_boxes, 1, "only the returned sum escapes");
    let args = [
        Value::make_float(1.5),
        Value::make_float(2.0),
        Value::make_float(0.25),
    ];
    for _ in 0..3 {
        let (run, allocated) = run_counting(&mut ev, &leaf, &args);
        assert_eq!(float_of(ok_value(run)), 3.25);
        assert_eq!(allocated, 1, "the product is never boxed");
    }
    // Mixed operands promote natively, still one box.
    let (run, allocated) = run_counting(
        &mut ev,
        &leaf,
        &[
            Value::make_int(3),
            Value::make_float(0.5),
            Value::make_int(1),
        ],
    );
    assert_eq!(float_of(ok_value(run)), 2.5);
    assert_eq!(allocated, 1);
}

#[test]
fn fixnum_pairs_through_a_float_chain_stay_fixnums() {
    let mut ev = Context::new();
    let (leaf, census) = compile_in(FlonumMode::OpLocal, &mul_add());
    assert_eq!(census.results, 2);
    let (run, allocated) = run_counting(
        &mut ev,
        &leaf,
        &[Value::make_int(2), Value::make_int(3), Value::make_int(4)],
    );
    assert_eq!(
        run,
        NativeRun::Ok(Value::make_int(10).bits()),
        "(+ (* 2 3) 4) is the fixnum 10, natively"
    );
    assert_eq!(allocated, 0, "a fixnum result is its own tag word");
    // A fixnum product plus a float promotes the product.
    let (run, allocated) = run_counting(
        &mut ev,
        &leaf,
        &[
            Value::make_int(2),
            Value::make_int(3),
            Value::make_float(0.5),
        ],
    );
    assert_eq!(float_of(ok_value(run)), 6.5);
    assert_eq!(allocated, 1);
}

#[test]
fn aliases_of_one_result_are_one_object() {
    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context as *mut u8;

    // (lambda (v a b) (let ((x (* a b)))
    //   (aset v 0 x) (aset v 1 x) (eq (aref v 0) (aref v 1))))
    let stored_twice = float_fn(
        3,
        vec![
            Op::StackRef(1),
            Op::StackRef(1),
            Op::Mul,
            Op::StackRef(3),
            Op::Constant(0),
            Op::StackRef(2),
            Op::Aset,
            Op::Pop,
            Op::StackRef(3),
            Op::Constant(1),
            Op::StackRef(2),
            Op::Aset,
            Op::Pop,
            Op::StackRef(3),
            Op::Constant(0),
            Op::Aref,
            Op::StackRef(4),
            Op::Constant(1),
            Op::Aref,
            Op::Eq,
            Op::Return,
        ],
        vec![Value::make_int(0), Value::make_int(1)],
        &[2],
    );
    let (leaf, census) = compile_in(FlonumMode::OpLocal, &stored_twice);
    assert_eq!(census.results, 1);
    assert_eq!(
        census.escape_boxes, 1,
        "one box for the result and its copies"
    );
    let v = Value::vector(vec![Value::NIL, Value::NIL]);
    let (run, allocated) = run_counting(
        &mut ev,
        &leaf,
        &[v, Value::make_float(1.5), Value::make_float(2.0)],
    );
    assert_eq!(
        run,
        NativeRun::Ok(Value::T.bits()),
        "(eq (aref v 0) (aref v 1))"
    );
    assert_eq!(allocated, 1);
    let slots = v.as_vector_data().expect("vector");
    assert_eq!(slots[0].bits(), slots[1].bits(), "one object in both slots");
    assert_eq!(float_of(slots[0]), 3.0);

    // (lambda (a b) (let ((x (* a b))) (eq x x)))
    let eq_self = float_fn(
        2,
        vec![
            Op::StackRef(1),
            Op::StackRef(1),
            Op::Mul,
            Op::Dup,
            Op::Eq,
            Op::Return,
        ],
        vec![],
        &[2],
    );
    let (leaf, census) = compile_in(FlonumMode::OpLocal, &eq_self);
    assert_eq!((census.results, census.escape_boxes), (1, 1));
    assert_eq!(
        leaf.call(ctx, &[Value::make_float(1.5), Value::make_float(2.0)]),
        NativeRun::Ok(Value::T.bits())
    );

    // (lambda (v a b) (let ((x (* a b))) (eq (aset v 0 x) x)))
    let aset_value = float_fn(
        3,
        vec![
            Op::StackRef(1),
            Op::StackRef(1),
            Op::Mul,
            Op::StackRef(3),
            Op::Constant(0),
            Op::StackRef(2),
            Op::Aset,
            Op::StackRef(1),
            Op::Eq,
            Op::Return,
        ],
        vec![Value::make_int(0)],
        &[2],
    );
    let (leaf, census) = compile_in(FlonumMode::OpLocal, &aset_value);
    assert_eq!((census.results, census.escape_boxes), (1, 1));
    let v = Value::vector(vec![Value::NIL]);
    assert_eq!(
        leaf.call(ctx, &[v, Value::make_float(1.5), Value::make_float(2.0)]),
        NativeRun::Ok(Value::T.bits()),
        "aset returns the very object it stored"
    );
}

#[test]
fn distinct_results_are_distinct_objects() {
    let mut ev = Context::new();
    // (lambda (a b) (eq (* a b) (* a b)))
    let eq_two = float_fn(
        2,
        vec![
            Op::StackRef(1),
            Op::StackRef(1),
            Op::Mul,
            Op::StackRef(2),
            Op::StackRef(2),
            Op::Mul,
            Op::Eq,
            Op::Return,
        ],
        vec![],
        &[2, 5],
    );
    let (leaf, census) = compile_in(FlonumMode::OpLocal, &eq_two);
    assert_eq!((census.results, census.escape_boxes), (2, 2));
    let args = [Value::make_float(1.5), Value::make_float(2.0)];
    let (run, allocated) = run_counting(&mut ev, &leaf, &args);
    assert_eq!(
        run,
        NativeRun::Ok(Value::NIL.bits()),
        "two results, two objects"
    );
    assert_eq!(allocated, 2);

    // (lambda (a b) (list (* a b) (* a b))): distinct, and `eql`.
    let list_two = float_fn(
        2,
        vec![
            Op::StackRef(1),
            Op::StackRef(1),
            Op::Mul,
            Op::StackRef(2),
            Op::StackRef(2),
            Op::Mul,
            Op::List(2),
            Op::Return,
        ],
        vec![],
        &[2, 5],
    );
    let (leaf, _) = compile_in(FlonumMode::OpLocal, &list_two);
    let list = ok_value(leaf.call(&mut ev as *mut Context as *mut u8, &args));
    let items = crate::emacs_core::value::list_to_vec(&list).expect("a list");
    assert_ne!(items[0].bits(), items[1].bits());
    assert_eq!(float_of(items[0]).to_bits(), float_of(items[1]).to_bits());
}

#[test]
fn deopt_mid_chain_boxes_live_flonums_like_the_interpreter() {
    let mut ev = Context::new();
    // (lambda (a b s) (let* ((x (* a b)) (y (+ x a)) (z (- y b))) (+ z s)))
    // with the let-bound copies on the stack: [a b s x y z z s] at the last +.
    let f = float_fn(
        3,
        vec![
            Op::StackRef(2),
            Op::StackRef(2),
            Op::Mul,
            Op::Dup,
            Op::StackRef(4),
            Op::Add,
            Op::Dup,
            Op::StackRef(4),
            Op::Sub,
            Op::Dup,
            Op::StackRef(4),
            Op::Add,
            Op::Return,
        ],
        vec![],
        &[2, 5, 8, 11],
    );
    let (leaf, census) = compile_in(FlonumMode::OpLocal, &f);
    assert_eq!(census.results, 4);
    assert_eq!(census.escape_boxes, 1, "the returned sum");
    // The +@5 exit holds x; -@8 holds x and y; +@11 holds x, y and z (twice,
    // one box).
    assert_eq!(census.cold_boxes, 6);
    let sym = Value::symbol("flonum-deopt-probe");
    let args = [Value::make_float(1.5), Value::make_float(2.0), sym];
    let (run, allocated) = run_counting(&mut ev, &leaf, &args);
    let NativeRun::DeoptAt(resume) = run else {
        panic!("a symbol operand must deopt precisely, got {run:?}")
    };
    assert_eq!(allocated, 3, "the exit boxes x, y and z once each");
    assert_eq!(resume.pc, 11);
    assert_eq!(resume.stack.len(), 8);
    assert_eq!(float_of(resume.stack[3]).to_bits(), 3.0_f64.to_bits());
    assert_eq!(float_of(resume.stack[4]).to_bits(), 4.5_f64.to_bits());
    assert_eq!(float_of(resume.stack[5]).to_bits(), 2.5_f64.to_bits());
    assert_eq!(
        resume.stack[5].bits(),
        resume.stack[6].bits(),
        "aliases must share one box"
    );
    assert_ne!(resume.stack[3].bits(), resume.stack[4].bits());
    assert_eq!(resume.stack[7], sym);
    let DeoptResume {
        pc,
        stack,
        handlers,
        binds,
        spec_base,
        cond_base,
        ..
    } = *resume;
    let resumed = Vm::from_context(&mut ev).run_resumed_frame(
        &f,
        Value::NIL,
        pc,
        &stack,
        handlers,
        &binds,
        spec_base,
        cond_base,
    );
    let interp = Vm::from_context(&mut ev).execute(&f, args.to_vec());
    for (what, result) in [("resumed", resumed), ("interpreter", interp)] {
        match result {
            Err(Flow::Signal(sig)) => assert_eq!(
                sig.symbol_name(),
                "wrong-type-argument",
                "{what} signals like GNU"
            ),
            other => panic!("{what}: expected wrong-type-argument, got {other:?}"),
        }
    }
    assert_eq!(ev.jit_root_stack_top, 0);
}

#[test]
fn signed_zero_and_nan_bits_survive_unboxed_chains() {
    let mut ev = Context::new();
    // (lambda (a b c) (/ (- (* a b) c) b))
    let f = float_fn(
        3,
        vec![
            Op::StackRef(2),
            Op::StackRef(2),
            Op::Mul,
            Op::StackRef(1),
            Op::Sub,
            Op::StackRef(2),
            Op::Div,
            Op::Return,
        ],
        vec![],
        &[2, 4, 6],
    );
    let (leaf, census) = compile_in(FlonumMode::OpLocal, &f);
    assert_eq!((census.results, census.escape_boxes), (3, 1));
    let signed_nan = f64::from_bits(f64::NAN.to_bits() | (1 << 63) | 5);
    let cases: [(f64, f64, f64); 8] = [
        (-0.0, 1.0, 0.0),
        (0.0, -1.0, 0.0),
        (-0.0, -0.0, -0.0),
        (f64::INFINITY, 1.0, 0.0),
        (f64::NEG_INFINITY, 2.0, 1.0),
        (signed_nan, 1.0, 0.0),
        (1.0, signed_nan, 0.0),
        // (/ (- (* 0.0 0.0) 0.0) 0.0): an invalid 0/0 is the NEGATIVE
        // default NaN, as GNU's is.
        (0.0, 0.0, 0.0),
    ];
    for (a, b, c) in cases {
        let args = [
            Value::make_float(a),
            Value::make_float(b),
            Value::make_float(c),
        ];
        let jit = float_of(ok_value(
            leaf.call(&mut ev as *mut Context as *mut u8, &args),
        ));
        let interp = float_of(
            Vm::from_context(&mut ev)
                .execute(&f, args.to_vec())
                .expect("interpreter"),
        );
        assert_eq!(
            jit.to_bits(),
            interp.to_bits(),
            "({a:?} {b:?} {c:?}): JIT {jit:?} vs interpreter {interp:?}"
        );
    }
}

#[test]
fn a_flonum_into_a_fixnum_only_site_deopts_with_a_boxed_float() {
    let mut ev = Context::new();
    // (lambda (a b) (1+ (* a b))), `1+` never saw a float (FixnumOnly).
    let f = float_fn(
        2,
        vec![
            Op::StackRef(1),
            Op::StackRef(1),
            Op::Mul,
            Op::Add1,
            Op::Return,
        ],
        vec![],
        &[2],
    );
    let (leaf, census) = compile_in(FlonumMode::OpLocal, &f);
    assert_eq!(census.results, 1);
    assert_eq!(census.escape_boxes, 0);
    assert_eq!(census.cold_boxes, 1, "the 1+ exit boxes the product");
    // Fixnum product: the tag word IS the fixnum, and `1+` runs natively.
    let (run, allocated) = run_counting(&mut ev, &leaf, &[Value::make_int(2), Value::make_int(3)]);
    assert_eq!(run, NativeRun::Ok(Value::make_int(7).bits()));
    assert_eq!(allocated, 0);
    // Float product: the guard on the tag word fails (never elided), the
    // exit boxes the float, and the interpreter reruns `1+` on it.
    let args = [Value::make_float(1.5), Value::make_float(2.0)];
    let NativeRun::DeoptAt(resume) = leaf.call(&mut ev as *mut Context as *mut u8, &args) else {
        panic!("a float into a fixnum-only 1+ must deopt precisely")
    };
    assert_eq!(resume.pc, 3);
    assert_eq!(float_of(resume.stack[2]), 3.0);
    let DeoptResume {
        pc,
        stack,
        handlers,
        binds,
        spec_base,
        cond_base,
        ..
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
        .expect("resume");
    assert_eq!(float_of(value), 4.0, "(1+ 3.0) is 4.0");
}

#[test]
fn mixed_compare_on_a_flonum_breaks_ties_on_integers() {
    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context as *mut u8;
    // (lambda (a b c) (OP (+ a b) c)), both sites Float.
    let cmp = |op: Op| {
        float_fn(
            3,
            vec![
                Op::StackRef(2),
                Op::StackRef(2),
                Op::Add,
                Op::StackRef(1),
                op,
                Op::Return,
            ],
            vec![],
            &[2, 4],
        )
    };
    let two53 = 9_007_199_254_740_992_i64;
    // (+ 2^53 1) is the fixnum 2^53+1, whose double rounds to 2^53.0.
    let args = [
        Value::make_int(two53),
        Value::make_int(1),
        Value::make_float(two53 as f64),
    ];
    for (op, expect) in [
        (Op::Gtr, Value::T),
        (Op::Eqlsign, Value::NIL),
        (Op::Leq, Value::NIL),
        (Op::Geq, Value::T),
    ] {
        let f = cmp(op.clone());
        let (leaf, census) = compile_in(FlonumMode::OpLocal, &f);
        assert_eq!((census.results, census.escape_boxes), (1, 0));
        let before = ev.tagged_heap.allocated_count();
        assert_eq!(
            leaf.call(ctx, &args),
            NativeRun::Ok(expect.bits()),
            "{op:?} (+ 2^53 1) 2^53.0 must decide on the exact integers, as GNU does"
        );
        assert_eq!(ev.tagged_heap.allocated_count(), before, "nothing escapes");
        let interp = Vm::from_context(&mut ev)
            .execute(&f, args.to_vec())
            .expect("interpreter");
        assert_eq!(interp, expect, "{op:?}: the interpreter agrees");
    }
    // A float sum against a float, and a NaN.
    let f = cmp(Op::Gtr);
    let (leaf, _) = compile_in(FlonumMode::OpLocal, &f);
    let run = |a: f64, b: f64, c: f64| {
        leaf.call(
            ctx,
            &[
                Value::make_float(a),
                Value::make_float(b),
                Value::make_float(c),
            ],
        )
    };
    assert_eq!(run(1.5, 1.0, 2.0), NativeRun::Ok(Value::T.bits()));
    assert_eq!(run(f64::NAN, 1.0, 2.0), NativeRun::Ok(Value::NIL.bits()));
}

#[test]
fn mode_off_emits_todays_boxing() {
    let mut ev = Context::new();
    let (leaf, census) = compile_in(FlonumMode::Off, &mul_add());
    assert_eq!(
        census,
        lowering::FlonumCensus::default(),
        "no flonum at all"
    );
    let args = [
        Value::make_float(1.5),
        Value::make_float(2.0),
        Value::make_float(0.25),
    ];
    let (run, allocated) = run_counting(&mut ev, &leaf, &args);
    assert_eq!(float_of(ok_value(run)), 3.25);
    assert_eq!(allocated, 2, "one box per site");
}

fn floats_consed(ev: &Context) -> u64 {
    ev.tagged_heap.memory_use_counts_snapshot()
        [crate::tagged::gc::MemoryUseCountSlot::Floats.index()]
}

#[test]
fn flonums_survive_a_call_that_collects() {
    let mut ev = Context::new();
    ev.eval_str("(defalias 'flonum-gc-callee (lambda () (garbage-collect) 7))")
        .expect("define the collecting callee");
    // (lambda (a b) (let* ((x (* a b)) (y (+ x a))) (flonum-gc-callee) (+ x y)))
    let f = float_fn(
        2,
        vec![
            Op::StackRef(1),
            Op::StackRef(1),
            Op::Mul,
            Op::Dup,
            Op::StackRef(3),
            Op::Add,
            Op::Constant(0),
            Op::Call(0),
            Op::Pop,
            Op::StackRef(1),
            Op::StackRef(1),
            Op::Add,
            Op::Return,
        ],
        vec![Value::symbol("flonum-gc-callee")],
        &[2, 5, 11],
    );
    // Resident: x and y cross the collecting call unboxed (a raw f64 needs
    // no root); local: they are boxed, and rooted, across it; off: every
    // result is boxed at its site. The collection itself conses floats too,
    // so the boxes are counted against `off`, which makes all three.
    let mut consed = Vec::new();
    for (mode, escapes) in [
        (FlonumMode::Off, 0),
        (FlonumMode::Resident, 1),
        (FlonumMode::OpLocal, 3),
    ] {
        let (leaf, census) = compile_in(mode, &f);
        let results = if mode == FlonumMode::Off { 0 } else { 3 };
        assert_eq!(census.results, results, "{mode:?}");
        assert_eq!(census.escape_boxes, escapes, "{mode:?}");
        let mut per_call = Vec::new();
        for _ in 0..3 {
            let gcs = crate::emacs_core::gc_stats::snapshot().collections;
            let before = floats_consed(&ev);
            let args = [Value::make_float(1.5), Value::make_float(2.0)];
            let got = ok_value(leaf.call(&mut ev as *mut Context as *mut u8, &args));
            assert_eq!(float_of(got).to_bits(), 7.5_f64.to_bits(), "{mode:?}");
            per_call.push(floats_consed(&ev) - before);
            assert!(
                crate::emacs_core::gc_stats::snapshot().collections > gcs,
                "the callee must really have collected"
            );
            assert_eq!(ev.jit_root_stack_top, 0);
        }
        consed.push(per_call);
    }
    let (off, resident, local) = (&consed[0], &consed[1], &consed[2]);
    for call in 0..3 {
        assert_eq!(
            off[call] - resident[call],
            2,
            "resident boxes only the returned sum: {consed:?}"
        );
        assert_eq!(off[call], local[call], "local boxes all three: {consed:?}");
    }
}

#[test]
fn a_handler_sees_boxed_flonums() {
    let mut ev = Context::new();
    ev.eval_str("(defalias 'flonum-signal-fn (lambda () (signal 'error '(\"boom\"))))")
        .expect("define the signalling callee");
    let conditions = ev.eval_str("'(error)").expect("conditions");
    // (lambda (a b) (let ((x 0.0) (y 0.0))
    //   (condition-case nil
    //       (progn (setq y (setq x (* a b))) (flonum-signal-fn))
    //     (error (list x y)))))
    // `x` and `y` are set INSIDE the protected extent, so at the signalling
    // call both slots below the handler's depth hold one unboxed product.
    let f = float_fn(
        2,
        vec![
            Op::Constant(0),
            Op::Constant(0),
            Op::Constant(1),
            Op::PushConditionCaseRaw(16),
            Op::StackRef(3),
            Op::StackRef(3),
            Op::Mul,
            Op::Dup,
            Op::StackSet(3),
            Op::Dup,
            Op::StackSet(2),
            Op::Pop,
            Op::Constant(2),
            Op::Call(0),
            Op::PopHandler,
            Op::Return,
            Op::Pop,
            Op::StackRef(1),
            Op::StackRef(1),
            Op::List(2),
            Op::Return,
        ],
        vec![
            Value::make_float(0.0),
            conditions,
            Value::symbol("flonum-signal-fn"),
        ],
        &[6],
    );
    // Resident: the product crosses the call unboxed, and the handler's
    // dispatch block boxes it once, for both slots. Local: the call boxes
    // it. Off: its site does. Signalling conses floats of its own, so each
    // mode's count is checked against `off`'s: one box in all three.
    let mut off_consed = None;
    for (mode, results, escapes, cold) in [
        (FlonumMode::Off, 0, 0, 0),
        (FlonumMode::Resident, 1, 0, 1),
        (FlonumMode::OpLocal, 1, 1, 0),
    ] {
        let (leaf, census) = compile_in(mode, &f);
        assert_eq!(census.results, results, "{mode:?}");
        assert_eq!(census.escape_boxes, escapes, "{mode:?}");
        assert_eq!(census.cold_boxes, cold, "{mode:?}");
        let before = floats_consed(&ev);
        let args = [Value::make_float(1.5), Value::make_float(2.0)];
        let got = ok_value(leaf.call(&mut ev as *mut Context as *mut u8, &args));
        let consed = floats_consed(&ev) - before;
        assert_eq!(
            *off_consed.get_or_insert(consed),
            consed,
            "{mode:?}: one box, like off"
        );
        let items = crate::emacs_core::value::list_to_vec(&got).expect("(list x y)");
        assert_eq!(float_of(items[0]), 3.0, "{mode:?}");
        assert_eq!(
            items[0].bits(),
            items[1].bits(),
            "{mode:?}: x and y are one object"
        );
        let interp = Vm::from_context(&mut ev)
            .execute(&f, args.to_vec())
            .expect("interpreter");
        let expect = crate::emacs_core::value::list_to_vec(&interp).expect("a list");
        assert_eq!(float_of(expect[0]), 3.0);
        assert_eq!(expect[0].bits(), expect[1].bits(), "the interpreter agrees");
        assert_eq!(ev.jit_root_stack_top, 0);
    }
}

#[test]
fn a_variable_read_keeps_flonums_unboxed_and_its_handler_boxes_them() {
    let mut ev = Context::new();
    ev.eval_str(
        "(defvar flonum-plain-var 0.5) \
         (defvaralias 'flonum-alias-var 'flonum-plain-var)",
    )
    .expect("define the variables");
    // (lambda (a b) (let ((x (* a b))) (+ x VAR))): the inline cell read
    // for a plain variable, the `neovm_jit_varref` shim for an alias.
    for var in ["flonum-plain-var", "flonum-alias-var"] {
        let f = float_fn(
            2,
            vec![
                Op::StackRef(1),
                Op::StackRef(1),
                Op::Mul,
                Op::VarRef(0),
                Op::Add,
                Op::Return,
            ],
            vec![Value::symbol(var)],
            &[2, 4],
        );
        let mut consed = Vec::new();
        for mode in [FlonumMode::Off, FlonumMode::OpLocal, FlonumMode::Resident] {
            let (leaf, census) = compile_in(mode, &f);
            if mode != FlonumMode::Off {
                assert_eq!(
                    (census.results, census.escape_boxes),
                    (2, 1),
                    "{var} {mode:?}: the product crosses the read unboxed"
                );
            }
            let before = floats_consed(&ev);
            let args = [Value::make_float(1.5), Value::make_float(2.0)];
            let got = ok_value(leaf.call(&mut ev as *mut Context as *mut u8, &args));
            assert_eq!(float_of(got), 3.5, "{var} {mode:?}");
            consed.push(floats_consed(&ev) - before);
        }
        assert_eq!(
            consed,
            vec![consed[0], consed[0] - 1, consed[0] - 1],
            "{var}: the product is never boxed"
        );
    }

    // A void variable signals from the read's slow path inside a
    // condition-case: the handler reads one box for both slots.
    let conditions = ev.eval_str("'(error)").expect("conditions");
    let f = float_fn(
        2,
        vec![
            Op::Constant(0),
            Op::Constant(0),
            Op::Constant(1),
            Op::PushConditionCaseRaw(15),
            Op::StackRef(3),
            Op::StackRef(3),
            Op::Mul,
            Op::Dup,
            Op::StackSet(3),
            Op::Dup,
            Op::StackSet(2),
            Op::Pop,
            Op::VarRef(2),
            Op::PopHandler,
            Op::Return,
            Op::Pop,
            Op::StackRef(1),
            Op::StackRef(1),
            Op::List(2),
            Op::Return,
        ],
        vec![
            Value::make_float(0.0),
            conditions,
            Value::symbol("flonum-void-var"),
        ],
        &[6],
    );
    for mode in [FlonumMode::OpLocal, FlonumMode::Resident] {
        let (leaf, census) = compile_in(mode, &f);
        assert_eq!(census.results, 1, "{mode:?}");
        assert_eq!(census.escape_boxes, 0, "{mode:?}: the read keeps it");
        assert_eq!(census.cold_boxes, 1, "{mode:?}: the handler entry boxes it");
        let args = [Value::make_float(1.5), Value::make_float(2.0)];
        let got = ok_value(leaf.call(&mut ev as *mut Context as *mut u8, &args));
        let items = crate::emacs_core::value::list_to_vec(&got).expect("(list x y)");
        assert_eq!(float_of(items[0]), 3.0, "{mode:?}");
        assert_eq!(items[0].bits(), items[1].bits(), "{mode:?}: one object");
        assert_eq!(ev.jit_root_stack_top, 0);
    }
}

#[test]
fn a_fused_region_entry_boxes_flonums_before_its_framestate() {
    crate::emacs_core::jit::compile::force_profit_gate_for_test(false);
    crate::emacs_core::jit::inline::force_inline_for_test(Some(true));
    let mut ev = Context::new();
    // (lambda (x) x), spliced into the caller.
    let mut callee = ByteCodeFunction::new(LambdaParams {
        required: vec![crate::emacs_core::intern::SymId(1)],
        optional: Vec::new(),
        rest: None,
    });
    callee.lexical = true;
    callee.ops = vec![Op::StackRef(0), Op::Return];
    callee.max_stack = 4;
    callee.jit_runtime().set_hot_for_test();
    let callee_value = Value::make_bytecode(callee);
    crate::emacs_core::eval::push_scratch_gc_root(callee_value);
    // (lambda (a b) (let ((p (* a b))) (eq (id p) p))): the region starts
    // with the product in two slots, its let slot and the call's argument.
    let f = float_fn(
        2,
        vec![
            Op::StackRef(1),
            Op::StackRef(1),
            Op::Mul,
            Op::Constant(0),
            Op::StackRef(1),
            Op::Call(1),
            Op::StackRef(1),
            Op::Eq,
            Op::Return,
        ],
        vec![callee_value],
        &[2],
    );
    let args = [Value::make_float(1.5), Value::make_float(2.0)];
    for mode in [FlonumMode::OpLocal, FlonumMode::Resident] {
        force_flonum_mode_for_test(Some(mode));
        let leaf = compile_bytecode_function_with(&f, Some(&ev.obarray)).expect("compiles");
        let census = lowering::flonum_census();
        assert_eq!(
            (census.results, census.escape_boxes),
            (1, 1),
            "{mode:?}: one box, at the region's entry"
        );
        let before = ev.tagged_heap.allocated_count();
        assert_eq!(
            leaf.call(&mut ev as *mut Context as *mut u8, &args),
            NativeRun::Ok(Value::T.bits()),
            "{mode:?}: (eq (id p) p)"
        );
        assert_eq!(ev.tagged_heap.allocated_count() - before, 1, "{mode:?}");
        // A failed region guard replays the call from the framestate taken
        // at the region's entry: already boxed, and aliases shared. (Only a
        // spliced call has a guard here: unfused, the body would run to the
        // end even with every guard forced to fail.)
        force_deopt_for_test(true);
        let leaf = compile_bytecode_function_with(&f, Some(&ev.obarray)).expect("compiles");
        force_deopt_for_test(false);
        let run = leaf.call(&mut ev as *mut Context as *mut u8, &args);
        let NativeRun::DeoptAt(resume) = run else {
            panic!("{mode:?}: a forced region guard must deopt precisely, got {run:?}")
        };
        assert_eq!(resume.pc, 5, "the region deopts to its call");
        assert_eq!(resume.stack.len(), 5);
        assert_eq!(float_of(resume.stack[2]), 3.0);
        assert_eq!(resume.stack[2].bits(), resume.stack[4].bits());
        let DeoptResume {
            pc,
            stack,
            handlers,
            binds,
            spec_base,
            cond_base,
            ..
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
            .expect("resume");
        assert_eq!(
            value,
            Value::T,
            "{mode:?}: the replayed call returns the same box"
        );
    }
    force_flonum_mode_for_test(None);
    crate::emacs_core::jit::inline::force_inline_for_test(None);
}
