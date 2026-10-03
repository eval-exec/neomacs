//! The bytecode fuser: splicing a constant-bytecode callee into its caller
//! must produce a body the INTERPRETER runs exactly as it ran the call.
use crate::emacs_core::bytecode::ByteCodeFunction;
use crate::emacs_core::bytecode::Vm;
use crate::emacs_core::bytecode::opcode::Op;
use crate::emacs_core::error::FlowResultExt as _;
use crate::emacs_core::eval::Context;
use crate::emacs_core::jit::NumericFeedback;
use crate::emacs_core::jit::inline::fuse_calls;
use crate::emacs_core::print::print_value;
use crate::emacs_core::value::{LambdaParams, Value};

fn lexical_fn(required: u32, ops: Vec<Op>, constants: Vec<Value>) -> ByteCodeFunction {
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: (1..=required)
            .map(crate::emacs_core::intern::SymId)
            .collect(),
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = ops;
    f.constants = constants.into();
    f.max_stack = 16;
    f
}

/// Run `f` on the interpreter and print the result or the signal.
fn interp(ev: &mut Context, f: &ByteCodeFunction, args: Vec<Value>) -> String {
    let mut vm = Vm::from_context(ev);
    match vm.execute(f, args).kinded() {
        Ok(v) => print_value(&v),
        Err(crate::emacs_core::error::FlowKind::Signal(sig)) => {
            format!("signal {}", sig.symbol_name())
        }
        Err(other) => format!("{other:?}"),
    }
}

/// A caller whose call to a constant bytecode object is spliced away runs
/// identically on the interpreter, for every shape of callee body: straight
/// line, branchy with two returns, and one that only shuffles its stack.
#[test]
fn a_fused_body_runs_as_the_call_did() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let cases: Vec<(&str, ByteCodeFunction)> = vec![
        // (lambda (x) (1+ x))
        (
            "add1",
            lexical_fn(1, vec![Op::StackRef(0), Op::Add1, Op::Return], vec![]),
        ),
        // (lambda (x) (if (> x 10) (- x 10) x)): two returns, a branch.
        (
            "clamp",
            lexical_fn(
                1,
                vec![
                    Op::StackRef(0),
                    Op::Constant(0),
                    Op::Gtr,
                    Op::GotoIfNil(8),
                    Op::StackRef(0),
                    Op::Constant(0),
                    Op::Sub,
                    Op::Return,
                    Op::StackRef(0),
                    Op::Return,
                ],
                vec![Value::make_int(10)],
            ),
        ),
        // (lambda (x) x): the identity, whose Return discards only the frame.
        (
            "id",
            lexical_fn(1, vec![Op::StackRef(0), Op::Return], vec![]),
        ),
        // (lambda (a b) (+ a b)): two arguments. (`cons` is deliberately NOT
        // admissible — see `op_is_inlinable` — so a two-argument shape has to
        // be built from something that cannot allocate.)
        (
            "pair",
            lexical_fn(
                2,
                vec![Op::StackRef(1), Op::StackRef(1), Op::Add, Op::Return],
                vec![],
            ),
        ),
    ];
    for (name, callee) in cases {
        callee.jit_runtime().set_hot_for_test();
        let nargs = callee.params.required.len();
        let callee_value = Value::make_bytecode(callee);
        crate::emacs_core::eval::push_scratch_gc_root(callee_value);
        // (lambda (y) (cons (CALLEE y [y]) 'tail))
        let mut ops = vec![Op::Constant(0)];
        for k in 0..nargs {
            // The argument `y` sits one slot below the callee push, and one
            // lower again for every argument already pushed.
            ops.push(Op::StackRef((1 + k) as u16));
        }
        ops.extend([
            Op::Call(nargs as u16),
            Op::Constant(1),
            Op::Cons,
            Op::Return,
        ]);
        let caller = lexical_fn(1, ops, vec![callee_value, Value::symbol("tail")]);
        let feedback = vec![NumericFeedback::FixnumOnly; caller.ops.len()];
        let fused = fuse_calls(&caller.ops, &caller.constants, None, 1, &feedback)
            .unwrap_or_else(|| panic!("{name}: the call must fuse"));
        assert_eq!(fused.regions.len(), 1, "{name}: one region");
        let mut fused_fn = lexical_fn(1, fused.ops.clone(), fused.constants.clone());
        fused_fn.max_stack = 32;
        for arg in [Value::make_int(5), Value::make_int(40), Value::make_int(-7)] {
            let want = interp(&mut ev, &caller, vec![arg]);
            let got = interp(&mut ev, &fused_fn, vec![arg]);
            assert_eq!(got, want, "{name} with {}", print_value(&arg));
        }
    }
}

/// End to end: with inlining on, the caller's compiled leaf answers what the
/// interpreter answers — including for an argument that makes a spliced guard
/// fail, which must deopt back to the CALL and re-run it interpreted.
#[test]
fn an_inlined_call_runs_and_deopts_as_the_interpreter_does() {
    crate::test_utils::init_test_tracing();
    crate::emacs_core::jit::compile::force_profit_gate_for_test(false);
    crate::emacs_core::jit::inline::force_inline_for_test(Some(true));
    let mut ev = Context::new();
    // (lambda (x) (if (> x 10) (- x 10) x))
    let callee = lexical_fn(
        1,
        vec![
            Op::StackRef(0),
            Op::Constant(0),
            Op::Gtr,
            Op::GotoIfNil(8),
            Op::StackRef(0),
            Op::Constant(0),
            Op::Sub,
            Op::Return,
            Op::StackRef(0),
            Op::Return,
        ],
        vec![Value::make_int(10)],
    );
    callee.jit_runtime().set_hot_for_test();
    let callee_value = Value::make_bytecode(callee);
    crate::emacs_core::eval::push_scratch_gc_root(callee_value);
    // (lambda (y) (cons (callee y) 'tail))
    let caller = lexical_fn(
        1,
        vec![
            Op::Constant(0),
            Op::StackRef(1),
            Op::Call(1),
            Op::Constant(1),
            Op::Cons,
            Op::Return,
        ],
        vec![callee_value, Value::symbol("tail")],
    );
    caller.jit_runtime().set_hot_for_test();
    let interp_only = caller.clone();
    let caller_value = Value::make_bytecode(caller);
    crate::emacs_core::eval::push_scratch_gc_root(caller_value);
    let leaf = crate::emacs_core::jit::compile::compile_bytecode_function_with(
        Value::from_bits(caller_value.bits())
            .get_bytecode_data()
            .expect("bytecode"),
        Some(&ev.obarray),
    )
    .expect("the fused caller compiles");
    assert!(
        leaf.reloc_data
            .iter()
            .any(|v| v.bits() == callee_value.bits()),
        "the callee object stays rooted through the leaf's reloc vector"
    );
    for arg in [
        Value::make_int(5),
        Value::make_int(40),
        Value::symbol("not-a-number"),
        Value::make_float(12.5),
    ] {
        let want = interp(&mut ev, &interp_only, vec![arg]);
        let got = match ev
            .funcall_general_untraced(caller_value, vec![arg])
            .kinded()
        {
            Ok(v) => print_value(&v),
            Err(crate::emacs_core::error::FlowKind::Signal(sig)) => {
                format!("signal {}", sig.symbol_name())
            }
            Err(other) => format!("{other:?}"),
        };
        assert_eq!(got, want, "argument {}", print_value(&arg));
    }
    crate::emacs_core::jit::inline::force_inline_for_test(None);
}

/// Build a hot one-argument callee and the caller `(lambda (y z) ...)` that
/// calls it, returning `(caller, callee_value)`. The caller's own `Add` sits
/// AFTER the call, so its guard is a caller-code deopt site.
fn caller_with_a_guard_after_the_call() -> (ByteCodeFunction, Value) {
    // (lambda (x) (1+ x)) — no guard of its own beyond the arithmetic.
    let callee = lexical_fn(1, vec![Op::StackRef(0), Op::Add1, Op::Return], vec![]);
    callee.jit_runtime().set_hot_for_test();
    let callee_value = Value::make_bytecode(callee);
    crate::emacs_core::eval::push_scratch_gc_root(callee_value);
    // (lambda (y z) (+ (callee y) z))
    let caller = lexical_fn(
        2,
        vec![
            Op::Constant(0),
            Op::StackRef(2),
            Op::Call(1),
            Op::StackRef(1),
            Op::Add,
            Op::Return,
        ],
        vec![callee_value],
    );
    caller.jit_runtime().set_hot_for_test();
    (caller, callee_value)
}

/// A deopt raised at one of the CALLER's own ops, positioned after a spliced
/// region, resumes the interpreter in the caller's UNFUSED body — so the pc it
/// records has to be the original index. Splicing shifts every later caller op
/// (a callee `Return` alone expands to two), so a fused index there names a
/// different instruction, or runs off the end of the body entirely.
#[test]
fn a_deopt_after_a_spliced_region_resumes_the_original_body() {
    crate::test_utils::init_test_tracing();
    crate::emacs_core::jit::compile::force_profit_gate_for_test(false);
    crate::emacs_core::jit::inline::force_inline_for_test(Some(true));
    let mut ev = Context::new();
    let (caller, _callee_value) = caller_with_a_guard_after_the_call();
    let interp_only = caller.clone();
    let caller_value = Value::make_bytecode(caller);
    crate::emacs_core::eval::push_scratch_gc_root(caller_value);
    let fused = fuse_calls(
        &interp_only.ops,
        &interp_only.constants,
        None,
        2,
        &vec![NumericFeedback::FixnumOnly; interp_only.ops.len()],
    )
    .expect("the call must fuse");
    assert!(
        fused.ops.len() > interp_only.ops.len(),
        "the splice has to shift the caller's tail for this test to mean anything"
    );
    crate::emacs_core::jit::compile::compile_bytecode_function_with(
        Value::from_bits(caller_value.bits())
            .get_bytecode_data()
            .expect("bytecode"),
        Some(&ev.obarray),
    )
    .expect("the fused caller compiles");
    // `z` is what the caller's own `Add` guards; a non-number there deopts at
    // a CALLER op, past the end of the region.
    for z in [
        Value::make_int(7),
        Value::symbol("not-a-number"),
        Value::make_float(0.5),
    ] {
        let args = vec![Value::make_int(3), z];
        let want = interp(&mut ev, &interp_only, args.clone());
        let got = match ev.funcall_general_untraced(caller_value, args).kinded() {
            Ok(v) => print_value(&v),
            Err(crate::emacs_core::error::FlowKind::Signal(sig)) => {
                format!("signal {}", sig.symbol_name())
            }
            Err(other) => format!("{other:?}"),
        };
        assert_eq!(got, want, "z = {}", print_value(&z));
    }
    crate::emacs_core::jit::inline::force_inline_for_test(None);
}

/// The constant-propagation that picks a splice site is a selection heuristic
/// — it does not model jump-table edges — so the region's first act is a check
/// that the callee slot still holds the object whose body follows. A callee
/// body with no guard of its own would otherwise compile to a leaf with no
/// deopt site at all.
#[test]
fn a_spliced_region_rechecks_the_callee_it_speculated_on() {
    crate::test_utils::init_test_tracing();
    crate::emacs_core::jit::compile::force_profit_gate_for_test(false);
    crate::emacs_core::jit::inline::force_inline_for_test(Some(true));
    let ev = Context::new();
    // (lambda (x) x): no arithmetic, no car/cdr — nothing that guards.
    let callee = lexical_fn(1, vec![Op::StackRef(0), Op::Return], vec![]);
    callee.jit_runtime().set_hot_for_test();
    let callee_value = Value::make_bytecode(callee);
    crate::emacs_core::eval::push_scratch_gc_root(callee_value);
    let caller = lexical_fn(
        1,
        vec![Op::Constant(0), Op::StackRef(1), Op::Call(1), Op::Return],
        vec![callee_value],
    );
    caller.jit_runtime().set_hot_for_test();
    let fused = fuse_calls(
        &caller.ops,
        &caller.constants,
        None,
        1,
        &vec![NumericFeedback::FixnumOnly; caller.ops.len()],
    )
    .expect("the call must fuse");
    assert_eq!(
        fused.regions[0].callee_bits,
        callee_value.bits() as u64,
        "the region records the callee it speculated on"
    );
    let caller_value = Value::make_bytecode(caller);
    crate::emacs_core::eval::push_scratch_gc_root(caller_value);
    crate::emacs_core::jit::compile::compile_bytecode_function_with(
        Value::from_bits(caller_value.bits())
            .get_bytecode_data()
            .expect("bytecode"),
        Some(&ev.obarray),
    )
    .expect("the fused caller compiles");
    let (_, _, sites, _) =
        crate::emacs_core::jit::compile::lowering::LAST_IR_STATS.with(|c| c.get());
    assert!(
        sites >= 1,
        "a region has to emit its callee re-check, even around a guard-free body"
    );
    crate::emacs_core::jit::inline::force_inline_for_test(None);
}

/// The admission rules that keep a region's deopt framestate honest, and the
/// one caller shape whose jump operands this pass must not rewrite.
#[test]
fn an_inadmissible_shape_is_left_as_a_call() {
    crate::test_utils::init_test_tracing();
    crate::emacs_core::jit::inline::force_inline_for_test(Some(true));
    let _ev = Context::new();
    let fuse_with = |callee_ops: Vec<Op>, caller_extra: Vec<Op>| {
        let callee = lexical_fn(1, callee_ops, vec![]);
        callee.jit_runtime().set_hot_for_test();
        let callee_value = Value::make_bytecode(callee);
        crate::emacs_core::eval::push_scratch_gc_root(callee_value);
        let mut ops = vec![Op::Constant(0), Op::StackRef(1), Op::Call(1)];
        ops.extend(caller_extra);
        ops.push(Op::Return);
        let caller = lexical_fn(1, ops, vec![callee_value, Value::symbol("tail")]);
        let feedback = vec![NumericFeedback::FixnumOnly; caller.ops.len()];
        fuse_calls(&caller.ops, &caller.constants, None, 1, &feedback).is_some()
    };
    // The control: an admissible callee in a plain caller does fuse, so the
    // rejections below are attributable to the rule under test.
    assert!(
        fuse_with(vec![Op::StackRef(0), Op::Add1, Op::Return], vec![]),
        "control: a fixnum-only callee fuses"
    );
    // Allocation inside a region could collect between the framestate snapshot
    // taken at region entry and a deopt that spills it.
    assert!(
        !fuse_with(
            vec![Op::StackRef(0), Op::StackRef(0), Op::Cons, Op::Return],
            vec![]
        ),
        "a callee that conses is not admissible"
    );
    // A back edge takes a poll, which is also a safe point.
    assert!(
        !fuse_with(
            vec![Op::StackRef(0), Op::GotoIfNotNil(0), Op::Return],
            vec![]
        ),
        "a callee with a back edge is not admissible"
    );
    // The `Return` expansion is the only thing that rebalances the stack, so a
    // body that could fall off its end would leave the callee frame behind.
    // No such body reaches the fuser: sealing (the decoder in production, the
    // test normalizer in `make_bytecode`) ends every executable body in a
    // `Return`, which is the invariant `inlinable_verdict`'s check documents.
    let sealed = Value::make_bytecode(lexical_fn(1, vec![Op::StackRef(0), Op::Add1], vec![]));
    crate::emacs_core::eval::push_scratch_gc_root(sealed);
    assert!(
        matches!(
            sealed
                .get_bytecode_data()
                .expect("bytecode")
                .executable_ops()
                .last(),
            Some(Op::Return)
        ),
        "sealing terminates a hand-assembled body with a Return"
    );
    crate::emacs_core::jit::inline::force_inline_for_test(None);
}

/// A spliced `Op::Constant` names its slot in a u16, and every splice appends
/// the callee's whole pool: a caller whose fused pool would pass 65,536 keeps
/// its call rather than wrap an index onto an unrelated constant.
#[test]
fn a_splice_that_would_overflow_the_constant_pool_is_left_as_a_call() {
    crate::test_utils::init_test_tracing();
    let _ev = Context::new();
    let callee = lexical_fn(
        1,
        vec![Op::StackRef(0), Op::Constant(2), Op::Add, Op::Return],
        vec![Value::make_int(1), Value::make_int(2), Value::make_int(3)],
    );
    callee.jit_runtime().set_hot_for_test();
    let callee_value = Value::make_bytecode(callee);
    crate::emacs_core::eval::push_scratch_gc_root(callee_value);
    let fuses_with_pool = |caller_pool: usize| {
        let mut pool = vec![callee_value];
        pool.extend((1..caller_pool).map(|i| Value::make_int(i as i64)));
        let caller = lexical_fn(
            1,
            vec![Op::Constant(0), Op::StackRef(1), Op::Call(1), Op::Return],
            pool,
        );
        let feedback = vec![NumericFeedback::FixnumOnly; caller.ops.len()];
        fuse_calls(&caller.ops, &caller.constants, None, 1, &feedback)
    };
    let control = fuses_with_pool(100).expect("control: a small pool fuses");
    assert!(
        control
            .ops
            .iter()
            .any(|op| matches!(op, Op::Constant(i) if *i as usize == 100 + 2)),
        "control: the spliced constant is rebased past the caller's pool"
    );
    assert!(
        fuses_with_pool(u16::MAX as usize + 1 - 3).is_some(),
        "exactly 65,536 fused constants still fit"
    );
    assert!(
        fuses_with_pool(u16::MAX as usize + 1 - 2).is_none(),
        "65,537 fused constants would wrap a spliced index"
    );
}

/// A handler installed by the CALLER serves two consumers that need different
/// numbering: the compiled dispatch reaches its handler block by fused pc, but
/// the runtime frame the push creates is later adopted by a RESUMED INTERPRETER
/// frame, which jumps to its target in the unfused ops. Here a `car` guard
/// after the region, inside the protected extent, deopts; the interpreter then
/// signals, and the handler it adopted has to land on the real handler code.
///
///     (lambda (y z) (condition-case nil (+ (f y) (car z)) (error 'caught)))
#[test]
fn a_caller_handler_catches_in_the_original_body_after_a_deopt() {
    crate::test_utils::init_test_tracing();
    crate::emacs_core::jit::compile::force_profit_gate_for_test(false);
    crate::emacs_core::jit::inline::force_inline_for_test(Some(true));
    let mut ev = Context::new();
    // (lambda (x) (1+ x))
    let callee = lexical_fn(1, vec![Op::StackRef(0), Op::Add1, Op::Return], vec![]);
    callee.jit_runtime().set_hot_for_test();
    let callee_value = Value::make_bytecode(callee);
    crate::emacs_core::eval::push_scratch_gc_root(callee_value);
    let caller = lexical_fn(
        2,
        vec![
            Op::PushConditionCase(9), // 0                  [y z]
            Op::Constant(0),          // 1: f               [y z f]
            Op::StackRef(2),          // 2: y               [y z f y]
            Op::Call(1),              // 3: spliced         [y z r]
            Op::StackRef(1),          // 4: z               [y z r z]
            Op::Car,                  // 5: caller guard    [y z r cz]
            Op::Add,                  // 6                  [y z s]
            Op::PopHandler,           // 7                  [y z s]
            Op::Return,               // 8
            Op::Constant(1),          // 9: handler         [y z err caught]
            Op::Return,               // 10
        ],
        vec![callee_value, Value::symbol("caught")],
    );
    caller.jit_runtime().set_hot_for_test();
    let fused = fuse_calls(
        &caller.ops,
        &caller.constants,
        None,
        2,
        &vec![NumericFeedback::FixnumOnly; caller.ops.len()],
    )
    .expect("a caller that installs a handler fuses");
    assert!(
        fused
            .ops
            .iter()
            .any(|op| matches!(op, Op::PushConditionCase(t) if *t != 9)),
        "the splice has to move the handler target for this test to mean anything"
    );
    let interp_only = caller.clone();
    let caller_value = Value::make_bytecode(caller);
    crate::emacs_core::eval::push_scratch_gc_root(caller_value);
    crate::emacs_core::jit::compile::compile_bytecode_function_with(
        Value::from_bits(caller_value.bits())
            .get_bytecode_data()
            .expect("bytecode"),
        Some(&ev.obarray),
    )
    .expect("the fused caller compiles");
    let pair = |n: i64| {
        let c = Value::cons(Value::make_int(n), Value::NIL);
        crate::emacs_core::eval::push_scratch_gc_root(c);
        c
    };
    for (y, z) in [
        (Value::make_int(3), pair(7)),              // straight through
        (Value::make_int(3), Value::symbol("sym")), // caller guard deopts, then catches
        (Value::make_float(1.5), pair(7)),          // region deopts, call re-runs
        (Value::symbol("sym"), pair(7)),            // region deopts, call signals, catches
    ] {
        let args = vec![y, z];
        let want = interp(&mut ev, &interp_only, args.clone());
        let got = match ev.funcall_general_untraced(caller_value, args).kinded() {
            Ok(v) => print_value(&v),
            Err(crate::emacs_core::error::FlowKind::Signal(sig)) => {
                format!("signal {}", sig.symbol_name())
            }
            Err(other) => format!("{other:?}"),
        };
        assert_eq!(
            got,
            want,
            "y = {}, z = {}",
            print_value(&y),
            print_value(&z)
        );
    }
    crate::emacs_core::jit::inline::force_inline_for_test(None);
}
