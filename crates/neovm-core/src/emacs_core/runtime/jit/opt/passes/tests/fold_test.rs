//! Fold regressions use the independent IR evaluator and the real Tier-0 ops.

use super::*;
use crate::emacs_core::bytecode::{ByteCodeFunction, Op, Vm};
use crate::emacs_core::eval::Context;
use crate::emacs_core::jit::opt::{
    build::{BuildInput, build},
    eval::{Inputs, Outcome, evaluate},
    ir::{Opcode, ParamShape, Term, ValueBits},
};
use crate::emacs_core::value::{LambdaParams, Value};

fn source(ops: Vec<Op>, constants: Vec<Value>, arity: usize) -> ByteCodeFunction {
    let mut function = ByteCodeFunction::new(LambdaParams {
        required: (0..arity)
            .map(|_| crate::emacs_core::intern::intern("opt-fold-arg"))
            .collect(),
        optional: Vec::new(),
        rest: None,
    });
    function.lexical = true;
    function.max_stack = 64;
    function.ops = ops;
    function.constants = constants.into();
    function
}

fn plan(function: &ByteCodeFunction, prefix: usize) -> Func {
    let params = ParamShape {
        required: function.params.required.len(),
        ..ParamShape::default()
    };
    let cfg = crate::emacs_core::jit::compile::analyze_cfg(
        function.executable_ops(),
        &function.constants,
        function.executable_gnu_byte_offset_map(),
        params.native_arity(),
    )
    .expect("source CFG");
    let constants = function
        .constants
        .iter()
        .copied()
        .map(ValueBits::from_value)
        .collect::<Vec<_>>();
    build(BuildInput {
        ops: function.executable_ops(),
        constants: &constants,
        cfg: &cfg,
        params,
        dynamic_prefix: prefix,
        fused: None,
        osr: None,
    })
    .expect("source SSA")
}

fn returned(ctx: &mut Context, func: &Func, args: &[Value], prefix: &[Value]) -> ValueBits {
    let run = evaluate(
        func,
        ctx,
        Inputs {
            args,
            prefix,
            ..Inputs::default()
        },
    )
    .expect("reference execution");
    let Outcome::Returned(value) = run.outcome else {
        panic!("expected reference return")
    };
    value
}

#[test]
fn opt_fold_constant_predicate_prunes_unexecuted_effect_arm() {
    let mut ctx = Context::new();
    ctx.eval_str("(setq opt-fold-effects 0)").unwrap();
    let function = source(
        vec![
            Op::Constant(0),
            Op::Consp,
            Op::GotoIfNil(7),
            Op::Constant(1),
            Op::VarSet(2),
            Op::Nil,
            Op::Return,
            Op::Constant(3),
            Op::Return,
        ],
        vec![
            Value::fixnum(7),
            Value::fixnum(42),
            Value::symbol("opt-fold-effects"),
            Value::fixnum(17),
        ],
        0,
    );
    let original = plan(&function, 0);
    let mut folded = original.clone();
    let stats = run(&mut folded).expect("fold pass");
    assert!(
        stats.branches_folded > 0,
        "the known false branch must fold"
    );
    assert!(
        folded
            .blocks
            .iter()
            .all(|block| !matches!(block.term, Term::Branch { .. }))
    );
    assert!(
        !folded
            .insts
            .iter()
            .any(|inst| matches!(inst.op, Opcode::Opaque(Op::VarSet(_))))
    );
    folded.verify().expect("transformed SSA");
    let expected = {
        let mut vm = Vm::from_context(&mut ctx);
        vm.force_interpreter_only_for_test();
        ValueBits::from_value(vm.execute(&function, Vec::new()).unwrap())
    };
    assert_eq!(returned(&mut ctx, &original, &[], &[]), expected);
    assert_eq!(returned(&mut ctx, &folded, &[], &[]), expected);
    assert_eq!(ctx.eval_str("opt-fold-effects").unwrap(), Value::fixnum(0));
}

#[test]
fn opt_fold_cons_reads_use_non_nil_and_same_block_guard_proofs() {
    let mut ctx = Context::new();
    let function = source(
        vec![
            Op::Dup,
            Op::GotoIfNil(7),
            Op::Dup,
            Op::Car,
            Op::Pop,
            Op::Cdr,
            Op::Goto(0),
            Op::Return,
        ],
        Vec::new(),
        1,
    );
    let original = plan(&function, 0);
    let mut folded = original.clone();
    let stats = run(&mut folded).expect("fold pass");
    assert_eq!(
        stats.cons_loads, 2,
        "both proven cons reads use typed loads"
    );
    assert!(stats.guards_narrowed > 0, "non-nil list is a cons");
    assert!(
        stats.guards_folded > 0,
        "the first cons guard proves the alias"
    );
    let polls = |func: &Func| {
        func.insts
            .iter()
            .filter(|inst| inst.op == Opcode::Poll)
            .count()
    };
    assert_eq!(polls(&folded), polls(&original));
    assert!(polls(&folded) > 0);
    for input in [
        Value::NIL,
        Value::list(vec![Value::fixnum(1), Value::fixnum(2), Value::fixnum(3)]),
    ] {
        let expected = {
            let mut vm = Vm::from_context(&mut ctx);
            vm.force_interpreter_only_for_test();
            ValueBits::from_value(vm.execute(&function, vec![input]).unwrap())
        };
        assert_eq!(returned(&mut ctx, &original, &[input], &[]), expected);
        assert_eq!(returned(&mut ctx, &folded, &[input], &[]), expected);
    }
    // A truthy improper tail must still deopt at the original car, with the
    // full residual GNU stack. There is no speculation across loop entries.
    let invalid = Value::fixnum(7);
    let before = evaluate(
        &original,
        &mut ctx,
        Inputs {
            args: &[invalid],
            ..Inputs::default()
        },
    )
    .unwrap();
    let after = evaluate(
        &folded,
        &mut ctx,
        Inputs {
            args: &[invalid],
            ..Inputs::default()
        },
    )
    .unwrap();
    let (Outcome::Deopt(before), Outcome::Deopt(after)) = (before.outcome, after.outcome) else {
        panic!("improper input must retain the type deopt")
    };
    assert_eq!(after, before);
}

#[test]
fn opt_fold_impossible_guard_preserves_prior_effect_and_exact_frame() {
    let mut ctx = Context::new();
    ctx.eval_str("(setq opt-fold-effects 0)").unwrap();
    let function = source(
        vec![
            Op::Constant(0),
            Op::VarSet(1),
            Op::Constant(2),
            Op::Car,
            Op::Return,
        ],
        vec![
            Value::fixnum(42),
            Value::symbol("opt-fold-effects"),
            Value::fixnum(7),
        ],
        0,
    );
    let original = plan(&function, 0);
    let mut folded = original.clone();
    let stats = run(&mut folded).expect("fold pass");
    assert_eq!(
        stats.deopts, 1,
        "the contradictory guard becomes a terminator"
    );
    assert!(
        folded
            .blocks
            .iter()
            .any(|block| matches!(block.term, Term::Deopt(_)))
    );
    assert!(
        folded
            .insts
            .iter()
            .any(|inst| matches!(inst.op, Opcode::Opaque(Op::VarSet(_))))
    );
    let before = evaluate(&original, &mut ctx, Inputs::default()).unwrap();
    ctx.eval_str("(setq opt-fold-effects 0)").unwrap();
    let after = evaluate(&folded, &mut ctx, Inputs::default()).unwrap();
    let (Outcome::Deopt(before), Outcome::Deopt(after)) = (before.outcome, after.outcome) else {
        panic!("both plans must deopt")
    };
    assert_eq!(after, before);
    assert_eq!(ctx.eval_str("opt-fold-effects").unwrap(), Value::fixnum(42));
    folded
        .verify()
        .expect("truncated source stacks remain valid");
}

#[test]
fn opt_fold_environment_constant_is_unknown_for_every_instance() {
    let mut ctx = Context::new();
    let function = source(
        vec![
            Op::Constant(0),
            Op::Consp,
            Op::GotoIfNil(5),
            Op::Constant(1),
            Op::Return,
            Op::Constant(2),
            Op::Return,
        ],
        vec![Value::fixnum(7), Value::fixnum(1), Value::fixnum(2)],
        0,
    );
    let original = plan(&function, 1);
    let mut folded = original.clone();
    let stats = run(&mut folded).unwrap();
    assert_eq!(stats.branches_folded, 0);
    assert!(
        folded
            .insts
            .iter()
            .any(|inst| inst.op == Opcode::EnvConst(0))
    );
    for prefix in [Value::fixnum(7), Value::list(vec![Value::fixnum(7)])] {
        assert_eq!(
            returned(&mut ctx, &folded, &[], &[prefix]),
            returned(&mut ctx, &original, &[], &[prefix])
        );
    }
}

#[test]
fn opt_fold_threads_boolean_phi_without_skipping_pinned_operations() {
    let mut ctx = Context::new();
    let function = source(
        vec![
            Op::Dup,
            Op::GotoIfNil(4),
            Op::True,
            Op::Goto(5),
            Op::Nil,
            Op::GotoIfNil(8),
            Op::Constant(0),
            Op::Return,
            Op::Constant(1),
            Op::Return,
        ],
        vec![Value::fixnum(1), Value::fixnum(2)],
        1,
    );
    let original = plan(&function, 0);
    let mut folded = original.clone();
    let stats = run(&mut folded).unwrap();
    assert!(
        stats.threaded_edges >= 2,
        "each exact incoming boolean selects its own destination"
    );
    let branches = |func: &Func| {
        func.blocks
            .iter()
            .filter(|block| matches!(block.term, Term::Branch { .. }))
            .count()
    };
    assert!(branches(&folded) < branches(&original));
    for input in [
        Value::NIL,
        Value::T,
        Value::fixnum(0),
        Value::list(vec![Value::fixnum(1)]),
    ] {
        let expected = {
            let mut vm = Vm::from_context(&mut ctx);
            vm.force_interpreter_only_for_test();
            ValueBits::from_value(vm.execute(&function, vec![input]).unwrap())
        };
        assert_eq!(returned(&mut ctx, &original, &[input], &[]), expected);
        assert_eq!(returned(&mut ctx, &folded, &[input], &[]), expected);
    }
}

#[test]
fn opt_fold_seeded_loop_diamonds_preserve_tier0_and_every_observable_stack() {
    let mut ctx = Context::new();
    for seed in 1_u64..=16 {
        let mut state = seed;
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let function = source(
            vec![
                Op::Constant(0),
                Op::Constant(0),
                Op::StackRef(1),
                Op::StackRef(3),
                Op::Lss,
                Op::GotoIfNil(23),
                Op::StackRef(1),
                Op::Constant(1),
                Op::Rem,
                Op::GotoIfNil(15),
                Op::StackRef(0),
                Op::Constant(2),
                Op::Add,
                Op::StackSet(1),
                Op::Goto(19),
                Op::StackRef(0),
                Op::Constant(3),
                Op::Sub,
                Op::StackSet(1),
                Op::StackRef(1),
                Op::Add1,
                Op::StackSet(2),
                Op::Goto(2),
                Op::Return,
            ],
            vec![
                Value::fixnum(0),
                Value::fixnum(2),
                Value::fixnum((state % 13) as i64),
                Value::fixnum(((state >> 8) % 11) as i64),
            ],
            1,
        );
        let original = plan(&function, 0);
        let mut folded = original.clone();
        run(&mut folded).unwrap();
        let argument = Value::fixnum((state % 18) as i64);
        let expected = {
            let mut vm = Vm::from_context(&mut ctx);
            vm.force_interpreter_only_for_test();
            ValueBits::from_value(vm.execute(&function, vec![argument]).unwrap())
        };
        let before = evaluate(
            &original,
            &mut ctx,
            Inputs {
                args: &[argument],
                ..Inputs::default()
            },
        )
        .unwrap();
        let after = evaluate(
            &folded,
            &mut ctx,
            Inputs {
                args: &[argument],
                ..Inputs::default()
            },
        )
        .unwrap();
        assert_eq!(
            after.trace, before.trace,
            "seed {seed}: full GNU source stacks"
        );
        let (Outcome::Returned(before), Outcome::Returned(after)) = (before.outcome, after.outcome)
        else {
            panic!("seed {seed}: loop must return")
        };
        assert_eq!(before, expected, "seed {seed}");
        assert_eq!(after, expected, "seed {seed}");
    }
}

#[test]
fn opt_fold_gnu_dhry_proc3_alias_join_remains_verifiable() {
    let mut ctx = Context::new();
    // Copied from the GNU-built elisp-benchmarks-1.16 dhrystone.elc fixture.
    // Its ret slot joins the original unknown argument and a heap-load result;
    // later cl-struct checks retain both values in complete source frames.
    let raw = vec![
        137, 8, 131, 8, 0, 136, 8, 65, 8, 64, 196, 1, 33, 9, 62, 132, 25, 0, 197, 198, 199, 3, 68,
        34, 136, 137, 200, 72, 178, 1, 196, 1, 33, 10, 62, 132, 45, 0, 197, 198, 201, 3, 68, 34,
        136, 137, 200, 202, 203, 11, 34, 73, 182, 2, 135,
    ];
    let mut constants = vec![
        Value::symbol("dhry-ptr-glob"),
        Value::symbol("cl-struct-dhry-record-tags"),
        Value::symbol("cl-struct-dhry-var-1-tags"),
        Value::symbol("dhry-int-glob"),
        Value::symbol("type-of"),
        Value::symbol("signal"),
        Value::symbol("wrong-type-argument"),
        Value::symbol("dhry-record"),
        Value::fixnum(2),
        Value::symbol("dhry-var-1"),
        Value::symbol("dhry-proc-7"),
        Value::fixnum(10),
    ];
    let (ops, offsets) = crate::emacs_core::bytecode::decode::decode_gnu_bytecode_with_offset_map(
        &raw,
        &mut constants,
    )
    .expect("GNU fixture decodes");
    let mut function = source(ops, constants, 1);
    function.max_stack = 8;
    function.gnu_byte_offset_map = Some(offsets);
    function.gnu_bytecode_bytes = Some(crate::tagged::header::LispByteVec::owned(raw));
    // Installing GNU bytes must use the canonical decode publication path.
    // Merely attaching decoded ops does not establish the unchecked driver's
    // sealed-code proof, and test-only hand-assembly sealing refuses GNU bodies.
    function
        .restore_gnu_decode_policy()
        .expect("GNU decode publication");
    assert!(function.executes_sealed_ops());
    assert!(function.executes_verified_ops());
    let original = plan(&function, 0);
    assert_eq!(function.executable_ops().len(), 47);
    assert_eq!(
        (
            original.blocks.len(),
            original.insts.len(),
            original.census.phis
        ),
        (11, 35, 1)
    );
    let mut folded = original.clone();
    let result = run(&mut folded);
    assert!(
        result.is_ok(),
        "GNU dhry-proc-3 fold must remain admissible: {result:?}\n{}",
        folded.display()
    );
    folded.verify().expect("folded GNU helper remains valid");

    ctx.eval_str(
        "(progn
           (setq cl-struct-dhry-record-tags '(dhry-record)
                 cl-struct-dhry-var-1-tags '(dhry-var-1)
                 dhry-int-glob 5
                 dhry-ptr-glob (cons (record 'dhry-record 0 (record 'dhry-var-1 0 0 \"x\")) '(tail))))",
    ).expect("runtime inputs for the GNU fixture");
    ctx.obarray.set_symbol_function_id(
        crate::emacs_core::intern::intern("dhry-proc-7"),
        Value::make_bytecode(source(
            vec![
                Op::StackRef(1),
                Op::Constant(0),
                Op::Add,
                Op::StackRef(1),
                Op::StackRef(1),
                Op::Add,
                Op::Return,
            ],
            vec![Value::fixnum(2)],
            2,
        )),
    );
    for argument in [
        Value::NIL,
        Value::fixnum(37),
        Value::list(vec![Value::fixnum(1)]),
    ] {
        let reset = "(aset (aref (car dhry-ptr-glob) 2) 2 0)";
        let observed = "(aref (aref (car dhry-ptr-glob) 2) 2)";
        ctx.eval_str(reset).unwrap();
        let expected = {
            let mut vm = Vm::from_context(&mut ctx);
            vm.force_interpreter_only_for_test();
            let value = vm
                .execute(&function, vec![argument])
                .unwrap_or_else(|flow| {
                    let strings = flow.as_signal().map(|signal| {
                        signal
                            .data
                            .iter()
                            .filter_map(|value| value.as_runtime_string_owned())
                            .collect::<Vec<_>>()
                    });
                    panic!("Tier0 GNU fixture execution failed: {flow:?}; strings={strings:?}")
                });
            ValueBits::from_value(value)
        };
        let expected_store = ctx.eval_str(observed).unwrap();
        ctx.eval_str(reset).unwrap();
        let before = evaluate(
            &original,
            &mut ctx,
            Inputs {
                args: &[argument],
                ..Inputs::default()
            },
        )
        .unwrap();
        assert_eq!(ctx.eval_str(observed).unwrap(), expected_store);
        ctx.eval_str(reset).unwrap();
        let after = evaluate(
            &folded,
            &mut ctx,
            Inputs {
                args: &[argument],
                ..Inputs::default()
            },
        )
        .unwrap();
        assert_eq!(ctx.eval_str(observed).unwrap(), expected_store);
        assert_eq!(after.trace, before.trace, "original GNU source stacks");
        let (Outcome::Returned(before), Outcome::Returned(after)) = (before.outcome, after.outcome)
        else {
            panic!("valid GNU records must return")
        };
        assert_eq!(before, expected);
        assert_eq!(after, expected);
    }
}
