use super::*;
use crate::emacs_core::value::LambdaParams;

fn nullary() -> ByteCodeFunction {
    ByteCodeFunction::new(LambdaParams {
        required: Vec::new(),
        optional: Vec::new(),
        rest: None,
    })
}

#[test]
fn compiles_constant_return() {
    // (lambda () 42)  ==  [Constant(0), Return], constants = [42]
    let c = Value::make_int(42);
    let leaf = lower_nullary_leaf(&[Op::Constant(0), Op::Return], &[c]).unwrap();
    assert_eq!(leaf.call_for_test(&[]), Some(c.bits()));
}

#[test]
fn is_fixnum_const_detects_fixnum_constants_for_guard_elision() {
    // Redundant-guard elimination: a fixnum `iconst` is provably a fixnum, so
    // guard_fixnum elides its runtime guard; a symbol (nil) constant and a
    // computed value are NOT fixnum constants and keep their guards.
    let mut func = Function::with_name_signature(
        UserFuncName::user(0, 0),
        Signature::new(cranelift_codegen::isa::CallConv::SystemV),
    );
    let mut fbctx = FunctionBuilderContext::new();
    let mut fb = FunctionBuilder::new(&mut func, &mut fbctx);
    let block = fb.create_block();
    fb.switch_to_block(block);
    fb.seal_block(block);
    let fixnum = fb
        .ins()
        .iconst(types::I64, Value::make_int(7).bits() as i64);
    let nil = fb.ins().iconst(types::I64, Value::NIL.bits() as i64);
    let sum = fb.ins().iadd(fixnum, fixnum);
    assert!(
        is_fixnum_const(&fb, fixnum),
        "a fixnum iconst is a fixnum constant"
    );
    assert!(
        !is_fixnum_const(&fb, nil),
        "nil (symbol tag) is not a fixnum"
    );
    assert!(
        !is_fixnum_const(&fb, sum),
        "an iadd result is not a constant"
    );

    // is_known_fixnum additionally recognizes a retag_fixnum output (a
    // range-checked arithmetic result), eliding the re-guard on chained
    // arithmetic; a bare untagged iadd is not recognized.
    let shifted = fb.ins().ishl_imm_u(sum, FIXNUM_SHIFT as i64);
    let retagged = fb.ins().bor_imm_u(shifted, FIXNUM_CHECK_VALUE as i64);
    assert!(
        is_known_fixnum(&fb, retagged),
        "retag_fixnum output is a known fixnum"
    );
    assert!(
        is_known_fixnum(&fb, fixnum),
        "a fixnum constant is a known fixnum"
    );
    assert!(
        !is_known_fixnum(&fb, sum),
        "a bare iadd is not a known fixnum"
    );
    assert!(!is_known_fixnum(&fb, nil), "nil is not a known fixnum");
}

fn known_fixnum_at(ops: &[Op], constants: &[Value], leader: usize) -> Option<Vec<bool>> {
    let cfg = analyze_cfg(ops, constants, None, 0).unwrap();
    compute_known_fixnum_slots(ops, constants, &cfg)
        .get(&leader)
        .cloned()
}

#[test]
fn cross_block_known_fixnum_propagates_meets_and_loops() {
    // Forward: a fixnum constant flows across a Goto into its successor block.
    let ops = [Op::Constant(0), Op::Goto(2), Op::Return];
    assert_eq!(
        known_fixnum_at(&ops, &[Value::make_int(7)], 2),
        Some(vec![true]),
        "fixnum constant is known-fixnum across a Goto"
    );
    // A non-fixnum constant is NOT known-fixnum across the edge.
    assert_eq!(
        known_fixnum_at(&ops, &[Value::NIL], 2),
        Some(vec![false]),
        "nil is not a known fixnum across a Goto"
    );

    // Merge narrows: fixnum on the then-path, non-fixnum on the else-path.
    let diamond = [
        Op::Constant(0),  // 0: condition
        Op::GotoIfNil(4), // 1: pop, branch to else(4) or fall to then(2)
        Op::Constant(1),  // 2: then -> fixnum
        Op::Goto(5),      // 3
        Op::Constant(2),  // 4: else -> nil (leader); falls through to 5
        Op::Return,       // 5: merge (leader)
    ];
    let cs = [Value::make_int(0), Value::make_int(9), Value::NIL];
    assert_eq!(
        known_fixnum_at(&diamond, &cs, 5),
        Some(vec![false]),
        "merge of fixnum and non-fixnum is not known-fixnum"
    );

    // THE TARGET: a loop induction variable (i=0; while i<10: i=1+i) is
    // proven fixnum at the loop head across the back-edge (the fixpoint).
    let loop_ops = [
        Op::Constant(0),  // 0: i = 0
        Op::StackRef(0),  // 1: loop head (back-edge target): push i
        Op::Constant(1),  // 2: push limit 10
        Op::Lss,          // 3: i < 10
        Op::GotoIfNil(9), // 4: pop; exit -> 9
        Op::StackRef(0),  // 5: body: push i
        Op::Add1,         // 6: 1+ i
        Op::StackSet(1),  // 7: i = 1+ i
        Op::Goto(1),      // 8: back-edge
        Op::Return,       // 9: exit
    ];
    let lc = [Value::make_int(0), Value::make_int(10)];
    assert_eq!(
        known_fixnum_at(&loop_ops, &lc, 1),
        Some(vec![true]),
        "loop induction variable is known-fixnum at the loop head"
    );
}

#[test]
fn compiles_nil_and_true() {
    assert_eq!(
        lower_nullary_leaf(&[Op::Nil, Op::Return], &[])
            .unwrap()
            .call_for_test(&[]),
        Some(Value::NIL.bits())
    );
    assert_eq!(
        lower_nullary_leaf(&[Op::True, Op::Return], &[])
            .unwrap()
            .call_for_test(&[]),
        Some(Value::T.bits())
    );
}

#[test]
fn dup_and_pop_select_the_right_value() {
    // [Const(0), Const(1), Dup, Pop, Return] -> top is constants[1]
    let a = Value::make_int(7);
    let b = Value::make_int(9);
    let leaf = lower_nullary_leaf(
        &[
            Op::Constant(0),
            Op::Constant(1),
            Op::Dup,
            Op::Pop,
            Op::Return,
        ],
        &[a, b],
    )
    .unwrap();
    assert_eq!(leaf.call_for_test(&[]), Some(b.bits()));
}

#[test]
fn stackref_reaches_below_top() {
    // [Const(0), Const(1), StackRef(1), Return] -> pushes a copy of a, returns a
    let a = Value::make_int(100);
    let b = Value::make_int(200);
    let leaf = lower_nullary_leaf(
        &[
            Op::Constant(0),
            Op::Constant(1),
            Op::StackRef(1),
            Op::Return,
        ],
        &[a, b],
    )
    .unwrap();
    assert_eq!(leaf.call_for_test(&[]), Some(a.bits()));
}

#[test]
fn compiles_fixnum_add() {
    // (+ 40 2) -> 42, all fixnums in range
    let leaf = lower_nullary_leaf(
        &[Op::Constant(0), Op::Constant(1), Op::Add, Op::Return],
        &[Value::make_int(40), Value::make_int(2)],
    )
    .unwrap();
    assert_eq!(leaf.call_for_test(&[]), Some(Value::make_int(42).bits()));
}

#[test]
fn compiles_fixnum_sub_including_negative() {
    // (- 3 10) -> -7
    let leaf = lower_nullary_leaf(
        &[Op::Constant(0), Op::Constant(1), Op::Sub, Op::Return],
        &[Value::make_int(3), Value::make_int(10)],
    )
    .unwrap();
    assert_eq!(leaf.call_for_test(&[]), Some(Value::make_int(-7).bits()));
}

#[test]
fn add_overflowing_fixnum_range_deopts() {
    // MOST_POSITIVE_FIXNUM + 1 leaves fixnum range -> deopt (None), so the
    // interpreter can promote to a bignum.
    let leaf = lower_nullary_leaf(
        &[Op::Constant(0), Op::Constant(1), Op::Add, Op::Return],
        &[
            Value::make_int(Value::MOST_POSITIVE_FIXNUM),
            Value::make_int(1),
        ],
    )
    .unwrap();
    assert_eq!(leaf.call_for_test(&[]), None);
}

#[test]
fn add_non_fixnum_operand_deopts() {
    // a = fixnum 5, b = nil -> not both fixnums -> deopt.
    let leaf = lower_nullary_leaf(
        &[Op::Constant(0), Op::Nil, Op::Add, Op::Return],
        &[Value::make_int(5)],
    )
    .unwrap();
    assert_eq!(leaf.call_for_test(&[]), None);
}

#[test]
fn add_then_sub_chain() {
    // ((1 + 2) - 4) = -1
    let leaf = lower_nullary_leaf(
        &[
            Op::Constant(0),
            Op::Constant(1),
            Op::Add,
            Op::Constant(2),
            Op::Sub,
            Op::Return,
        ],
        &[Value::make_int(1), Value::make_int(2), Value::make_int(4)],
    )
    .unwrap();
    assert_eq!(leaf.call_for_test(&[]), Some(Value::make_int(-1).bits()));
}

#[test]
fn compiles_unary_fixnum_ops() {
    // 1+ 41 -> 42
    let add1 = lower_nullary_leaf(
        &[Op::Constant(0), Op::Add1, Op::Return],
        &[Value::make_int(41)],
    )
    .unwrap();
    assert_eq!(add1.call_for_test(&[]), Some(Value::make_int(42).bits()));

    // 1- 43 -> 42
    let sub1 = lower_nullary_leaf(
        &[Op::Constant(0), Op::Sub1, Op::Return],
        &[Value::make_int(43)],
    )
    .unwrap();
    assert_eq!(sub1.call_for_test(&[]), Some(Value::make_int(42).bits()));

    // - 42 -> -42
    let neg = lower_nullary_leaf(
        &[Op::Constant(0), Op::Negate, Op::Return],
        &[Value::make_int(42)],
    )
    .unwrap();
    assert_eq!(neg.call_for_test(&[]), Some(Value::make_int(-42).bits()));
}

#[test]
fn unary_boundary_inputs_deopt() {
    // 1+ MOST_POSITIVE -> overflow -> deopt
    let add1 = lower_nullary_leaf(
        &[Op::Constant(0), Op::Add1, Op::Return],
        &[Value::make_int(Value::MOST_POSITIVE_FIXNUM)],
    )
    .unwrap();
    assert_eq!(add1.call_for_test(&[]), None);

    // 1- MOST_NEGATIVE -> underflow -> deopt
    let sub1 = lower_nullary_leaf(
        &[Op::Constant(0), Op::Sub1, Op::Return],
        &[Value::make_int(Value::MOST_NEGATIVE_FIXNUM)],
    )
    .unwrap();
    assert_eq!(sub1.call_for_test(&[]), None);

    // - MOST_NEGATIVE -> +MOST_POSITIVE+1 out of range -> deopt
    let neg = lower_nullary_leaf(
        &[Op::Constant(0), Op::Negate, Op::Return],
        &[Value::make_int(Value::MOST_NEGATIVE_FIXNUM)],
    )
    .unwrap();
    assert_eq!(neg.call_for_test(&[]), None);
}

#[test]
fn unary_on_non_fixnum_deopts() {
    // 1+ t -> not a fixnum -> deopt
    let leaf = lower_nullary_leaf(&[Op::True, Op::Add1, Op::Return], &[]).unwrap();
    assert_eq!(leaf.call_for_test(&[]), None);
}

#[test]
fn compiles_fixnum_comparisons() {
    fn cmp(ops: &[Op], a: i64, b: i64) -> Option<usize> {
        lower_nullary_leaf(ops, &[Value::make_int(a), Value::make_int(b)])
            .unwrap()
            .call_for_test(&[])
    }
    let t = Some(Value::T.bits());
    let nil = Some(Value::NIL.bits());
    assert_eq!(
        cmp(
            &[Op::Constant(0), Op::Constant(1), Op::Lss, Op::Return],
            3,
            5
        ),
        t
    );
    assert_eq!(
        cmp(
            &[Op::Constant(0), Op::Constant(1), Op::Lss, Op::Return],
            5,
            3
        ),
        nil
    );
    assert_eq!(
        cmp(
            &[Op::Constant(0), Op::Constant(1), Op::Gtr, Op::Return],
            5,
            3
        ),
        t
    );
    assert_eq!(
        cmp(
            &[Op::Constant(0), Op::Constant(1), Op::Leq, Op::Return],
            4,
            4
        ),
        t
    );
    assert_eq!(
        cmp(
            &[Op::Constant(0), Op::Constant(1), Op::Geq, Op::Return],
            4,
            5
        ),
        nil
    );
    assert_eq!(
        cmp(
            &[Op::Constant(0), Op::Constant(1), Op::Eqlsign, Op::Return],
            7,
            7
        ),
        t
    );
    assert_eq!(
        cmp(
            &[Op::Constant(0), Op::Constant(1), Op::Eqlsign, Op::Return],
            7,
            8
        ),
        nil
    );
}

#[test]
fn comparison_on_non_fixnum_deopts() {
    // (< 1 nil) -> nil isn't a fixnum -> deopt.
    let leaf = lower_nullary_leaf(
        &[Op::Constant(0), Op::Nil, Op::Lss, Op::Return],
        &[Value::make_int(1)],
    )
    .unwrap();
    assert_eq!(leaf.call_for_test(&[]), None);
}

#[test]
fn compiles_if_branch() {
    // (lambda (x) (if x 1 2)):
    //  0 StackRef(0); 1 GotoIfNil(4); 2 Constant(0=>1); 3 Return;
    //  4 Constant(1=>2); 5 Return
    let f = lower_leaf(
        &[
            Op::StackRef(0),
            Op::GotoIfNil(4),
            Op::Constant(0),
            Op::Return,
            Op::Constant(1),
            Op::Return,
        ],
        &[Value::make_int(1), Value::make_int(2)],
        1,
    )
    .unwrap();
    assert_eq!(
        f.call_for_test(&[Value::T]),
        Some(Value::make_int(1).bits())
    );
    assert_eq!(
        f.call_for_test(&[Value::make_int(99)]),
        Some(Value::make_int(1).bits())
    );
    assert_eq!(
        f.call_for_test(&[Value::NIL]),
        Some(Value::make_int(2).bits())
    );
}

#[test]
fn compiles_goto_if_not_nil() {
    // jumps to the second arm when the arg is non-nil.
    let f = lower_leaf(
        &[
            Op::StackRef(0),
            Op::GotoIfNotNil(4),
            Op::Constant(0),
            Op::Return,
            Op::Constant(1),
            Op::Return,
        ],
        &[Value::make_int(1), Value::make_int(2)],
        1,
    )
    .unwrap();
    assert_eq!(
        f.call_for_test(&[Value::NIL]),
        Some(Value::make_int(1).bits())
    );
    assert_eq!(
        f.call_for_test(&[Value::T]),
        Some(Value::make_int(2).bits())
    );
}

#[test]
fn compiles_goto_if_nil_else_pop() {
    // (lambda (x) (and x 7)) shape:
    //  0 StackRef(0); 1 GotoIfNilElsePop(3); 2 Constant(0=>7); 3 Return
    // x nil  -> jump keeping x -> return x (nil);
    // x else -> pop x, push 7 -> return 7.  A join with differing stacks (phi).
    let f = lower_leaf(
        &[
            Op::StackRef(0),
            Op::GotoIfNilElsePop(3),
            Op::Constant(0),
            Op::Return,
        ],
        &[Value::make_int(7)],
        1,
    )
    .unwrap();
    assert_eq!(
        f.call_for_test(&[Value::make_int(5)]),
        Some(Value::make_int(7).bits())
    );
    assert_eq!(f.call_for_test(&[Value::NIL]), Some(Value::NIL.bits()));
}

#[test]
fn compiles_unconditional_goto() {
    //  0 Goto(1); 1 Constant(0=>5); 2 Return
    let f = lower_leaf(
        &[Op::Goto(1), Op::Constant(0), Op::Return],
        &[Value::make_int(5)],
        0,
    )
    .unwrap();
    assert_eq!(f.call_for_test(&[]), Some(Value::make_int(5).bits()));
}

#[test]
fn jit_matches_interpreter_on_if_branch() {
    use crate::emacs_core::bytecode::Vm;
    use crate::emacs_core::eval::Context;
    let ops = [
        Op::StackRef(0),
        Op::GotoIfNil(4),
        Op::Constant(0),
        Op::Return,
        Op::Constant(1),
        Op::Return,
    ];
    let constants = [Value::make_int(10), Value::make_int(20)];
    for arg in [Value::T, Value::NIL, Value::make_int(3)] {
        let mut eval = Context::new_minimal_vm_harness();
        let mut f = ByteCodeFunction::new(LambdaParams {
            required: vec![crate::emacs_core::intern::SymId(1)],
            optional: Vec::new(),
            rest: None,
        });
        f.lexical = true;
        f.ops = ops.to_vec();
        f.constants = constants.to_vec().into();
        f.max_stack = 16;
        let want = {
            let mut vm = Vm::from_context(&mut eval);
            vm.execute(&f, vec![arg]).expect("interp runs if").bits()
        };
        let got = lower_leaf(&ops, &constants, 1)
            .unwrap()
            .call_for_test(&[arg]);
        assert_eq!(
            got,
            Some(want),
            "if-branch mismatch for arg bits {}",
            arg.bits()
        );
        // Also via the typed-MIR Tier-2 path (lower_mir_pure control flow).
        let mir = mir::build_mir(&ops, &constants, 1).expect("MIR builds the if");
        let mleaf = lower_mir_pure(&mir).expect("MIR lowers the if");
        let ctx_ptr = &mut eval as *mut Context as *mut u8;
        let NativeRun::Ok(bits) = mleaf.call(ctx_ptr, &[arg]) else {
            panic!("MIR if must run natively for arg bits {}", arg.bits());
        };
        assert_eq!(
            bits,
            want,
            "MIR if-branch mismatch for arg bits {}",
            arg.bits()
        );
    }
}

#[test]
fn compiles_stackset() {
    // (lambda (a) (setq a (1+ a)) a):
    //  0 StackRef(0); 1 Add1; 2 StackSet(1); 3 StackRef(0); 4 Return
    let f = lower_leaf(
        &[
            Op::StackRef(0),
            Op::Add1,
            Op::StackSet(1),
            Op::StackRef(0),
            Op::Return,
        ],
        &[],
        1,
    )
    .unwrap();
    assert_eq!(
        f.call_for_test(&[Value::make_int(41)]),
        Some(Value::make_int(42).bits())
    );
}

#[test]
fn compiles_discardn() {
    let consts = &[
        Value::make_int(10),
        Value::make_int(20),
        Value::make_int(30),
    ];
    // Non-preserve: push 10,20,30; discard top 2 -> 10.
    let np = lower_nullary_leaf(
        &[
            Op::Constant(0),
            Op::Constant(1),
            Op::Constant(2),
            Op::DiscardN(2),
            Op::Return,
        ],
        consts,
    )
    .unwrap();
    assert_eq!(np.call_for_test(&[]), Some(Value::make_int(10).bits()));
    // Preserve TOS: push 10,20,30; discardN(2 | 0x80) keeps 30 -> 30.
    let pr = lower_nullary_leaf(
        &[
            Op::Constant(0),
            Op::Constant(1),
            Op::Constant(2),
            Op::DiscardN(0x82),
            Op::Return,
        ],
        consts,
    )
    .unwrap();
    assert_eq!(pr.call_for_test(&[]), Some(Value::make_int(30).bits()));
}

#[test]
fn compiles_countdown_loop_matches_interpreter() {
    use crate::emacs_core::bytecode::Vm;
    use crate::emacs_core::eval::Context;
    // (lambda (n) (while (> n 0) (setq n (1- n))) n) -> 0. A back-edge loop:
    //  0 StackRef(0); 1 Constant(0=>0); 2 Gtr; 3 GotoIfNil(8);
    //  4 StackRef(0); 5 Sub1; 6 StackSet(1); 7 Goto(0);
    //  8 StackRef(0); 9 Return
    let ops = [
        Op::StackRef(0),
        Op::Constant(0),
        Op::Gtr,
        Op::GotoIfNil(8),
        Op::StackRef(0),
        Op::Sub1,
        Op::StackSet(1),
        Op::Goto(0),
        Op::StackRef(0),
        Op::Return,
    ];
    let constants = [Value::make_int(0)];
    for n in [0i64, 1, 4, 9] {
        let mut eval = Context::new_minimal_vm_harness();
        let mut f = ByteCodeFunction::new(LambdaParams {
            required: vec![crate::emacs_core::intern::SymId(1)],
            optional: Vec::new(),
            rest: None,
        });
        f.lexical = true;
        f.ops = ops.to_vec();
        f.constants = constants.to_vec().into();
        f.max_stack = 16;
        let want = {
            let mut vm = Vm::from_context(&mut eval);
            vm.execute(&f, vec![Value::make_int(n)])
                .expect("interp loop")
                .bits()
        };
        let got = lower_leaf(&ops, &constants, 1)
            .unwrap()
            .call_for_test(&[Value::make_int(n)]);
        assert_eq!(got, Some(want), "loop mismatch for n={n}");
        assert_eq!(
            got,
            Some(Value::make_int(0).bits()),
            "countdown should reach 0 (n={n})"
        );
        // Also via the typed-MIR Tier-2 path (lower_mir_pure loops/back-edges).
        // Hard failures: a MIR build/lowering regression must not pass silently.
        let mir = mir::build_mir(&ops, &constants, 1).expect("MIR builds the loop");
        let mleaf = lower_mir_pure(&mir).expect("MIR lowers the loop");
        let ctx_ptr = &mut eval as *mut Context as *mut u8;
        let NativeRun::Ok(bits) = mleaf.call(ctx_ptr, &[Value::make_int(n)]) else {
            panic!("MIR loop must run natively for n={n}");
        };
        assert_eq!(bits, want, "MIR loop mismatch for n={n}");
    }
}

#[test]
fn mir_merge_phi_matches_interpreter() {
    use crate::emacs_core::bytecode::Vm;
    use crate::emacs_core::eval::Context;
    // (lambda (c) (1+ (if c 10 20))) — a diamond whose then/else values merge
    // at a common block (a phi), consumed by Add1. Tests build_mir's merge-phi.
    let ops = [
        Op::StackRef(0),  // 0: cond
        Op::GotoIfNil(4), // 1: pop; else->4, fall to then->2
        Op::Constant(0),  // 2: then: 10
        Op::Goto(5),      // 3
        Op::Constant(1),  // 4: else: 20 (leader); falls through to 5
        Op::Add1,         // 5: merge: 1+ phi (leader)
        Op::Return,       // 6
    ];
    let constants = [Value::make_int(10), Value::make_int(20)];
    for c in [Value::T, Value::NIL] {
        let mut eval = Context::new_minimal_vm_harness();
        let mut f = ByteCodeFunction::new(LambdaParams {
            required: vec![crate::emacs_core::intern::SymId(1)],
            optional: Vec::new(),
            rest: None,
        });
        f.lexical = true;
        f.ops = ops.to_vec();
        f.constants = constants.to_vec().into();
        f.max_stack = 16;
        let want = {
            let mut vm = Vm::from_context(&mut eval);
            vm.execute(&f, vec![c]).expect("interp diamond").bits()
        };
        let mir = mir::build_mir(&ops, &constants, 1).expect("MIR builds the diamond");
        let mleaf = lower_mir_pure(&mir).expect("MIR lowers the diamond");
        let ctx_ptr = &mut eval as *mut Context as *mut u8;
        let NativeRun::Ok(bits) = mleaf.call(ctx_ptr, &[c]) else {
            panic!("MIR diamond must run natively for cond bits {}", c.bits());
        };
        assert_eq!(
            bits,
            want,
            "MIR merge-phi mismatch for cond bits {}",
            c.bits()
        );
    }
}

/// The MIR known-fixnum fixpoint, end to end on GNU's byte-compile of
///
///     (defun probe-a (n v)
///       (let ((i 0) (acc 0))
///         (while (< i n) (setq acc (+ acc i)) (setq i (1+ i)))
///         (if v acc acc)))
///
/// `i` and `acc` are loop-carried block params fed a fixnum constant on the
/// entry edge and a `Bin`/`Unary` result on the back edge, so the fixpoint
/// proves them `Fixnum` at every leader and the lowering elides their tag
/// guards; `i`'s untagged twin serves both the `+` and the `1+`. What
/// remains per iteration: ONE tag guard (`n`, an untyped argument) plus the
/// two range guards. Before the port every use re-guarded: 7 guards.
#[test]
fn mir_probe_a_emits_one_tag_guard_per_iteration() {
    use crate::emacs_core::bytecode::Vm;
    use crate::emacs_core::eval::Context;
    let ops = [
        Op::Constant(0),
        Op::Dup,
        Op::StackRef(1),
        Op::StackRef(4),
        Op::Lss,
        Op::GotoIfNil(14),
        Op::Dup,
        Op::StackRef(2),
        Op::Add,
        Op::StackSet(1),
        Op::StackRef(1),
        Op::Add1,
        Op::StackSet(2),
        Op::Goto(2),
        Op::Return,
    ];
    let constants = [Value::make_int(0)];
    let mir = mir::build_mir(&ops, &constants, 2).expect("MIR builds probe-a");
    let mleaf = lower_mir_pure(&mir).expect("MIR lowers probe-a");
    assert_eq!(
        super::lowering::guards_emitted(),
        3,
        "one tag guard (n) + the Add and Add1 range guards; 7 means the block \
         params were not proven fixnum"
    );
    assert_eq!(
        super::lowering::untags_emitted(),
        4,
        "test block: i and n; body: acc and i, the 1+ reusing i's twin; 5 means \
         the twin memo is not hit"
    );
    assert_eq!(
        super::lowering::retags_emitted(),
        4,
        "three hot retags (entry constant and two back-edge results), plus \
         the intermediate accumulator spilled on the cold precise-deopt path"
    );
    for n in [0i64, 1, 7, 1000] {
        let mut eval = Context::new_minimal_vm_harness();
        let mut f = ByteCodeFunction::new(LambdaParams {
            required: vec![
                crate::emacs_core::intern::SymId(1),
                crate::emacs_core::intern::SymId(2),
            ],
            optional: Vec::new(),
            rest: None,
        });
        f.lexical = true;
        f.ops = ops.to_vec();
        f.constants = constants.to_vec().into();
        f.max_stack = 16;
        let args = [Value::make_int(n), Value::T];
        let want = {
            let mut vm = Vm::from_context(&mut eval);
            vm.execute(&f, args.to_vec())
                .expect("interp probe-a")
                .bits()
        };
        let ctx_ptr = &mut eval as *mut Context as *mut u8;
        let NativeRun::Ok(bits) = mleaf.call(ctx_ptr, &args) else {
            panic!("probe-a must run natively for n={n}");
        };
        assert_eq!(bits, want, "probe-a mismatch for n={n}");
        assert_eq!(bits, Value::make_int(n * (n - 1) / 2).bits());
    }
    // The elision is type-directed, so a non-fixnum `n` still deopts on the
    // remaining guard rather than being shifted as a pointer.
    let mut eval = Context::new_minimal_vm_harness();
    let ctx_ptr = &mut eval as *mut Context as *mut u8;
    let NativeRun::DeoptAt(resume) = mleaf.call(ctx_ptr, &[Value::make_float(3.0), Value::NIL])
    else {
        panic!("a float n must deopt precisely at the one remaining guard");
    };
    assert_eq!(resume.pc, 4);
}

/// A value the fixpoint knows is NOT a fixnum keeps its tag guard: the
/// elision is keyed on `Fixnum` alone, not on "immediate" (`Boolean`, `Nil`,
/// `True` and `Symbol` need no GC root but are not numbers). `(1+ (< a b))`
/// must deopt to the interpreter's `wrong-type-argument`, never shift `t`.
#[test]
fn mir_known_boolean_keeps_its_guard() {
    use crate::emacs_core::eval::Context;
    let ops = [
        Op::StackRef(1),
        Op::StackRef(1),
        Op::Lss,
        Op::Add1,
        Op::Return,
    ];
    let mir = mir::build_mir(&ops, &[], 2).expect("MIR builds");
    let mleaf = lower_mir_pure(&mir).expect("MIR lowers");
    assert_eq!(
        super::lowering::guards_emitted(),
        4,
        "a and b tag guards, the Boolean's tag guard, the 1+ range guard"
    );
    let mut eval = Context::new_minimal_vm_harness();
    let ctx_ptr = &mut eval as *mut Context as *mut u8;
    assert!(
        matches!(
            mleaf.call(ctx_ptr, &[Value::make_int(1), Value::make_int(2)]),
            NativeRun::Deopt
        ),
        "(1+ t) must deopt, not shift the tag bits of t"
    );
}

/// The other consumer of the inferred table — the GC-root skip at a call —
/// on a merge param the fixpoint proved `Fixnum`, live across an ALLOCATING
/// residual call under GC stress:
///
///     F = (let ((i (if c 1 2))) (g (sq i)) i)
///     sq = (* x x)   [inlined, so the tier gate takes the MIR leaf]
///     g  = (cons y y) [a residual call: not inlinable, allocates]
///
/// At `(g ..)` the residual stack is `[c i]`: `c` is an untyped argument and
/// stays conditionally rooted; `i` (a non-entry block param, a fixnum on both
/// edges) is skipped by type, and debug builds trap if the skipped value's
/// tag is not the fixnum tag. With every allocation collecting, a wrongly
/// skipped heap value would be freed under the callee. (Loop-free: a loop
/// with a shim site is gated out of the tier until it has a back-edge poll.)
#[test]
fn mir_rooting_skip_on_an_inferred_fixnum_param_across_an_allocating_call() {
    use crate::emacs_core::eval::Context;
    use crate::emacs_core::intern::SymId;
    let mut ev = Context::new();
    ev.gc_stress = true;
    let ctx = &mut ev as *mut Context;
    let mk_sym = |name: &str| {
        let s = Value::symbol(name);
        let crate::emacs_core::value::ValueKind::Symbol(id) = s.kind() else {
            panic!("symbol");
        };
        (s, id)
    };
    let (sq_sym, sq_id) = mk_sym("jit-rs-sq");
    let (g_sym, g_id) = mk_sym("jit-rs-g");
    let mut sq = ByteCodeFunction::new(LambdaParams {
        required: vec![SymId(1)],
        optional: Vec::new(),
        rest: None,
    });
    sq.lexical = true;
    sq.ops = vec![Op::Dup, Op::Mul, Op::Return];
    sq.max_stack = 16;
    ev.obarray
        .set_symbol_function_id(sq_id, Value::make_bytecode(sq));
    let mut g = ByteCodeFunction::new(LambdaParams {
        required: vec![SymId(1)],
        optional: Vec::new(),
        rest: None,
    });
    g.lexical = true;
    // (lambda (y) (garbage-collect) (make-list 4096 0)): an exact collection,
    // then a reallocation that reuses any freed cell.
    g.ops = vec![
        Op::Constant(0),
        Op::Call(0),
        Op::Pop,
        Op::Constant(1),
        Op::Constant(2),
        Op::Constant(3),
        Op::Call(2),
        Op::Return,
    ];
    g.constants = vec![
        Value::symbol("garbage-collect"),
        Value::symbol("make-list"),
        Value::make_int(4096),
        Value::make_int(0),
    ]
    .into();
    g.max_stack = 16;
    ev.obarray
        .set_symbol_function_id(g_id, Value::make_bytecode(g));
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: vec![SymId(3)],
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = vec![
        Op::StackRef(0),  // 0: c               [c c]
        Op::GotoIfNil(4), // 1                  [c]
        Op::Constant(0),  // 2: 1               [c 1]
        Op::Goto(5),      // 3
        Op::Constant(1),  // 4: 2               [c 2]
        Op::Constant(2),  // 5: g  (merge: i)   [c i g]
        Op::Constant(3),  // 6: sq              [c i g sq]
        Op::StackRef(2),  // 7: i               [c i g sq i]
        Op::Call(1),      // 8: (sq i)          [c i g r]
        Op::Call(1),      // 9: (g r) residual = [c i]: c heap, i Fixnum
        Op::Pop,          // 10                 [c i]
        Op::StackRef(1),  // 11: c              [c i c]
        Op::StackRef(1),  // 12: i              [c i c i]
        Op::Cons,         // 13: (c . i)        [c i p]
        Op::Return,       // 14
    ];
    f.constants = vec![Value::make_int(1), Value::make_int(2), g_sym, sq_sym].into();
    f.max_stack = 16;
    let mir = mir::build_mir(&f.ops, &f.constants, 1).expect("MIR builds F");
    let merge = mir
        .blocks
        .iter()
        .find(|b| b.bytecode_pc == 5)
        .expect("merge");
    assert_eq!(
        mir::infer_value_types(&mir)[merge.params[1].0 as usize],
        mir::LispType::Fixnum,
        "i is a fixnum on both edges"
    );
    // The tier gate sends a body with a residual generic call to the baseline,
    // so drive the MIR lowering directly: inline sq as the production path
    // would, then lower what is left.
    let mut mir = mir;
    let sq_mir = mir::build_mir(&[Op::Dup, Op::Mul, Op::Return], &[], 1).expect("sq builds");
    let n = mir::inline_pure_single_block_callees(
        &mut mir,
        &|_, v| (v.bits() == sq_sym.bits()).then(|| sq_mir.clone()),
        8,
        &mut Vec::new(),
    );
    assert_eq!(n, 1, "sq inlines");
    let merge = mir
        .blocks
        .iter()
        .find(|b| b.bytecode_pc == 5)
        .expect("merge");
    assert_eq!(
        mir::infer_value_types(&mir)[merge.params[1].0 as usize],
        mir::LispType::Fixnum,
        "i is still a fixnum after inlining"
    );
    let leaf = lower_mir_pure(&mir).expect("F lowers in the MIR tier");
    assert_eq!(leaf.tier, super::leaf::LeafTier::Mir);
    assert!(leaf.has_side_effects, "F keeps the residual call to g");
    for _ in 0..3 {
        // c non-nil: a fresh heap list that only the residual root keeps alive
        // across g's exact collection.
        let c = ev.eval_str("(list 7 8 9)").expect("c");
        let NativeRun::Ok(bits) = leaf.call(ctx as *mut u8, &[c]) else {
            panic!("F(list) must run natively");
        };
        let p = Value::from_bits(bits);
        assert!(p.is_cons());
        let c2 = p.cons_car();
        assert!(c2.is_cons(), "c survived the collection (got {c2:?})");
        assert_eq!(c2.cons_car(), Value::make_int(7), "c's contents intact");
        assert_eq!(
            p.cons_cdr(),
            Value::make_int(1),
            "i = 1 on the non-nil path"
        );
    }
    let NativeRun::Ok(bits) = leaf.call(ctx as *mut u8, &[Value::NIL]) else {
        panic!("F(nil) must run natively");
    };
    let p = Value::from_bits(bits);
    assert!(p.cons_car().is_nil() && p.cons_cdr() == Value::make_int(2));
}

#[test]
fn mir_multi_phi_merge_matches_interpreter() {
    use crate::emacs_core::bytecode::Vm;
    use crate::emacs_core::eval::Context;
    // A diamond where BOTH branches leave TWO values on the stack, so the
    // merge needs TWO phis; then Sub consumes them. Compares MIR to the
    // interpreter (ground truth) — no manual expected value.
    let ops = [
        Op::StackRef(0),  // 0: cond, depth 2
        Op::GotoIfNil(5), // 1: pop; else->5, fall->2  (depth 1)
        Op::Constant(0),  // 2: then: 10  (depth 2)
        Op::Constant(1),  // 3:        20 (depth 3)
        Op::Goto(7),      // 4: -> merge(7)
        Op::Constant(1),  // 5: else: 20 (depth 2) [leader]
        Op::Constant(0),  // 6:        10 (depth 3)  falls to 7
        Op::Sub,          // 7: merge: two phis -> Sub (depth 2) [leader]
        Op::Return,       // 8
    ];
    let constants = [Value::make_int(10), Value::make_int(20)];
    for c in [Value::T, Value::NIL] {
        let mut eval = Context::new_minimal_vm_harness();
        let mut f = ByteCodeFunction::new(LambdaParams {
            required: vec![crate::emacs_core::intern::SymId(1)],
            optional: Vec::new(),
            rest: None,
        });
        f.lexical = true;
        f.ops = ops.to_vec();
        f.constants = constants.to_vec().into();
        f.max_stack = 16;
        let want = {
            let mut vm = Vm::from_context(&mut eval);
            vm.execute(&f, vec![c]).expect("interp multi-phi").bits()
        };
        let mir = mir::build_mir(&ops, &constants, 1).expect("MIR builds the two-phi diamond");
        let mleaf = lower_mir_pure(&mir).expect("MIR lowers the two-phi diamond");
        let ctx_ptr = &mut eval as *mut Context as *mut u8;
        let NativeRun::Ok(bits) = mleaf.call(ctx_ptr, &[c]) else {
            panic!(
                "MIR two-phi diamond must run natively for cond bits {}",
                c.bits()
            );
        };
        assert_eq!(
            bits,
            want,
            "MIR multi-phi-merge mismatch for cond bits {}",
            c.bits()
        );
    }
}

#[test]
fn inline_pure_callee_lowers_and_runs() {
    // Caller (lambda (a) (sq a)) with sq = (lambda (x) (* x x)). build_mir(caller)
    // has an Opaque{Call} (so lower_mir_pure alone would bail); inlining sq's
    // pure body turns the caller into (* a a) — a pure MIR lower_mir_pure
    // handles — proving cross-call-boundary inlining + unboxing.
    let sq_sym = Value::symbol("jit-inline-sq");
    let sq_ops = [Op::Dup, Op::Mul, Op::Return];
    let caller_ops = [Op::Constant(0), Op::StackRef(1), Op::Call(1), Op::Return];
    let caller_consts = [sq_sym];
    let mut m = mir::build_mir(&caller_ops, &caller_consts, 1).expect("caller MIR builds");
    let n = mir::inline_pure_single_block_callees(
        &mut m,
        &|_, v| {
            (v.bits() == sq_sym.bits()).then(|| mir::build_mir(&sq_ops, &[], 1).expect("sq builds"))
        },
        16,
        &mut Vec::new(),
    );
    assert_eq!(n, 1, "sq must be inlined (the call replaced by its body)");
    let leaf = lower_mir_pure(&m).expect("inlined (now pure) MIR lowers");
    for a in [3i64, 7, -4, 0] {
        let arg = Value::make_int(a);
        match leaf.call(std::ptr::null_mut(), &[arg]) {
            NativeRun::Ok(bits) => assert_eq!(
                bits,
                Value::make_int(a * a).bits(),
                "inlined (* a a) for a={a}"
            ),
            NativeRun::Deopt | NativeRun::DeoptAt(_) => {}
            other => panic!("a={a}: unexpected {other:?}"),
        }
    }
}

#[test]
fn inlined_callee_redefinition_rejits() {
    use crate::emacs_core::eval::Context;
    use crate::emacs_core::intern::SymId;
    // C = (lambda (x) (* x x)); F = (lambda (a) (C a)). Compiling F inlines C
    // -> (* a a). Redefine C = (1+ x) + bump the function epoch (as fset would):
    // F's cache entry is now stale -> re-JIT -> F computes the NEW C, (1+ a).
    // If the inline-epoch invalidation were broken, the stale inline would
    // return 25 instead of 6. Verifies the redefinition soundness.
    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context;
    let c_sym = Value::symbol("jit-inline-redef-c");
    let crate::emacs_core::value::ValueKind::Symbol(c_id) = c_sym.kind() else {
        panic!("symbol");
    };
    let mk = |ops: Vec<Op>| {
        let mut c = ByteCodeFunction::new(LambdaParams {
            required: vec![SymId(1)],
            optional: Vec::new(),
            rest: None,
        });
        c.lexical = true;
        c.ops = ops;
        c.max_stack = 16;
        Value::make_bytecode(c)
    };
    ev.obarray
        .set_symbol_function_id(c_id, mk(vec![Op::Dup, Op::Mul, Op::Return]));
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: vec![SymId(2)],
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = vec![Op::Constant(0), Op::StackRef(1), Op::Call(1), Op::Return];
    f.constants = vec![c_sym].into();
    f.max_stack = 16;
    let f_val = Value::make_bytecode(f.clone());
    let r1 = crate::emacs_core::jit::try_run_compiled(ctx, &f, f_val, &[Value::make_int(5)]);
    assert!(
        matches!(r1, Ok(Some(b)) if b == Value::make_int(25).bits()),
        "inlined (* 5 5) should be 25"
    );
    // Redefine C and bump the epoch (fset/defalias bump function_epoch).
    ev.obarray
        .set_symbol_function_id(c_id, mk(vec![Op::Add1, Op::Return]));
    ev.obarray
        .bump_function_epoch(crate::emacs_core::symbol::FunctionEpochBump::SubrRewrite);
    let r2 = crate::emacs_core::jit::try_run_compiled(ctx, &f, f_val, &[Value::make_int(5)]);
    assert!(
        matches!(r2, Ok(Some(b)) if b == Value::make_int(6).bits()),
        "after redefinition + epoch bump, re-JIT inlines the new C: (1+ 5) = 6"
    );
}

#[test]
fn mir_call_lowering_runs_a_non_inlined_call() {
    use crate::emacs_core::eval::Context;
    use crate::emacs_core::intern::SymId;
    // C = identity; F = (1+ (C a)). The MIR has a NON-inlined Opaque{Call(C)}
    // plus a post-call 1+ guard. lower_mir_pure now lowers the call (generic
    // shim, vmctx-threaded) and routes the 1+ guard to PRECISE deopt. Verify
    // F(5) = 1+(id 5) = 6 end-to-end with a REAL Context (the precise-deopt
    // resume path is exercised by the NEOVM_JIT_FORCE_DEOPT gate).
    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context;
    let c_sym = Value::symbol("jit-mir-call-c");
    let crate::emacs_core::value::ValueKind::Symbol(c_id) = c_sym.kind() else {
        panic!("symbol");
    };
    let mut c = ByteCodeFunction::new(LambdaParams {
        required: vec![SymId(1)],
        optional: Vec::new(),
        rest: None,
    });
    c.lexical = true;
    c.ops = vec![Op::StackRef(0), Op::Return];
    c.max_stack = 16;
    ev.obarray
        .set_symbol_function_id(c_id, Value::make_bytecode(c));
    let f_ops = [
        Op::Constant(0),
        Op::StackRef(1),
        Op::Call(1),
        Op::Add1,
        Op::Return,
    ];
    let f_consts = [c_sym];
    let m = mir::build_mir(&f_ops, &f_consts, 1).expect("F builds");
    let leaf = lower_mir_pure(&m).expect("F lowers (non-inlined call + precise deopt)");
    assert!(
        leaf.has_side_effects,
        "a call-bearing MIR leaf is side-effecting (must never rerun-from-start)"
    );
    match leaf.call(ctx as *mut u8, &[Value::make_int(5)]) {
        NativeRun::Ok(bits) => {
            assert_eq!(bits, Value::make_int(6).bits(), "1+(id 5) = 6")
        }
        other => panic!("F(5): expected Ok(6), got {other:?}"),
    }
}

#[test]
fn inline_plus_residual_call_keeps_shared_speculation_in_mir() {
    use crate::emacs_core::eval::Context;
    use crate::emacs_core::intern::SymId;
    // F = (g (sq a)): sq inlines, while the multi-block g remains a call.
    // MIR must keep the baseline's speculated call rather than degrade to the
    // generic shim. Its inline entry guard still protects the spliced sq.
    // The baseline's profitability gate (calls > arithmetic) would refuse F
    // outright; this test is about the tier choice, so switch it off.
    crate::emacs_core::jit::compile::force_profit_gate_for_test(false);
    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context;
    let mk_sym = |name: &str| {
        let s = Value::symbol(name);
        let crate::emacs_core::value::ValueKind::Symbol(id) = s.kind() else {
            panic!("symbol");
        };
        (s, id)
    };
    let (sq_sym, sq_id) = mk_sym("jit-ir-sq");
    let (g_sym, g_id) = mk_sym("jit-ir-g");
    let mut sq = ByteCodeFunction::new(LambdaParams {
        required: vec![SymId(1)],
        optional: Vec::new(),
        rest: None,
    });
    sq.lexical = true;
    sq.ops = vec![Op::Dup, Op::Mul, Op::Return];
    sq.max_stack = 16;
    ev.obarray
        .set_symbol_function_id(sq_id, Value::make_bytecode(sq));
    let mut g = ByteCodeFunction::new(LambdaParams {
        required: vec![SymId(1)],
        optional: Vec::new(),
        rest: None,
    });
    g.lexical = true;
    // (if y (1+ y) 0) — two basic blocks, so callee_inlinable refuses it.
    g.ops = vec![
        Op::StackRef(0),
        Op::GotoIfNil(5),
        Op::StackRef(0),
        Op::Add1,
        Op::Return,
        Op::Constant(0),
        Op::Return,
    ];
    g.constants = vec![Value::make_int(0)].into();
    g.max_stack = 16;
    ev.obarray
        .set_symbol_function_id(g_id, Value::make_bytecode(g));
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: vec![SymId(3)],
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = vec![
        Op::Constant(0), // g
        Op::Constant(1), // sq
        Op::StackRef(2), // a
        Op::Call(1),     // (sq a)
        Op::Call(1),     // (g (sq a))
        Op::Return,
    ];
    f.constants = vec![g_sym, sq_sym].into();
    f.max_stack = 16;
    let leaf = compile_bytecode_function_with(&f, Some(&ev.obarray)).expect("F compiles");
    assert_eq!(
        leaf.tier,
        super::leaf::LeafTier::Mir,
        "a residual specialized call now stays in MIR"
    );
    assert!(leaf.inline_epoch().is_some(), "the caller still inlines sq");
    assert_eq!(leaf.spec_slots.len(), 1);
    let _ = ev.debug_on_next_call_is_armed();
    // F(3) = g(sq(3)) = g(9) = 1+9 = 10.
    match leaf.call(ctx as *mut u8, &[Value::make_int(3)]) {
        NativeRun::Ok(bits) => {
            assert_eq!(bits, Value::make_int(10).bits(), "g(sq(3)) = 1+9 = 10")
        }
        other => panic!("F(3): expected Ok(10), got {other:?}"),
    }
}

#[test]
fn precise_eviction_only_evicts_inlined_dependents() {
    use crate::emacs_core::eval::Context;
    use crate::emacs_core::intern::SymId;
    // F = (C a) inlines C = (* x x), so INLINE_DEPS records C -> {F}. Redefining
    // an UNRELATED symbol D must NOT evict F (precision: no churn). Redefining C
    // DOES evict F (so it re-JITs against the new C) and clears the dep entry.
    // The coarse inline_epoch backstop would re-JIT regardless; this asserts the
    // PRECISE eviction (only dependents are evicted, eagerly).
    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context;
    let mk_sym = |name: &str| {
        let s = Value::symbol(name);
        let crate::emacs_core::value::ValueKind::Symbol(id) = s.kind() else {
            panic!("symbol");
        };
        (s, id)
    };
    let mk_fn = |ops: Vec<Op>, consts: Vec<Value>| {
        let mut bf = ByteCodeFunction::new(LambdaParams {
            required: vec![SymId(1)],
            optional: Vec::new(),
            rest: None,
        });
        bf.lexical = true;
        bf.ops = ops;
        bf.constants = consts.into();
        bf.max_stack = 16;
        bf
    };
    let (c_sym, c_id) = mk_sym("jit-pe-c");
    let (_d_sym, d_id) = mk_sym("jit-pe-d");
    ev.obarray.set_symbol_function_id(
        c_id,
        Value::make_bytecode(mk_fn(vec![Op::Dup, Op::Mul, Op::Return], vec![])),
    );
    ev.obarray.set_symbol_function_id(
        d_id,
        Value::make_bytecode(mk_fn(vec![Op::Add1, Op::Return], vec![])),
    );
    let f = mk_fn(
        vec![Op::Constant(0), Op::StackRef(1), Op::Call(1), Op::Return],
        vec![c_sym],
    );
    let f_val = Value::make_bytecode(f.clone());
    // Compile F (inlines C).
    let _ = crate::emacs_core::jit::try_run_compiled(ctx, &f, f_val, &[Value::make_int(4)]);
    let f_id = f.jit_runtime().compiled_id_or_assign();
    assert!(
        crate::emacs_core::jit::cache::is_compiled_for_test(f_id),
        "F is JIT-cached after compile"
    );
    assert_eq!(
        crate::emacs_core::jit::cache::inline_dependent_count_for_test(c_id),
        1,
        "F recorded as inlining C"
    );
    // Redefine an UNRELATED symbol D -> F must NOT be evicted (precision).
    ev.obarray.set_symbol_function_id(
        d_id,
        Value::make_bytecode(mk_fn(vec![Op::Sub1, Op::Return], vec![])),
    );
    assert!(
        crate::emacs_core::jit::cache::is_compiled_for_test(f_id),
        "unrelated redefinition (D) must NOT evict F"
    );
    // Redefine the inlined callee C -> F evicted + dep entry cleared.
    ev.obarray.set_symbol_function_id(
        c_id,
        Value::make_bytecode(mk_fn(vec![Op::Add1, Op::Return], vec![])),
    );
    assert!(
        !crate::emacs_core::jit::cache::is_compiled_for_test(f_id),
        "redefining the inlined callee C evicts F (precise)"
    );
    assert_eq!(
        crate::emacs_core::jit::cache::inline_dependent_count_for_test(c_id),
        0,
        "C's dep entry cleared on eviction"
    );
}

#[test]
fn mir_scalar_replaces_non_escaping_cons() {
    // F = (car (cons a b)) -> a, with the cons ELIDED (escape analysis, pure
    // body -> MIR tier, zero allocation). Previously the cons bailed the whole
    // body to the baseline. Verify it lowers (no bail) and returns the car.
    let ops = [
        Op::StackRef(1),
        Op::StackRef(1),
        Op::Cons,
        Op::Car,
        Op::Return,
    ];
    let m = mir::build_mir(&ops, &[], 2).expect("builds");
    let leaf = lower_mir_pure(&m).expect("scalar-replaced cons lowers (no bail)");
    assert!(
        !leaf.has_side_effects,
        "a pure scalar-replaced body has no side effects (no allocation/call)"
    );
    match leaf.call_for_test(&[Value::make_int(3), Value::make_int(5)]) {
        Some(bits) => assert_eq!(bits, Value::make_int(3).bits(), "(car (cons 3 5)) = 3"),
        None => panic!("expected Some(3) — no deopt"),
    }
}

#[test]
fn mir_allocates_escaping_cons() {
    use crate::emacs_core::eval::Context;
    // F = (cons a b), returned -> the cons ESCAPES -> heap-allocated in the MIR
    // tier via neovm_jit_cons (previously this body bailed to the baseline).
    // Verify it lowers (no bail) + runs to a cons; the contents (3 . 5) are
    // covered by the differential gate. Real Context (the allocation runs).
    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context;
    let ops = [Op::StackRef(1), Op::StackRef(1), Op::Cons, Op::Return];
    let m = mir::build_mir(&ops, &[], 2).expect("builds");
    let leaf = lower_mir_pure(&m).expect("escaping cons lowers (no bail)");
    assert!(
        !leaf.has_side_effects,
        "a cons allocation is a GC safepoint, not a side effect (no precise deopt)"
    );
    match leaf.call(ctx as *mut u8, &[Value::make_int(3), Value::make_int(5)]) {
        NativeRun::Ok(bits) => assert!(
            matches!(
                Value::from_bits(bits).kind(),
                crate::emacs_core::value::ValueKind::Cons
            ),
            "(cons 3 5) allocates a cons"
        ),
        other => panic!("F(3,5): expected Ok(cons), got {other:?}"),
    }
}

/// The poll is the TIER's to make, not the baseline's alone: a loop body the
/// MIR tier claims must be as interruptible as the same body on the
/// baseline. `compile_bytecode_function_with` is the production entry that
/// picks the tier.
#[test]
fn a_compiled_loop_polls_quit_whichever_tier_compiles_it() {
    use crate::emacs_core::eval::Context;
    // `(while (> n 0) (setq n (1- n)))`: no shim-lowered op at all, so the
    // MIR tier's `has_adapter_site` is false and its gate used to admit it.
    let ops = vec![
        Op::StackRef(0),
        Op::Constant(0),
        Op::Gtr,
        Op::GotoIfNil(8),
        Op::StackRef(0),
        Op::Sub1,
        Op::StackSet(1),
        Op::Goto(0),
        Op::StackRef(0),
        Op::Return,
    ];
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: vec![crate::emacs_core::intern::SymId(1)],
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = ops;
    f.constants = vec![Value::make_int(0)].into();
    f.max_stack = 16;
    let mut ev = Context::new();
    let ctx_ptr = &mut ev as *mut Context as *mut u8;
    let leaf = compile_bytecode_function_with(&f, Some(&ev.obarray)).expect("compiles");
    assert_eq!(
        leaf.tier,
        super::leaf::LeafTier::Mir,
        "the optimizing tier must run the loop and preserve its service polls"
    );
    assert_eq!(
        leaf.call(ctx_ptr, &[Value::make_int(1000)]),
        NativeRun::Ok(Value::make_int(0).bits()),
        "the loop runs to completion with no quit pending"
    );
    ev.set_quit_flag_value(Value::T);
    let quit = leaf.call(ctx_ptr, &[Value::make_int(1000)]);
    ev.set_quit_flag_value(Value::NIL);
    assert_eq!(
        quit,
        NativeRun::Signal,
        "C-g must interrupt a compiled loop whichever tier compiled it"
    );
    assert!(take_pending_flow().is_some(), "quit Flow stashed");
    // The same for a loop that CONSES: its only shim is `neovm_jit_cons`,
    // which never reaches a safe point, so without a back-edge poll the loop
    // would allocate forever with no GC.
    f.ops = vec![
        Op::StackRef(0),
        Op::Constant(0),
        Op::Gtr,
        Op::GotoIfNil(12),
        Op::StackRef(0),
        Op::Constant(0),
        Op::Cons,
        Op::Pop,
        Op::StackRef(0),
        Op::Sub1,
        Op::StackSet(1),
        Op::Goto(0),
        Op::StackRef(0),
        Op::Return,
    ];
    let consing = compile_bytecode_function_with(&f, Some(&ev.obarray)).expect("compiles");
    assert_eq!(
        consing.call(ctx_ptr, &[Value::make_int(1000)]),
        NativeRun::Ok(Value::make_int(0).bits())
    );
    ev.set_quit_flag_value(Value::T);
    let quit = consing.call(ctx_ptr, &[Value::make_int(1000)]);
    ev.set_quit_flag_value(Value::NIL);
    assert_eq!(
        quit,
        NativeRun::Signal,
        "a consing loop must reach its safe point too"
    );
    assert!(take_pending_flow().is_some(), "quit Flow stashed");
}

/// Conditional branches must poll only when their backward edge is taken,
/// and ElsePop must carry the condition in that edge's root/argument stack.
#[test]
fn mir_conditional_loops_preserve_poll_cadence_and_quit() {
    use crate::emacs_core::eval::{
        Context, bytecode_branch_poll_count, reset_bytecode_branch_poll_count,
    };
    use crate::emacs_core::intern::SymId;

    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context as *mut u8;
    for (on_nil, else_pop) in [(false, false), (true, false), (false, true), (true, true)] {
        let mut f = ByteCodeFunction::new(LambdaParams {
            required: vec![SymId(1)],
            optional: Vec::new(),
            rest: None,
        });
        f.lexical = true;
        f.constants = vec![Value::make_int(0)].into();
        f.max_stack = 4;
        if else_pop {
            f.ops.extend([Op::Nil, Op::Pop]);
        }
        let target = if else_pop { 1 } else { 0 };
        f.ops.extend([
            Op::StackRef(0),
            Op::Sub1,
            Op::StackSet(1),
            Op::StackRef(0),
            Op::Constant(0),
            if on_nil { Op::Leq } else { Op::Gtr },
            match (on_nil, else_pop) {
                (false, false) => Op::GotoIfNotNil(target),
                (true, false) => Op::GotoIfNil(target),
                (false, true) => Op::GotoIfNotNilElsePop(target),
                (true, true) => Op::GotoIfNilElsePop(target),
            },
            Op::Return,
        ]);
        let leaf = compile_bytecode_function_with(&f, Some(&ev.obarray)).expect("compiles");
        assert_eq!(leaf.tier, super::leaf::LeafTier::Mir);
        for n in [1, 254, 255, 256, 511] {
            reset_bytecode_branch_poll_count();
            assert_eq!(
                leaf.call(ctx, &[Value::make_int(n)]),
                NativeRun::Ok(Value::make_int(0).bits()),
                "on_nil={on_nil} else_pop={else_pop} n={n}"
            );
            assert_eq!(bytecode_branch_poll_count(), ((n - 1) / 255) as usize);
        }
        ev.set_quit_flag_value(Value::T);
        let quit = leaf.call(ctx, &[Value::make_int(256)]);
        ev.set_quit_flag_value(Value::NIL);
        assert_eq!(quit, NativeRun::Signal);
        assert!(take_pending_flow().is_some());
        assert_eq!(ev.jit_root_stack_top, 0, "poll must restore its roots");
    }
}

/// The growing list exists only in native registers until a back-edge poll.
/// Exact GC must see the edge's live stack and preserve every earlier cons.
#[test]
fn mir_loop_keeps_native_list_alive_across_exact_gc() {
    use crate::emacs_core::eval::Context;
    use crate::emacs_core::intern::SymId;

    let mut ev = Context::new();
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: vec![SymId(1)],
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.constants = vec![Value::make_int(0)].into();
    f.max_stack = 4;
    // (lambda (n) (let (l) (while (> n 0) (setq l (cons n l) n (1- n))) l))
    f.ops = vec![
        Op::Nil,
        Op::StackRef(1),
        Op::Constant(0),
        Op::Gtr,
        Op::GotoIfNil(13),
        Op::StackRef(1),
        Op::StackRef(1),
        Op::Cons,
        Op::StackSet(1),
        Op::StackRef(1),
        Op::Sub1,
        Op::StackSet(2),
        Op::Goto(1),
        Op::Return,
    ];
    let leaf = compile_bytecode_function_with(&f, Some(&ev.obarray)).expect("compiles");
    assert_eq!(leaf.tier, super::leaf::LeafTier::Mir);
    let before = ev.tagged_heap.gc_collections();
    ev.gc_stress = true;
    let result = leaf.call(&mut ev as *mut Context as *mut u8, &[Value::make_int(1024)]);
    ev.gc_stress = false;
    let NativeRun::Ok(bits) = result else {
        panic!("list-building loop failed: {result:?}");
    };
    assert!(ev.tagged_heap.gc_collections() >= before + 4);
    assert_eq!(ev.jit_root_stack_top, 0);
    let mut list = Value::from_bits(bits);
    for n in 1..=1024 {
        assert!(list.is_cons(), "missing list cell {n}");
        assert_eq!(list.cons_car(), Value::make_int(n));
        list = list.cons_cdr();
    }
    assert!(list.is_nil());
}

/// After a poll, rerunning from entry would replay work across post-gc-hook.
/// An overflow must retain the loop's progress in its interpreter framestate.
#[test]
fn mir_loop_overflow_after_a_poll_deopts_at_the_current_iteration() {
    use crate::emacs_core::eval::{
        Context, bytecode_branch_poll_count, reset_bytecode_branch_poll_count,
    };
    use crate::emacs_core::intern::SymId;

    let mut ev = Context::new();
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: vec![SymId(1), SymId(2)],
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.constants = vec![Value::make_int(0)].into();
    f.max_stack = 4;
    f.ops = vec![
        Op::StackRef(1),
        Op::Constant(0),
        Op::Gtr,
        Op::GotoIfNil(11),
        Op::StackRef(0),
        Op::Add1,
        Op::StackSet(1),
        Op::StackRef(1),
        Op::Sub1,
        Op::StackSet(2),
        Op::Goto(0),
        Op::Return,
    ];
    let leaf = compile_bytecode_function_with(&f, Some(&ev.obarray)).expect("compiles");
    assert_eq!(leaf.tier, super::leaf::LeafTier::Mir);
    let max = (1_i64 << 61) - 1;
    reset_bytecode_branch_poll_count();
    let result = leaf.call(
        &mut ev as *mut Context as *mut u8,
        &[Value::make_int(512), Value::make_int(max - 300)],
    );
    assert_eq!(bytecode_branch_poll_count(), 1);
    let NativeRun::DeoptAt(resume) = result else {
        panic!("loop must deopt precisely after its poll: {result:?}");
    };
    assert_eq!(resume.pc, 5);
    assert_eq!(
        resume.stack,
        vec![
            Value::make_int(212),
            Value::make_int(max),
            Value::make_int(max)
        ]
    );
    assert_eq!(ev.jit_root_stack_top, 0);
}

/// A poll can run Lisp that redefines a callee. The next inlined call must
/// deopt to its original boundary, preserving iterations already completed.
#[test]
fn mir_loop_does_not_keep_an_inlined_callee_across_a_gc_hook() {
    use crate::emacs_core::eval::Context;
    use crate::emacs_core::intern::SymId;

    let mut ev = Context::new();
    let step = Value::symbol("mir-loop-step");
    let mut callee = ByteCodeFunction::new(LambdaParams {
        required: vec![SymId(1)],
        optional: Vec::new(),
        rest: None,
    });
    callee.lexical = true;
    callee.max_stack = 2;
    callee.ops = vec![Op::StackRef(0), Op::Sub1, Op::Return];
    ev.obarray
        .set_symbol_function_id(step.as_symbol_id().unwrap(), Value::make_bytecode(callee));
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: vec![SymId(1)],
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.max_stack = 3;
    f.constants = vec![Value::make_int(0), step].into();
    f.ops = vec![
        Op::StackRef(0),
        Op::Constant(0),
        Op::Gtr,
        Op::GotoIfNil(9),
        Op::Constant(1),
        Op::StackRef(1),
        Op::Call(1),
        Op::StackSet(1),
        Op::Goto(0),
        Op::Return,
    ];
    f.seal_hand_assembled_ops();
    let _ = ev.debug_on_next_call_is_armed();
    let leaf = compile_bytecode_function_with(&f, Some(&ev.obarray)).expect("compiles");
    assert_eq!(leaf.tier, super::leaf::LeafTier::Mir);
    assert_eq!(
        leaf.call(&mut ev as *mut Context as *mut u8, &[Value::make_int(1000)]),
        NativeRun::Ok(Value::make_int(0).bits())
    );
    ev.eval_str("(setq post-gc-hook (list (lambda () (fset 'mir-loop-step (lambda (n) -7)))))")
        .expect("install redefining GC hook");
    ev.gc_stress = true;
    let result = leaf.call(&mut ev as *mut Context as *mut u8, &[Value::make_int(1000)]);
    ev.gc_stress = false;
    let NativeRun::DeoptAt(frame) = &result else {
        panic!("{result:?}")
    };
    assert_eq!(frame.pc, 6);
    assert!(
        frame.stack[0].as_fixnum().unwrap() < 1000,
        "completed iterations are retained"
    );
    assert_eq!(
        mir_inline_guards::resume(&mut ev, &f, result),
        Value::make_int(-7)
    );
    assert_eq!(ev.jit_root_stack_top, 0);
}

#[test]
fn backedge_polls_quit_like_the_interpreter() {
    use crate::emacs_core::bytecode::Vm;
    use crate::emacs_core::eval::Context;
    // Countdown loop with enough iterations (> 255 backward jumps) for the
    // u8 quit counter to wrap and trigger the back-edge service poll.
    let ops = [
        Op::StackRef(0),
        Op::Constant(0),
        Op::Gtr,
        Op::GotoIfNil(8),
        Op::StackRef(0),
        Op::Sub1,
        Op::StackSet(1),
        Op::Goto(0),
        Op::StackRef(0),
        Op::Return,
    ];
    let constants = [Value::make_int(0)];
    let mut ev = Context::new_minimal_vm_harness();
    let ctx_ptr = &mut ev as *mut Context as *mut u8;
    let leaf = lower_leaf(&ops, &constants, 1).unwrap();

    // Flag clear: the loop runs to completion natively (polls return OK).
    assert_eq!(
        leaf.call(ctx_ptr, &[Value::make_int(1000)]),
        NativeRun::Ok(Value::make_int(0).bits())
    );

    // Flag set: the wrap poll must signal quit out of native code...
    ev.set_quit_flag_value(Value::T);
    assert_eq!(
        leaf.call(ctx_ptr, &[Value::make_int(1000)]),
        NativeRun::Signal,
        "C-g must interrupt a compiled loop"
    );
    assert!(take_pending_flow().is_some(), "quit Flow stashed");

    // ...exactly like the interpreter on the same body (the poll clears the
    // flag, so re-set it for the oracle run).
    ev.set_quit_flag_value(Value::T);
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: vec![crate::emacs_core::intern::SymId(1)],
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = ops.to_vec();
    f.constants = constants.to_vec().into();
    f.max_stack = 16;
    let interp = {
        let mut vm = Vm::from_context(&mut ev);
        vm.execute(&f, vec![Value::make_int(1000)])
    };
    assert!(interp.is_err(), "interpreter quits on the same loop");

    // Flag cleared by the quit: the loop completes again.
    assert_eq!(
        leaf.call(ctx_ptr, &[Value::make_int(1000)]),
        NativeRun::Ok(Value::make_int(0).bits())
    );
}

#[test]
fn compiles_save_excursion_with_unwind_semantics() {
    use crate::emacs_core::eval::Context;
    let mut ev = Context::new();
    let ctx_ptr = &mut ev as *mut Context as *mut u8;
    ev.eval_str(r#"(insert "hello world")"#).expect("insert");
    let specpdl_before = ev.specpdl.len();
    let constants = [
        Value::symbol("goto-char"),
        Value::make_int(1),
        Value::symbol("point"),
    ];

    // Balanced: (save-excursion (goto-char 1)) then (point) — restored.
    let balanced = lower_nullary_leaf(
        &[
            Op::SaveExcursion,
            Op::Constant(0),
            Op::Constant(1),
            Op::Call(1),
            Op::Pop,
            Op::Unbind(1),
            Op::Constant(2),
            Op::Call(0),
            Op::Return,
        ],
        &constants,
    )
    .unwrap();
    assert_eq!(
        balanced.call(ctx_ptr, &[]),
        NativeRun::Ok(Value::make_int(12).bits()),
        "point must be restored by the Unbind"
    );
    assert_eq!(ev.specpdl.len(), specpdl_before);

    // Early return with the record dangling: the frame unwind restores it.
    let dangling = lower_nullary_leaf(
        &[
            Op::SaveExcursion,
            Op::Constant(0),
            Op::Constant(1),
            Op::Call(1),
            Op::Return,
        ],
        &constants,
    )
    .unwrap();
    assert_eq!(
        dangling.call(ctx_ptr, &[]),
        NativeRun::Ok(Value::make_int(1).bits())
    );
    assert_eq!(ev.specpdl.len(), specpdl_before, "frame unwind pops record");
    let point_now =
        lower_nullary_leaf(&[Op::Constant(2), Op::Call(0), Op::Return], &constants).unwrap();
    assert_eq!(
        point_now.call(ctx_ptr, &[]),
        NativeRun::Ok(Value::make_int(12).bits()),
        "point must be restored by the frame unwind too"
    );

    // SaveCurrentBuffer / SaveRestriction: records create + frame-unwind
    // cleanly (same shim/record machinery; arms mirrored 1:1).
    for op in [Op::SaveCurrentBuffer, Op::SaveRestriction] {
        let mech = lower_nullary_leaf(&[op, Op::Nil, Op::Return], &[]).unwrap();
        assert_eq!(mech.call(ctx_ptr, &[]), NativeRun::Ok(Value::NIL.bits()));
        assert_eq!(ev.specpdl.len(), specpdl_before);
    }

    // Precise deopt: a guard after the Save* record compiles and runs
    // (a failing guard would resume the interpreter mid-frame with the
    // record still registered).
    let after = lower_nullary_leaf(
        &[Op::SaveExcursion, Op::Constant(1), Op::Add1, Op::Return],
        &constants,
    )
    .expect("guard after a side effect compiles under precise deopt");
    match after.call(ctx_ptr, &[]) {
        NativeRun::Ok(_) => {}
        other => panic!("guard-after-save must run, got {other:?}"),
    }
    assert_eq!(ev.specpdl.len(), specpdl_before);
}

#[test]
fn compiles_trivial_natives_carsafe_maxmin_throw_numpreds() {
    let mut ev = crate::emacs_core::eval::Context::new_minimal_vm_harness();
    let ctx_ptr = &mut ev as *mut crate::emacs_core::eval::Context as *mut u8;
    let t = NativeRun::Ok(Value::T.bits());
    let nil = NativeRun::Ok(Value::NIL.bits());
    let run1 = |op: Op, v: Value, ctx: *mut u8| {
        lower_nullary_leaf(&[Op::Constant(0), op, Op::Return], &[v])
            .unwrap()
            .call(ctx, &[])
    };

    // car-safe / cdr-safe: total — non-cons (incl. fixnums) -> nil, no deopt.
    let cons = Value::cons(Value::make_int(3), Value::make_int(4));
    assert_eq!(
        run1(Op::CarSafe, cons, ctx_ptr),
        NativeRun::Ok(Value::make_int(3).bits())
    );
    assert_eq!(
        run1(Op::CdrSafe, cons, ctx_ptr),
        NativeRun::Ok(Value::make_int(4).bits())
    );
    assert_eq!(run1(Op::CarSafe, Value::make_int(9), ctx_ptr), nil);
    assert_eq!(run1(Op::CdrSafe, Value::T, ctx_ptr), nil);
    assert_eq!(run1(Op::CarSafe, Value::NIL, ctx_ptr), nil);

    // max / min: fixnum fast path keeps the original tagged operand;
    // non-fixnum deopts to the interpreter's coercing builtin.
    let run2 = |op: Op, a: Value, b: Value, ctx: *mut u8| {
        lower_nullary_leaf(&[Op::Constant(0), Op::Constant(1), op, Op::Return], &[a, b])
            .unwrap()
            .call(ctx, &[])
    };
    assert_eq!(
        run2(Op::Max, Value::make_int(3), Value::make_int(7), ctx_ptr),
        NativeRun::Ok(Value::make_int(7).bits())
    );
    assert_eq!(
        run2(Op::Max, Value::make_int(-3), Value::make_int(-7), ctx_ptr),
        NativeRun::Ok(Value::make_int(-3).bits())
    );
    assert_eq!(
        run2(Op::Min, Value::make_int(3), Value::make_int(7), ctx_ptr),
        NativeRun::Ok(Value::make_int(3).bits())
    );
    // Non-fixnum operand: precise deopt at the Max op with the operands
    // still on the captured stack.
    match run2(Op::Max, Value::make_float(1.5), Value::make_int(7), ctx_ptr) {
        NativeRun::DeoptAt(resume) => {
            let DeoptResume { pc, ref stack, .. } = *resume;
            assert_eq!(pc, 2, "deopt at the Max op");
            assert_eq!(stack[1], Value::make_int(7));
        }
        other => panic!("expected a precise deopt, got {other:?}"),
    }

    // integerp / numberp: fixnum natively; float/bignum via the slow shim.
    assert_eq!(run1(Op::Integerp, Value::make_int(5), ctx_ptr), t);
    assert_eq!(run1(Op::Integerp, Value::make_float(1.5), ctx_ptr), nil);
    assert_eq!(run1(Op::Integerp, Value::T, ctx_ptr), nil);
    assert_eq!(run1(Op::Numberp, Value::make_int(5), ctx_ptr), t);
    assert_eq!(run1(Op::Numberp, Value::make_float(1.5), ctx_ptr), t);
    assert_eq!(run1(Op::Numberp, Value::NIL, ctx_ptr), nil);

    // throw: stashes Flow::Throw and exits via the signal path.
    let tag = Value::symbol("jit-throw-tag");
    let thrown = lower_nullary_leaf(
        &[Op::Constant(0), Op::Constant(1), Op::Throw],
        &[tag, Value::make_int(42)],
    )
    .unwrap();
    assert_eq!(thrown.call(ctx_ptr, &[]), NativeRun::Signal);
    match take_pending_flow().expect("throw Flow stashed") {
        Flow::Throw(thrown) => {
            assert_eq!(thrown.tag, tag);
            assert_eq!(thrown.value, Value::make_int(42));
        }
        other => panic!("expected Flow::Throw, got {other:?}"),
    }
}

#[test]
fn compiles_direct_builtin_ops() {
    let mut ev = crate::emacs_core::eval::Context::new_minimal_vm_harness();
    let ctx_ptr = &mut ev as *mut crate::emacs_core::eval::Context as *mut u8;
    let ok_int = |n: i64| NativeRun::Ok(Value::make_int(n).bits());
    let run = |ops: &[Op], consts: &[Value], ctx: *mut u8| {
        lower_nullary_leaf(ops, consts).unwrap().call(ctx, &[])
    };

    // length
    let list = Value::cons(
        Value::make_int(1),
        Value::cons(
            Value::make_int(2),
            Value::cons(Value::make_int(3), Value::NIL),
        ),
    );
    assert_eq!(
        run(&[Op::Constant(0), Op::Length, Op::Return], &[list], ctx_ptr),
        ok_int(3)
    );

    // nth: (nth 1 '(1 2 3)) = 2 — operand order matches the arm (n, list).
    assert_eq!(
        run(
            &[Op::Constant(0), Op::Constant(1), Op::Nth, Op::Return],
            &[Value::make_int(1), list],
            ctx_ptr
        ),
        ok_int(2)
    );

    // memq: (memq 'b '(a b c)) -> the tail whose car is 'b.
    let (a, bsym, c) = (
        Value::symbol("jit-memq-a"),
        Value::symbol("jit-memq-b"),
        Value::symbol("jit-memq-c"),
    );
    let abc = Value::cons(a, Value::cons(bsym, Value::cons(c, Value::NIL)));
    let NativeRun::Ok(tail) = run(
        &[Op::Constant(0), Op::Constant(1), Op::Memq, Op::Return],
        &[bsym, abc],
        ctx_ptr,
    ) else {
        panic!("memq must succeed");
    };
    assert_eq!(Value::from_bits(tail).cons_car(), bsym);

    // equal on structurally-equal fresh lists -> t.
    let l1 = Value::cons(
        Value::make_int(1),
        Value::cons(Value::make_int(2), Value::NIL),
    );
    let l2 = Value::cons(
        Value::make_int(1),
        Value::cons(Value::make_int(2), Value::NIL),
    );
    assert_eq!(
        run(
            &[Op::Constant(0), Op::Constant(1), Op::Equal, Op::Return],
            &[l1, l2],
            ctx_ptr
        ),
        NativeRun::Ok(Value::T.bits())
    );

    // setcar mutates through the SATB-barriered builtin; result = new car.
    let cell = Value::cons(Value::make_int(10), Value::make_int(20));
    assert_eq!(
        run(
            &[Op::Constant(0), Op::Constant(1), Op::Setcar, Op::Return],
            &[cell, Value::make_int(99)],
            ctx_ptr
        ),
        ok_int(99)
    );
    assert_eq!(cell.cons_car(), Value::make_int(99), "mutation visible");

    // Precise deopt: a guard after the mutation compiles and runs —
    // (1+ (setcar cell 1)) = 2 with the mutation visible.
    assert_eq!(
        run(
            &[
                Op::Constant(0),
                Op::Constant(1),
                Op::Setcar,
                Op::Add1,
                Op::Return,
            ],
            &[cell, Value::make_int(1)],
            ctx_ptr
        ),
        ok_int(2)
    );
    assert_eq!(cell.cons_car(), Value::make_int(1), "mutation visible");

    // symbol-value: live read + void-variable signal.
    let var = Value::symbol("jit-bw-var");
    let crate::emacs_core::value::ValueKind::Symbol(var_id) = var.kind() else {
        panic!("symbol expected");
    };
    ev.obarray.set_symbol_value_id(var_id, Value::make_int(5));
    assert_eq!(
        run(
            &[Op::Constant(0), Op::SymbolValue, Op::Return],
            &[var],
            ctx_ptr
        ),
        ok_int(5)
    );
    let unbound = Value::symbol("jit-bw-unbound");
    assert_eq!(
        run(
            &[Op::Constant(0), Op::SymbolValue, Op::Return],
            &[unbound],
            ctx_ptr
        ),
        NativeRun::Signal
    );
    assert!(take_pending_flow().is_some());

    // put / get round-trip on a plist.
    let psym = Value::symbol("jit-bw-plist");
    let prop = Value::symbol("jit-bw-prop");
    assert_eq!(
        run(
            &[
                Op::Constant(0),
                Op::Constant(1),
                Op::Constant(2),
                Op::Put,
                Op::Return,
            ],
            &[psym, prop, Value::make_int(7)],
            ctx_ptr
        ),
        ok_int(7)
    );
    assert_eq!(
        run(
            &[Op::Constant(0), Op::Constant(1), Op::Get, Op::Return],
            &[psym, prop],
            ctx_ptr
        ),
        ok_int(7)
    );

    // aref on a string; string-equal.
    let s = Value::string("abc");
    assert_eq!(
        run(
            &[Op::Constant(0), Op::Constant(1), Op::Aref, Op::Return],
            &[s, Value::make_int(1)],
            ctx_ptr
        ),
        ok_int('b' as i64)
    );
    let s2 = Value::string("abc");
    assert_eq!(
        run(
            &[
                Op::Constant(0),
                Op::Constant(1),
                Op::StringEqual,
                Op::Return
            ],
            &[s, s2],
            ctx_ptr
        ),
        NativeRun::Ok(Value::T.bits())
    );
}

/// The bit-op intrinsics (`logand`/`logior`/`logxor`/`ash`/`lognot`)
/// JIT-compiled: the armed fast shim computes the native op == the interpreter
/// (including negative two's-complement fixnums and `ash` shifts), a non-fixnum
/// arg deopts to the generic fallback (same wrong-type signal), and an `ash`
/// LEFT-shift that overflows fixnum range deopts to the generic bignum.
#[test]
fn arith_intrinsic_bitops_jit_match_interp_and_deopt() {
    use crate::emacs_core::eval::Context;
    // This test targets the armed SHIM path (asserts SUBR_SPEC_FAST_COUNT); pin
    // Level-B inline OFF so and/or/xor/lognot go through the shim deterministically
    // regardless of NEOVM_JIT_INLINE_ARITH (the inline path is covered separately).
    force_inline_arith_for_test(false);
    let mut ev = Context::new(); // binds logand/logior/logxor/ash/lognot
    let ctx = &mut ev as *mut Context as *mut u8;
    // An N-arg body `(OP p0 [p1])`: Constant(OP); StackRef(nargs)*nargs; Call(nargs).
    let mk = |op_name: &str, nargs: usize, ob: &crate::emacs_core::symbol::Obarray| {
        let mut ops = vec![Op::Constant(0)];
        for _ in 0..nargs {
            ops.push(Op::StackRef(nargs as u16));
        }
        ops.push(Op::Call(nargs as u16));
        ops.push(Op::Return);
        let mut f = ByteCodeFunction::new(LambdaParams {
            required: (0..nargs).map(|i| SymId(1 + i as u32)).collect(),
            optional: Vec::new(),
            rest: None,
        });
        f.lexical = true;
        f.ops = ops;
        f.constants = vec![Value::symbol(op_name)].into();
        f.max_stack = 16;
        compile_bytecode_function_with(&f, Some(ob)).expect("bit-op body compiles")
    };
    let int = |n: i64| Value::make_int(n);
    // 2-arg ops with a fixnum result.
    for (name, a, b, want) in [
        ("logand", 12, 10, 8),
        ("logior", 12, 10, 14),
        ("logxor", 12, 10, 6),
        ("logand", -1, 5, 5),  // two's-complement: -1 is all-ones
        ("logior", -8, 3, -5), // sign bit survives
        ("logxor", -1, -1, 0),
        ("ash", 3, 4, 48),    // left shift: 3 << 4
        ("ash", 5, 0, 5),     // no shift
        ("ash", 256, -3, 32), // right shift: 256 >> 3
        ("ash", -7, -1, -4),  // arithmetic right shift: floor(-3.5) = -4
    ] {
        let leaf = mk(name, 2, &ev.obarray);
        #[cfg(debug_assertions)]
        let fast0 = SUBR_SPEC_FAST_COUNT.load(Ordering::Relaxed);
        let r = leaf.call(ctx, &[int(a), int(b)]);
        assert_eq!(
            r,
            NativeRun::Ok(int(want).bits()),
            "({name} {a} {b}) = {want}"
        );
        #[cfg(debug_assertions)]
        assert!(
            SUBR_SPEC_FAST_COUNT.load(Ordering::Relaxed) > fast0,
            "({name} {a} {b}): armed fast shim must fire"
        );
    }
    // lognot (1-arg): !n of a fixnum is always a fixnum.
    for (a, want) in [(5i64, -6i64), (-1, 0), (0, -1)] {
        let leaf = mk("lognot", 1, &ev.obarray);
        #[cfg(debug_assertions)]
        let fast0 = SUBR_SPEC_FAST_COUNT.load(Ordering::Relaxed);
        assert_eq!(
            leaf.call(ctx, &[int(a)]),
            NativeRun::Ok(int(want).bits()),
            "(lognot {a}) = {want}"
        );
        #[cfg(debug_assertions)]
        assert!(
            SUBR_SPEC_FAST_COUNT.load(Ordering::Relaxed) > fast0,
            "(lognot {a}): armed fast shim must fire"
        );
    }
    // ash LEFT-shift overflowing fixnum range -> NEED_GENERIC -> generic makes
    // the bignum 2^100; result must equal the interpreter's (a bignum, != any
    // fixnum), taking the generic bounce not the fast path.
    {
        let leaf = mk("ash", 2, &ev.obarray);
        #[cfg(debug_assertions)]
        let gen0 = SUBR_SPEC_GENERIC_COUNT.load(Ordering::Relaxed);
        let NativeRun::Ok(bits) = leaf.call(ctx, &[int(1), int(100)]) else {
            panic!("(ash 1 100) must return Ok (a bignum via generic)");
        };
        let got = Value::from_bits(bits);
        let interp = ev.eval_str("(ash 1 100)").expect("interp ash");
        assert!(
            crate::emacs_core::value::equal_value(&got, &interp, 0),
            "(ash 1 100): JIT {got:?} != interp {interp:?} (both should be 2^100)"
        );
        assert!(got.as_bignum().is_some(), "(ash 1 100) is a bignum");
        #[cfg(debug_assertions)]
        assert!(
            SUBR_SPEC_GENERIC_COUNT.load(Ordering::Relaxed) > gen0,
            "(ash 1 100): overflow took the NEED_GENERIC bounce"
        );
    }
    // Non-fixnum arg (a cons): as_fixnum → None → STATUS_NEED_GENERIC → the
    // generic fallback runs the real logand, which signals wrong-type — the
    // SAME as the interpreter. Proves the deopt path is wired, GC-safe.
    let leaf = mk("logand", 2, &ev.obarray);
    let cons = Value::cons(int(1), Value::NIL);
    #[cfg(debug_assertions)]
    let gen0 = SUBR_SPEC_GENERIC_COUNT.load(Ordering::Relaxed);
    assert_eq!(
        leaf.call(ctx, &[cons, int(5)]),
        NativeRun::Signal,
        "(logand '(1) 5) signals wrong-type via the generic fallback"
    );
    assert!(take_pending_flow().is_some(), "the signal was stashed");
    #[cfg(debug_assertions)]
    assert!(
        SUBR_SPEC_GENERIC_COUNT.load(Ordering::Relaxed) > gen0,
        "the non-fixnum arg took the NEED_GENERIC bounce"
    );
}

/// LEVEL-B: with inline-arith forced on, logand/logior/logxor/lognot compile
/// to inline native ops (== interpreter, incl. negatives); the leaf records an
/// inline_epoch (redefinition eviction); `ash` stays on the shim (no
/// inline_epoch); and a non-fixnum arg DEOPTS (never wrongly computes inline).
#[test]
fn arith_intrinsic_inline_level_b_matches_interp() {
    use crate::emacs_core::eval::Context;
    force_inline_arith_for_test(true);
    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context as *mut u8;
    let mk = |op_name: &str, nargs: usize, ob: &crate::emacs_core::symbol::Obarray| {
        let mut ops = vec![Op::Constant(0)];
        for _ in 0..nargs {
            ops.push(Op::StackRef(nargs as u16));
        }
        ops.push(Op::Call(nargs as u16));
        ops.push(Op::Return);
        let mut f = ByteCodeFunction::new(LambdaParams {
            required: (0..nargs).map(|i| SymId(1 + i as u32)).collect(),
            optional: Vec::new(),
            rest: None,
        });
        f.lexical = true;
        f.ops = ops;
        f.constants = vec![Value::symbol(op_name)].into();
        f.max_stack = 16;
        compile_bytecode_function_with(&f, Some(ob)).expect("inline bit-op body compiles")
    };
    let int = |n: i64| Value::make_int(n);
    for (name, nargs, args, want) in [
        ("logand", 2usize, vec![12i64, 10], 8i64),
        ("logior", 2, vec![12, 10], 14),
        ("logxor", 2, vec![12, 10], 6),
        ("logand", 2, vec![-1, 5], 5),
        ("logior", 2, vec![-8, 3], -5),
        ("logxor", 2, vec![-1, -1], 0),
        ("logxor", 2, vec![-1, 0], -1), // tag-restore on a negative result
        ("lognot", 1, vec![5], -6),
        ("lognot", 1, vec![-1], 0),
        ("lognot", 1, vec![0], -1),
        (
            "lognot",
            1,
            vec![Value::MOST_NEGATIVE_FIXNUM],
            Value::MOST_POSITIVE_FIXNUM,
        ),
    ] {
        let leaf = mk(name, nargs, &ev.obarray);
        assert!(
            !leaf.inline_deps().is_empty(),
            "{name} inline leaf must register a redefinition-eviction dep"
        );
        let argv: Vec<Value> = args.iter().map(|&n| int(n)).collect();
        assert_eq!(
            leaf.call(ctx, &argv),
            NativeRun::Ok(int(want).bits()),
            "({name} {args:?}) inline = {want}"
        );
    }
    // ash is NOT inlined: stays on the self-arming shim, so no inline dep.
    let ash_leaf = mk("ash", 2, &ev.obarray);
    assert!(
        ash_leaf.inline_deps().is_empty(),
        "ash stays on the shim — no inline redefinition dep"
    );
    assert_eq!(
        ash_leaf.call(ctx, &[int(3), int(4)]),
        NativeRun::Ok(int(48).bits())
    );
    // A non-fixnum arg on an inline op DEOPTS (guard fails) — it never runs the
    // inline `&` on a non-fixnum; the caller re-runs the real logand interpreted.
    let leaf = mk("logand", 2, &ev.obarray);
    let cons = Value::cons(int(1), Value::NIL);
    assert!(
        matches!(
            leaf.call(ctx, &[cons, int(5)]),
            NativeRun::Deopt | NativeRun::DeoptAt(_)
        ),
        "(logand '(1) 5) must deopt, not compute inline"
    );
    force_inline_arith_for_test(false);
}

/// OSR compile path: a counting loop `(while (< i 5) (setq i (1+ i)))`
/// compiled with an ALTERNATE ENTRY at the loop-header pc. Called with a
/// synthetic operand stack (the live `i`), it must resume the loop mid-flight
/// and return the same result as running from the start — for i seeded at 0
/// (full loop), 3 (partial), 5 (exit immediately), and 10 (past the bound).
#[test]
fn osr_entry_resumes_loop_from_seeded_stack() {
    use crate::emacs_core::eval::Context;
    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context as *mut u8;
    // pcs:  0 Constant0(=0)   -- i = 0  (prologue, UNREACHABLE under OSR)
    //       1 StackRef0       -- loop header / OSR entry, entry_depth = 1
    //       2 Constant1(=5)
    //       3 Lss             -- i < 5
    //       4 GotoIfNil(9)
    //       5 StackRef0
    //       6 Add1            -- i + 1
    //       7 StackSet1       -- i = i+1
    //       8 Goto(1)         -- backward branch (loop)
    //       9 Return          -- return i
    let ops = vec![
        Op::Constant(0),
        Op::StackRef(0),
        Op::Constant(1),
        Op::Lss,
        Op::GotoIfNil(9),
        Op::StackRef(0),
        Op::Add1,
        Op::StackSet(1),
        Op::Goto(1),
        Op::Return,
    ];
    let constants = vec![Value::make_int(0), Value::make_int(5)];
    const OSR_PC: usize = 1;
    let leaf = lower_leaf_full_osr(
        &ops,
        &constants,
        0,
        None,
        Some(&ev.obarray),
        Some(OSR_PC),
        0,
    )
    .expect("OSR variant compiles (alternate loop-header entry)");
    // Seed the operand stack = [i]; the OSR entry resumes the loop from `i`.
    for (seed, want) in [(0i64, 5i64), (3, 5), (5, 5), (10, 10)] {
        let args = [Value::make_int(seed).bits() as i64];
        match leaf.call_premarshaled(ctx, args.as_ptr()) {
            NativeRun::Ok(bits) => assert_eq!(
                Value::from_bits(bits),
                Value::make_int(want),
                "OSR resume from i={seed} must return {want}"
            ),
            other => panic!("OSR run from i={seed}: expected Ok({want}), got {other:?}"),
        }
    }
}

/// OSR end-to-end: a once-called summation loop `(let ((acc 0)(i 0)) (while
/// (< i n) (setq acc (+ acc i)) (setq i (1+ i))) acc)` run through the
/// INTERPRETER with OSR forced on + the function pinned hot — the hot back-edge
/// transfers into native code mid-loop and finishes there. The result must
/// equal the pure interpreter (OSR off), for n large enough to wrap the
/// back-edge counter (256) and trigger the transfer.
#[test]
fn osr_transfers_hot_loop_and_matches_interpreter() {
    use crate::emacs_core::bytecode::vm::Vm;
    use crate::emacs_core::eval::Context;
    // sum(0..n-1) loop; see osr_entry_resumes_loop_from_seeded_stack for the shape.
    let mk = || {
        let mut f = ByteCodeFunction::new(LambdaParams {
            required: vec![SymId(1)], // n
            optional: Vec::new(),
            rest: None,
        });
        f.lexical = true;
        f.ops = vec![
            Op::Constant(0), // acc = 0
            Op::Constant(0), // i = 0
            Op::StackRef(0), // 2: L_header (OSR entry) — i
            Op::StackRef(3), // n
            Op::Lss,         // i < n
            Op::GotoIfNil(14),
            Op::StackRef(1), // acc
            Op::StackRef(1), // i
            Op::Add,         // acc + i
            Op::StackSet(2), // acc = acc + i
            Op::StackRef(0), // i
            Op::Add1,        // i + 1
            Op::StackSet(1), // i = i + 1
            Op::Goto(2),     // back to L_header
            Op::StackRef(1), // 14: L_end — acc
            Op::Return,
        ];
        f.constants = vec![Value::make_int(0)].into();
        f.max_stack = 16;
        f.seal_hand_assembled_ops();
        f
    };
    let n = 2000i64;
    let want = Value::make_int(n * (n - 1) / 2); // sum 0..n-1

    // OSR OFF: pure interpreter baseline.
    let mut ev = Context::new();
    crate::emacs_core::jit::force_osr_for_test(false);
    let f_off = mk();
    let off = Vm::from_context(&mut ev)
        .execute(&f_off, vec![Value::make_int(n)])
        .expect("interp run");
    assert_eq!(off, want, "interpreter sum(0..{}) baseline", n - 1);

    // OSR ON + pinned hot: the hot back-edge transfers into native mid-loop.
    crate::emacs_core::jit::force_osr_for_test(true);
    let f_on = mk();
    f_on.jit_runtime().set_hot_for_test();
    let before = crate::emacs_core::jit::cache::OSR_TRANSFER_COUNT.load(Ordering::Relaxed);
    let on = Vm::from_context(&mut ev)
        .execute(&f_on, vec![Value::make_int(n)])
        .expect("OSR run");
    assert_eq!(on, want, "OSR sum(0..{}) must match the interpreter", n - 1);
    assert!(
        crate::emacs_core::jit::cache::OSR_TRANSFER_COUNT.load(Ordering::Relaxed) > before,
        "the OSR transfer must actually fire (not the interpreter finishing the loop)"
    );
    crate::emacs_core::jit::force_osr_for_test(false);
}

/// A loop that OSR enters and that then deoptimizes at a precise pc must run
/// each iteration exactly once. The native loop below increments a special
/// variable (a side effect) while a local counts up from 1,500 below
/// `most-positive-fixnum`, so its `1+` leaves the fixnum range mid-loop and
/// the native code deopts with its state captured. The transfer used to drop
/// that state and interpret on from the pre-transfer stack, replaying every
/// iteration the native code had run: the special ended above N. The
/// resumed frame must also not transfer again (see the transfer count).
#[test]
fn osr_precise_deopt_resumes_without_replaying_iterations() {
    use crate::emacs_core::bytecode::vm::Vm;
    use crate::emacs_core::eval::Context;
    // (lambda (start n)
    //   (let ((i 0) (x start))
    //     (while (< i n)
    //       (setq osr-deopt-counter (1+ osr-deopt-counter))
    //       (setq i (1+ i))
    //       (setq x (1+ x)))
    //     i))
    let mk = || {
        let mut f = ByteCodeFunction::new(LambdaParams {
            required: vec![SymId(1), SymId(2)], // start n
            optional: Vec::new(),
            rest: None,
        });
        f.lexical = true;
        f.ops = vec![
            Op::Constant(0), // i = 0            [start n i]
            Op::StackRef(2), // x = start        [start n i x]
            Op::StackRef(1), // 2: L_header (OSR entry) -- i
            Op::StackRef(3), // n
            Op::Lss,         // i < n
            Op::GotoIfNil(16),
            Op::VarRef(1), // osr-deopt-counter
            Op::Add1,
            Op::VarSet(1),   // the side effect
            Op::StackRef(1), // i
            Op::Add1,
            Op::StackSet(2), // i = i + 1
            Op::StackRef(0), // x
            Op::Add1,        // leaves the fixnum range mid-loop
            Op::StackSet(1), // x = x + 1
            Op::Goto(2),
            Op::StackRef(1), // 16: L_end -- i
            Op::Return,
        ];
        f.constants = vec![Value::make_int(0), Value::symbol("osr-deopt-counter")].into();
        f.max_stack = 16;
        f.seal_hand_assembled_ops();
        f
    };
    let n = 2000i64;
    let start = Value::make_int(Value::MOST_POSITIVE_FIXNUM - 1500);
    let counter = |ev: &mut Context| ev.eval_str("osr-deopt-counter").expect("counter");

    // OSR OFF: the interpreter baseline.
    let mut ev = Context::new();
    ev.eval_str("(setq osr-deopt-counter 0)").unwrap();
    crate::emacs_core::jit::force_osr_for_test(false);
    let off = Vm::from_context(&mut ev)
        .execute(&mk(), vec![start, Value::make_int(n)])
        .expect("interp run");
    assert_eq!(off, Value::make_int(n));
    assert_eq!(counter(&mut ev), Value::make_int(n), "interpreter baseline");

    // OSR ON + pinned hot: transfer mid-loop, deopt at the overflow.
    ev.eval_str("(setq osr-deopt-counter 0)").unwrap();
    crate::emacs_core::jit::force_osr_for_test(true);
    let f_on = mk();
    f_on.jit_runtime().set_hot_for_test();
    let before = crate::emacs_core::jit::cache::OSR_TRANSFER_COUNT.load(Ordering::Relaxed);
    let on = Vm::from_context(&mut ev)
        .execute(&f_on, vec![start, Value::make_int(n)])
        .expect("OSR run");
    let transfers =
        crate::emacs_core::jit::cache::OSR_TRANSFER_COUNT.load(Ordering::Relaxed) - before;
    let count = counter(&mut ev);
    crate::emacs_core::jit::force_osr_for_test(false);
    // One transfer: the frame resumed from the deopt starts latched, so the
    // 500 iterations left (a bignum now, which would deopt again) do not
    // transfer and nest another resumed frame at the next hot back-edge.
    assert_eq!(transfers, 1, "exactly one OSR transfer");
    assert_eq!(on, Value::make_int(n), "the loop's own count");
    assert_eq!(
        count,
        Value::make_int(n),
        "each iteration's side effect runs exactly once"
    );
}

#[test]
fn compiles_unwind_protect_pop() {
    use crate::emacs_core::eval::Context;
    let mut ev = Context::new();
    let ctx_ptr = &mut ev as *mut Context as *mut u8;
    // NOTE: the opcode's operand is a LIST of cleanup forms (sf_progn_value),
    // exactly what the byte-compiler pushes for (unwind-protect BODY FORMS..).
    let cleanup = ev
        .eval_str("'((setq jit-up-ran t))")
        .expect("cleanup forms");
    // The cleanup form list lives in a Rust local across the next eval
    // and the native calls below; root it or a stress-GC frees it.
    ev.push_specpdl_root(cleanup);
    ev.eval_str("(setq jit-up-ran nil)").expect("flag init");
    let specpdl_before = ev.specpdl.len();
    let consts = [
        cleanup,
        Value::make_int(7),
        Value::symbol("jit-up-no-such-fn"),
    ];

    // Balanced: the matching Unbind runs the cleanup.
    let balanced = lower_nullary_leaf(
        &[
            Op::Constant(0),
            Op::UnwindProtectPop,
            Op::Constant(1),
            Op::Unbind(1),
            Op::Return,
        ],
        &consts,
    )
    .unwrap();
    assert_eq!(
        balanced.call(ctx_ptr, &[]),
        NativeRun::Ok(Value::make_int(7).bits())
    );
    assert_eq!(
        ev.eval_str("jit-up-ran").unwrap(),
        Value::T,
        "cleanup ran on the balanced path"
    );
    assert_eq!(ev.specpdl.len(), specpdl_before);

    // Signal inside the protected extent: the frame unwind runs the cleanup.
    ev.eval_str("(setq jit-up-ran nil)").expect("flag reset");
    let signaled = lower_nullary_leaf(
        &[
            Op::Constant(0),
            Op::UnwindProtectPop,
            Op::Constant(2),
            Op::Call(0),
            Op::Return,
        ],
        &consts,
    )
    .unwrap();
    assert_eq!(signaled.call(ctx_ptr, &[]), NativeRun::Signal);
    assert!(take_pending_flow().is_some());
    assert_eq!(
        ev.eval_str("jit-up-ran").unwrap(),
        Value::T,
        "cleanup ran on the signal path"
    );
    assert_eq!(ev.specpdl.len(), specpdl_before);
}

/// MIR Tier-2 Phase 4b: a pure body lowered bytecode→MIR→CLIF produces the
/// SAME native result as the interpreter — the first end-to-end proof of the
/// MIR pipeline.
#[test]
fn mir_pure_lowering_matches_interpreter() {
    use crate::emacs_core::bytecode::ByteCodeFunction;
    use crate::emacs_core::value::LambdaParams;

    // (ops, constants, arity, args).
    type Case = (Vec<Op>, Vec<Value>, usize, Vec<Value>);
    let cases: Vec<Case> = vec![
        // (lambda (a b) (+ a b)) on (40, 2) -> 42.
        (
            vec![Op::StackRef(1), Op::StackRef(1), Op::Add, Op::Return],
            vec![],
            2,
            vec![Value::make_int(40), Value::make_int(2)],
        ),
        // (lambda (n) (if (< n 2) n (1- n))) — branch + arithmetic.
        (
            vec![
                Op::StackRef(0),
                Op::Constant(0),
                Op::Lss,
                Op::GotoIfNil(6),
                Op::StackRef(0),
                Op::Return,
                Op::StackRef(0),
                Op::Sub1,
                Op::Return,
            ],
            vec![Value::make_int(2)],
            1,
            vec![Value::make_int(9)],
        ),
        // Pure countdown loop: (lambda (n) (let ((acc 0)) (while (> n 0)
        // (setq acc (+ acc n)) (setq n (1- n))) acc)).
        (
            vec![
                Op::Constant(0),   // 0  acc=0      [n 0]
                Op::StackRef(1),   // 1  [n acc n]   <- head
                Op::Constant(0),   // 2  0
                Op::Gtr,           // 3  [n acc c]
                Op::GotoIfNil(13), // 4  [n acc]
                Op::StackRef(1),   // 5  n
                Op::StackRef(1),   // 6  acc
                Op::Add,           // 7  acc'
                Op::StackSet(1),   // 8  [n acc']
                Op::StackRef(1),   // 9  n
                Op::Sub1,          // 10 n-1
                Op::StackSet(2),   // 11 [n-1 acc']
                Op::Goto(1),       // 12 backedge
                Op::StackRef(0),   // 13 [n acc acc]
                Op::Return,        // 14
            ],
            vec![Value::make_int(0)],
            1,
            vec![Value::make_int(10)],
        ),
    ];

    for (ops, constants, arity, args) in cases {
        let mir = mir::build_mir(&ops, &constants, arity).expect("MIR builds");
        let leaf = lower_mir_pure(&mir).expect("MIR lowers (pure subset)");

        // Interpreter oracle.
        let mut ev = crate::emacs_core::eval::Context::new_minimal_vm_harness();
        let mut f = ByteCodeFunction::new(LambdaParams {
            required: (1..=arity)
                .map(|i| crate::emacs_core::intern::SymId(i as u32))
                .collect(),
            optional: Vec::new(),
            rest: None,
        });
        f.lexical = true;
        f.ops = ops.clone();
        f.constants = constants.clone().into();
        f.max_stack = 32;
        let want = {
            let mut vm = Vm::from_context(&mut ev);
            vm.execute(&f, args.clone()).expect("interpreter runs")
        };

        match leaf.call_for_test(&args) {
            Some(bits) => assert_eq!(
                Value::from_bits(bits),
                want,
                "MIR-lowered native result must equal the interpreter for {ops:?}"
            ),
            None => panic!("MIR-lowered pure body deopted unexpectedly for {ops:?}"),
        }
    }
}

/// A pure-arithmetic guard deopts cleanly (non-fixnum input) — same as the
/// baseline tier, since the pure subset reruns the interpreter from start.
#[test]
fn mir_pure_lowering_deopts_on_nonfixnum() {
    // (lambda (a b) (+ a b)) called with a string -> the fixnum guard fails.
    let ops = vec![Op::StackRef(1), Op::StackRef(1), Op::Add, Op::Return];
    let mir = mir::build_mir(&ops, &[], 2).expect("builds");
    let leaf = lower_mir_pure(&mir).expect("lowers");
    assert_eq!(
        leaf.call_for_test(&[Value::string("x"), Value::make_int(2)]),
        None,
        "non-fixnum operand deopts (rerun-from-start)"
    );
}

/// A CALL LOWERS in the MIR tier (the calls-slice handles it via precise
/// deopt + the generic shim), and so does `eq` (through the adapter, via the
/// symbols-with-position shim) — an `eq`-only body is context-free, so it
/// keeps rerun-from-start.
#[test]
fn mir_pure_lowering_handles_a_call() {
    use crate::emacs_core::eval::Context;
    // (lambda () (foo)) — has a Call (opaque) -> lowered.
    let ops = vec![Op::Constant(0), Op::Call(0), Op::Return];
    let mir = mir::build_mir(&ops, &[Value::symbol("foo")], 0).expect("MIR builds");
    let leaf = lower_mir_pure(&mir).expect("a call lowers via the calls-slice");
    assert!(
        leaf.has_side_effects,
        "a call-bearing leaf is side-effecting (no rerun-from-start)"
    );
    // (lambda (a b) (eq a b)) — Eq lowers through the adapter.
    let eq_ops = vec![Op::StackRef(1), Op::StackRef(1), Op::Eq, Op::Return];
    let eq_mir = mir::build_mir(&eq_ops, &[], 2).expect("eq MIR builds");
    let eq_leaf = lower_mir_pure(&eq_mir).expect("eq lowers via the adapter");
    assert!(
        !eq_leaf.has_side_effects,
        "an eq-only body has no Opaque: it still reruns from the start"
    );
    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context as *mut u8;
    let sym = Value::symbol("jit-mir-eq-probe");
    for (a, b, want) in [
        (Value::make_int(3), Value::make_int(3), Value::T),
        (Value::make_int(3), Value::make_int(4), Value::NIL),
        (sym, sym, Value::T),
        (sym, Value::NIL, Value::NIL),
    ] {
        match eq_leaf.call(ctx, &[a, b]) {
            NativeRun::Ok(bits) => assert_eq!(bits, want.bits(), "(eq {a:?} {b:?})"),
            other => panic!("(eq {a:?} {b:?}): {other:?}"),
        }
    }
}

/// Rooting sites in one MIR body share ONE hoisted root window: the prologue
/// loads the frame base and checks capacity once, the first `set` site
/// stores the residual `[l h]`, and the next two sites (a call, a second
/// `set`) find the same values already in the window and elide their
/// stores. (`set` because it runs variable watchers, so it roots for good;
/// `length` was the example until it became GC-free.) The elision is only sound if the slots really still hold them, so
/// the middle site is a call to a function that runs an EXACT collection
/// (which ignores the native stack) and then reallocates 4096 conses: a
/// dropped root for `h` would be swept and its slot reused, reading back
/// `(0 . 0)`-shaped garbage instead of `(1 . 2)`.
///
///     (lambda (l) (let ((h (cons 1 2))) (set 'v l) (gcchurn) (set 'v l) h))
#[test]
fn mir_hoists_the_root_window_across_shim_sites() {
    use crate::emacs_core::eval::Context;
    let mut ev = Context::new();
    ev.gc_stress = true;
    let ctx = &mut ev as *mut Context as *mut u8;
    let churn_sym = Value::symbol("jit-mir-hoist-gcchurn");
    let crate::emacs_core::value::ValueKind::Symbol(churn_id) = churn_sym.kind() else {
        panic!("symbol");
    };
    let mut churn = ByteCodeFunction::new(LambdaParams {
        required: Vec::new(),
        optional: Vec::new(),
        rest: None,
    });
    churn.lexical = true;
    churn.ops = vec![
        Op::Constant(0), // 'garbage-collect
        Op::Call(0),     // exact collection
        Op::Pop,
        Op::Constant(1), // 'make-list
        Op::Constant(2), // 4096
        Op::Constant(3), // 0
        Op::Call(2),     // reuses freed slots
        Op::Return,
    ];
    churn.constants = vec![
        Value::symbol("garbage-collect"),
        Value::symbol("make-list"),
        Value::make_int(4096),
        Value::make_int(0),
    ]
    .into();
    churn.max_stack = 16;
    ev.obarray
        .set_symbol_function_id(churn_id, Value::make_bytecode(churn));
    let ops = vec![
        Op::Constant(0), // 1                 [l 1]
        Op::Constant(1), // 2                 [l 1 2]
        Op::Cons,        // h = (1 . 2)       [l h]
        Op::Constant(3), // 'v                [l h v]
        Op::StackRef(2), // l                 [l h v l]
        Op::Set,         // residual [l h]    stores 2
        Op::Pop,         //                   [l h]
        Op::Constant(2), // 'gcchurn          [l h g]
        Op::Call(0),     // residual [l h]    elides 2
        Op::Pop,         //                   [l h]
        Op::Constant(3), // 'v                [l h v]
        Op::StackRef(2), // l                 [l h v l]
        Op::Set,         // residual [l h]    elides 2
        Op::Pop,         //                   [l h]
        Op::Return,      // h
    ];
    let constants = [
        Value::make_int(1),
        Value::make_int(2),
        churn_sym,
        Value::symbol("jit-mir-hoist-var"),
    ];
    let mir = mir::build_mir(&ops, &constants, 1).expect("MIR builds");
    let plan = super::lowering::plan_mir_leaf(&mir);
    assert_eq!(plan.rooting_sites, 3);
    let leaf = lower_mir_pure(&mir).expect("lowers");
    assert_eq!(
        super::lowering::rootwin_counters(),
        (2, 4),
        "the first site stores l and h; the call and the second length elide both"
    );
    for _ in 0..3 {
        let l = ev.eval_str("(list 1 2 3)").expect("l");
        let NativeRun::Ok(bits) = leaf.call(ctx, &[l]) else {
            panic!("must run natively");
        };
        let h = Value::from_bits(bits);
        assert!(h.is_cons(), "h survived the exact collection (got {h:?})");
        assert_eq!(h.cons_car(), Value::make_int(1), "car intact");
        assert_eq!(h.cons_cdr(), Value::make_int(2), "cdr intact");
    }
}

/// A residual value the SSA proves is a tagged fixnum — a constant, or a
/// retag of one — is never stored into the root window: it is not a pointer,
/// so no GC can free it. Every rooting site here skips its whole residual,
/// which also empties the store record (`RootWinCarry` rule 1, kept under
/// test by `mir_store_record_is_truncated_to_each_sites_count`, whose
/// residuals are arguments).
///
///     (let ((k 7)) (set 'L 'L) (f) (set 'L 'L) k)
///
#[test]
fn mir_fixnum_residual_is_never_root_stored() {
    let ops = vec![
        Op::Constant(0), // k = 7        [k]
        Op::Constant(1), // L            [k L]
        Op::Dup,         //              [k L L]
        Op::Set,         // residual [k]
        Op::Pop,         //              [k]
        Op::Constant(2), // f            [k f]
        Op::Call(0),     // residual [k]
        Op::Pop,         //              [k]
        Op::Constant(1), // L            [k L]
        Op::Dup,         //              [k L L]
        Op::Set,         // residual [k]
        Op::Pop,         //              [k]
        Op::Return,
    ];
    let constants = [
        Value::make_int(7),
        Value::symbol("jit-carry-l"),
        Value::symbol("jit-carry-f"),
    ];
    let mir = mir::build_mir(&ops, &constants, 0).expect("MIR builds");
    assert_eq!(super::lowering::plan_mir_leaf(&mir).rooting_sites, 3);
    lower_mir_pure(&mir).expect("lowers");
    assert_eq!(
        super::lowering::rootwin_counters(),
        (0, 0),
        "a fixnum residual is never a root: three rooting sites, no stores"
    );
}

/// `RootWinCarry` rule 1 with a nonzero count: residuals of 3, then 1, then 3
/// slots. The middle call roots only slot 0, so slots 1 and 2 may be
/// overwritten by its callee and the third site must store them again.
#[test]
fn mir_store_record_is_truncated_to_each_sites_count() {
    let ops = vec![
        Op::StackRef(0), // [a a]
        Op::StackRef(1), // [a a a]
        Op::Constant(0), // [a a a L]
        Op::Dup,         // [a a a L L]
        Op::Set,         // residual [a a a]: 3 stores
        Op::DiscardN(3), // [a]
        Op::Constant(1), // [a f]
        Op::Call(0),     // residual [a]: slot 0 elided, record truncated to 1
        Op::Pop,         // [a]
        Op::StackRef(0), // [a a]
        Op::StackRef(1), // [a a a]
        Op::Constant(0), // [a a a L]
        Op::Dup,         // [a a a L L]
        Op::Set,         // residual [a a a]: slot 0 elided, slots 1-2 stored
        Op::DiscardN(3), // [a]
        Op::Return,
    ];
    let constants = [Value::symbol("jit-carry-l"), Value::symbol("jit-carry-f")];
    let mir = mir::build_mir(&ops, &constants, 1).expect("MIR builds");
    lower_mir_pure(&mir).expect("lowers");
    assert_eq!(
        super::lowering::rootwin_counters(),
        (5, 2),
        "3 + 0 + 2 stores; (3, 4) means slots above the call's count were trusted"
    );
}

/// `RootWinCarry` rule 2, in the BASELINE: a Tier-A `point` site stores
/// nothing on its fast path (a GC-free read) but its NEED_GENERIC fallback
/// roots the residual for a general call. The record at the continuation is
/// the meet of the two paths; the following call site must still store its
/// residual. Before the fix the fallback's compile-time stores leaked into
/// the record, the call's stores were all elided, and on the (normal) fast
/// path `x` sat unrooted across the call.
///
///     (lambda (a n) (let ((x (cons a a))) (h (+ n (point)) (1+ n)) x))
#[test]
fn baseline_tier_a_fallback_stores_do_not_leak_into_the_next_site() {
    use crate::emacs_core::eval::Context;
    use crate::emacs_core::intern::{SymId, intern};
    crate::emacs_core::jit::compile::force_profit_gate_for_test(false);
    let ev = Context::new();
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: vec![SymId(1), SymId(2)],
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = vec![
        Op::StackRef(1),                        // a        [a n a]
        Op::Dup,                                //          [a n a a]
        Op::Cons,                               // x        [a n x]
        Op::Constant(0),                        // h        [a n x h]
        Op::StackRef(2),                        // n        [a n x h n]
        Op::CallBuiltinSym(intern("point"), 0), // Tier-A   [a n x h n p]
        Op::Add,                                //          [a n x h s]
        Op::StackRef(3),                        // n        [a n x h s n]
        Op::Add1,                               //          [a n x h s n1]
        Op::Call(2),                            // residual [a n x]
        Op::Pop,                                //          [a n x]
        Op::Return,                             // x
    ];
    f.constants = vec![Value::symbol("jit-carry-h")].into();
    f.max_stack = 16;
    let leaf = compile_bytecode_function_with(&f, Some(&ev.obarray)).expect("compiles");
    assert_eq!(leaf.tier, super::leaf::LeafTier::Baseline);
    let (stored, elided) = super::lowering::rootwin_counters();
    assert_eq!(
        elided, 0,
        "the call's residual [a n x] is stored after the Tier-A site (stored {stored}); \
         an elision means the fallback-only stores were trusted on the fast path"
    );
}

/// Rule 2 on the other conditional arm: a reg-args predicate spec site
/// (`recordp` on a constant symbol) passes its argument in a register and
/// roots nothing on the fast path; its NEED_GENERIC fallback roots the
/// residual. The call after it must still store `[a n x]`.
///
///     (lambda (a n) (let ((x (cons a a))) (h (recordp n) (1+ n)) x))
#[test]
fn baseline_reg_args_fallback_stores_do_not_leak_into_the_next_site() {
    use crate::emacs_core::eval::Context;
    use crate::emacs_core::intern::SymId;
    crate::emacs_core::jit::compile::force_profit_gate_for_test(false);
    let ev = Context::new();
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: vec![SymId(1), SymId(2)],
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = vec![
        Op::StackRef(1), // a                 [a n a]
        Op::Dup,         //                   [a n a a]
        Op::Cons,        // x                 [a n x]
        Op::Constant(0), // h                 [a n x h]
        Op::Constant(1), // 'recordp          [a n x h recordp]
        Op::StackRef(3), // n                 [a n x h recordp n]
        Op::Call(1),     // reg-args pred     [a n x h r]
        Op::StackRef(3), // n                 [a n x h r n]
        Op::Add1,        //                   [a n x h r n1]
        Op::Call(2),     // residual [a n x]
        Op::Pop,         //                   [a n x]
        Op::Return,      // x
    ];
    f.constants = vec![Value::symbol("jit-carry-h2"), Value::symbol("recordp")].into();
    f.max_stack = 16;
    let leaf = compile_bytecode_function_with(&f, Some(&ev.obarray)).expect("compiles");
    assert_eq!(leaf.tier, super::leaf::LeafTier::Baseline);
    let (stored, elided) = super::lowering::rootwin_counters();
    assert_eq!(
        elided, 0,
        "the call's residual is stored after the reg-args site (stored {stored}); an \
         elision means the fallback-only stores were trusted on the fast path"
    );
}

/// Rule 2 must compare slot by slot, not just truncate. An earlier site
/// stores `[a b]`; slot 0 is then replaced by a fresh `c`; a Tier-A `point`
/// site has residual `[c b]` — its fast path stores nothing (slot 0 still
/// holds `a`), its fallback stores `c` over it. At the continuation only
/// slot 1 is common to both paths, so the next site must store `c` and may
/// elide `b`.
///
///     (lambda (a b) (setq v v) (let ((c (cons a a))) (setq a c) (point) (setq v v) b))
#[test]
fn baseline_fallback_meet_keeps_only_the_slots_both_paths_agree_on() {
    use crate::emacs_core::eval::Context;
    use crate::emacs_core::intern::{SymId, intern};
    crate::emacs_core::jit::compile::force_profit_gate_for_test(false);
    let ev = Context::new();
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: vec![SymId(1), SymId(2)],
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = vec![
        Op::Constant(0),                        // [a b v]
        Op::VarSet(0),                          // residual [a b]: stores a, b
        Op::StackRef(1),                        // a        [a b a]
        Op::Dup,                                //          [a b a a]
        Op::Cons,                               // c        [a b c]
        Op::StackSet(2),                        //          [c b]
        Op::CallBuiltinSym(intern("point"), 0), // residual [c b]: fast path stores nothing
        Op::Pop,                                // [c b]
        Op::Constant(0),                        // [c b v]
        Op::VarSet(0),                          // residual [c b]: store c, elide b
        Op::Return,                             // b
    ];
    f.constants = vec![Value::symbol("jit-meet-carry-v")].into();
    f.max_stack = 16;
    // Both tiers now use the same named-read fast path and rooted fallback.
    for baseline in [true, false] {
        let leaf = if baseline {
            lower_leaf_full(&f.ops, &f.constants, 2, None, Some(&ev.obarray), 0)
                .expect("baseline compiles")
        } else {
            compile_bytecode_function_with(&f, Some(&ev.obarray)).expect("MIR compiles")
        };
        assert_eq!(
            leaf.tier,
            if baseline {
                super::leaf::LeafTier::Baseline
            } else {
                super::leaf::LeafTier::Mir
            }
        );
        assert_eq!(
            super::lowering::rootwin_counters(),
            (4, 2),
            "a+b, then the fallback's c (b elided), then the last site's c (b elided); \
             (3, 3) means the meet kept the fallback's slot 0 though the fast path never wrote it"
        );
    }
}

/// Rule 3 at a handler-dispatch block. Dispatch blocks are emitted after the
/// whole bytecode block, so the store record they see is the block's END
/// state, but each is entered from ONE site's signal edge. A Tier-A `point`
/// site stores nothing on its fast path; the `setq` after it (whose shim
/// call always roots its residual) stores `[a x]`. If `point` signals, its dispatch block roots `[a x]` for the
/// match shim (which can run Lisp) — and must STORE them, not trust a record
/// written by a site the signal path never ran.
///
///     (lambda (a) (let ((x (cons a a))) (condition-case nil (progn (point) (setq v v)) (error x))))
#[test]
fn baseline_handler_dispatch_blocks_do_not_trust_the_block_end_store_record() {
    use crate::emacs_core::eval::Context;
    use crate::emacs_core::intern::{SymId, intern};
    crate::emacs_core::jit::compile::force_profit_gate_for_test(false);
    let ev = Context::new();
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: vec![SymId(1)],
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = vec![
        Op::StackRef(0),                        // 0: a              [a a]
        Op::Dup,                                // 1                 [a a a]
        Op::Cons,                               // 2: x              [a x]
        Op::PushConditionCase(10),              // 3                 [a x]
        Op::CallBuiltinSym(intern("point"), 0), // 4: Tier-A site    [a x p]
        Op::Pop,                                // 5                 [a x]
        Op::Constant(0),                        // 6                 [a x v]
        Op::VarSet(0),                          // 7: stores [a x]   [a x]
        Op::PopHandler,                         // 8                 [a x]
        Op::Return,                             // 9: x
        Op::Return,                             // 10: handler       [a x err]
    ];
    f.constants = vec![Value::symbol("jit-dispatch-carry-v")].into();
    f.max_stack = 16;
    let leaf = compile_bytecode_function_with(&f, Some(&ev.obarray)).expect("compiles");
    assert_eq!(leaf.tier, super::leaf::LeafTier::Baseline);
    let (stored, elided) = super::lowering::rootwin_counters();
    assert_eq!(
        elided, 0,
        "every dispatch block stores its residual (stored {stored}); an elision means \
         a dispatch block trusted the record from the end of its bytecode block"
    );
}

/// Rule 3 at back-edge poll blocks. A Switch whose jump table has two
/// BACKWARD targets emits two poll trampolines as sibling paths of its
/// compare chain; each roots the live stack `[k c]` around the poll shim (a
/// GC safe point). The second must not elide the stores the first made — at
/// run time it is reached without passing through the first.
///
///     0: (foo) [k acc] 2: (setq acc (cons 1 acc)) ... (switch k (a -> 0) (b -> 2)) (foo) acc
#[test]
fn baseline_switch_back_edge_polls_store_their_own_roots() {
    use crate::emacs_core::value::HashTableTest;
    let table = Value::hash_table(HashTableTest::Eq);
    let _ = table.with_hash_table_mut(|ht| {
        for (name, target) in [("jit-sw-carry-a", 0), ("jit-sw-carry-b", 2)] {
            let key = Value::symbol(name).to_hash_key(&ht.test);
            ht.insert(key, Value::symbol(name), Value::fixnum(target));
        }
    });
    let ops = [
        Op::Constant(0), // 0                        [k acc foo]
        Op::VarSet(0),   // 1: residual [k acc]      [k acc]
        Op::Constant(1), // 2: (target) 1            [k acc 1]
        Op::StackRef(1), // 3: acc                   [k acc 1 acc]
        Op::Cons,        // 4: c                     [k acc c]
        Op::StackSet(1), // 5                        [k c]
        Op::StackRef(1), // 6: k                     [k c k]
        Op::Constant(2), // 7: table                 [k c k table]
        Op::Switch,      // 8: -> 0 or 2 (both back) [k c]
        Op::Constant(0), // 9                        [k c foo]
        Op::VarSet(0),   // 10: residual [k c]       [k c]
        Op::Return,      // 11: c
    ];
    let constants = [Value::symbol("jit-sw-carry-foo"), Value::make_int(1), table];
    lower_leaf_with_map(&ops, &constants, 2, None).expect("switch loop compiles");
    let (stored, elided) = super::lowering::rootwin_counters();
    assert_eq!(
        elided, 0,
        "each poll trampoline stores [k c] itself (stored {stored}); an elision means \
         the second trampoline trusted stores made only on the first's path"
    );
}

/// Every `JIT_BUILTIN2_PURE` entry answers exactly as the rooted builtin at
/// the same index: same value (by `eq`-bits, or structural `equal` for a
/// freshly consed answer) or the same signal. Each comparison gets freshly
/// built arguments, because `setcar`/`setcdr` mutate them.
#[test]
fn jit_builtin2_pure_matches_the_table() {
    use super::dispatch::{JIT_BUILTIN2, JIT_BUILTIN2_PURE};
    use crate::emacs_core::error::Flow;
    use crate::emacs_core::eval::Context;
    let mut ev = Context::new();
    ev.eval_str("(put 'jit-pure-sym 'jit-pure-prop 42)")
        .expect("put");
    let pool_src = [
        "nil",
        "t",
        "0",
        "1",
        "-1",
        "5",
        "1.5",
        "'jit-pure-sym",
        "'jit-pure-prop",
        "(list 1 2 3)",
        "(list (cons 'a 1) (cons 'b 2) (cons 1 3))",
        "(vector 1 2 3)",
        "(copy-sequence \"abc\")",
        "(list 'jit-pure-sym 'x)",
        // `equal` but not `eq` to the fresh `1.5` and `"abc"` above: tells
        // memq/assq from member/assoc.
        "(list 1.5 (copy-sequence \"abc\") (cons 1.5 'f))",
    ];
    let outcome = |r: Result<Value, Flow>| -> String {
        match r {
            Ok(v) => crate::emacs_core::print::print_value(&v),
            Err(Flow::Signal(sig)) => format!("signal {}", sig.symbol_name()),
            Err(other) => format!("{other:?}"),
        }
    };
    let mut checked = 0;
    for (idx, pure) in JIT_BUILTIN2_PURE.iter().enumerate() {
        let Some(pure) = pure else { continue };
        for a_src in pool_src {
            for b_src in pool_src {
                // Both arguments come from ONE evaluation and go straight into
                // the call: a Rust local is not a GC root, and a second
                // evaluation under GC stress would free the first argument.
                let pair_src = format!("(cons {a_src} {b_src})");
                let pair = ev.eval_str(&pair_src).expect("args");
                let (a1, b1) = (pair.cons_car(), pair.cons_cdr());
                let rooted = outcome(JIT_BUILTIN2[idx](&mut ev, a1, b1));
                let pair = ev.eval_str(&pair_src).expect("args");
                let (a2, b2) = (pair.cons_car(), pair.cons_cdr());
                let root_free = outcome(pure(&ev, a2, b2));
                assert_eq!(
                    root_free, rooted,
                    "JIT_BUILTIN2[{idx}] on ({a_src}, {b_src}): the pure entry must answer as the table does"
                );
                checked += 1;
            }
        }
    }
    assert_eq!(checked, 11 * pool_src.len() * pool_src.len());
}

/// The unary twin: every `JIT_BUILTIN1_PURE` entry answers exactly as the
/// rooted builtin at its index (values and signals), each argument built in
/// the same evaluation that feeds the call.
#[test]
fn jit_builtin1_pure_matches_the_table() {
    use super::dispatch::{JIT_BUILTIN1, JIT_BUILTIN1_PURE};
    use crate::emacs_core::error::Flow;
    use crate::emacs_core::eval::Context;
    let mut ev = Context::new();
    ev.eval_str(
        "(progn (defvar jit-pure1-var 7) (defvar jit-pure1-unbound) (fset 'jit-pure1-fn 'car))",
    )
    .expect("setup");
    let pool_src = [
        "nil",
        "t",
        "5",
        "1.5",
        "'jit-pure1-var",
        "'jit-pure1-unbound",
        "'jit-pure1-fn",
        "'jit-pure1-never",
        "(list 1 2 3)",
        "(cons 1 2)",
        "(vector 1 2 3)",
        "(copy-sequence \"h\\u00e9llo\")",
        "(make-bool-vector 3 t)",
        "(record 'foo 1)",
    ];
    let outcome = |r: Result<Value, Flow>| -> String {
        match r {
            Ok(v) => crate::emacs_core::print::print_value(&v),
            Err(Flow::Signal(sig)) => format!(
                "signal {} {:?}",
                sig.symbol_name(),
                sig.data
                    .iter()
                    .map(crate::emacs_core::print::print_value)
                    .collect::<Vec<_>>()
            ),
            Err(other) => format!("{other:?}"),
        }
    };
    for (idx, pure) in JIT_BUILTIN1_PURE.iter().enumerate() {
        let Some(pure) = pure else { continue };
        for src in pool_src {
            let a = ev.eval_str(src).expect("arg");
            let rooted = outcome(JIT_BUILTIN1[idx](&mut ev, a));
            let a = ev.eval_str(src).expect("arg");
            let root_free = outcome(pure(&ev, a));
            assert_eq!(root_free, rooted, "JIT_BUILTIN1[{idx}] on {src}");
        }
    }
}

/// The unary twin of `gc_free_builtin_sites_root_no_residual`: two `length`
/// sites with a live residual `[l]` store no root, and both answer.
///
///     (lambda (l) (length l) (length l))
#[test]
fn gc_free_unary_builtin_sites_root_no_residual() {
    use crate::emacs_core::eval::Context;
    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context as *mut u8;
    let ops = [
        Op::StackRef(0), // l   [l l]
        Op::Length,      // residual [l]
        Op::Pop,         //     [l]
        Op::StackRef(0), // l   [l l]
        Op::Length,      // residual [l]
        Op::Return,
    ];
    let leaf = lower_leaf(&ops, &[], 1).expect("lowers");
    assert_eq!(
        super::lowering::rootwin_counters(),
        (0, 0),
        "a GC-free unary builtin site stores no residual root"
    );
    let l = ev.eval_str("(list 1 2 3)").expect("l");
    let NativeRun::Ok(bits) = leaf.call(ctx, &[l]) else {
        panic!("length must run natively");
    };
    assert_eq!(Value::from_bits(bits), Value::make_int(3));
}

/// A GC-free builtin site roots nothing: two `memq` sites with a live
/// residual `[a l]` in a hoisted body emit no root-window stores at all, and
/// the body still answers.
///
///     (lambda (a l) (memq a l) (memq a l))
#[test]
fn gc_free_builtin_sites_root_no_residual() {
    use crate::emacs_core::eval::Context;
    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context as *mut u8;
    let ops = [
        Op::StackRef(1), // a   [a l a]
        Op::StackRef(1), // l   [a l a l]
        Op::Memq,        // residual [a l]
        Op::Pop,         //     [a l]
        Op::StackRef(1), // a   [a l a]
        Op::StackRef(1), // l   [a l a l]
        Op::Memq,        // residual [a l]
        Op::Return,
    ];
    let leaf = lower_leaf(&ops, &[], 2).expect("lowers");
    assert_eq!(
        super::lowering::rootwin_counters(),
        (0, 0),
        "a GC-free builtin site stores no residual root"
    );
    let l = ev.eval_str("(list 1 2 3)").expect("l");
    let NativeRun::Ok(bits) = leaf.call(ctx, &[Value::make_int(2), l]) else {
        panic!("memq must run natively");
    };
    assert_eq!(
        crate::emacs_core::print::print_value(&Value::from_bits(bits)),
        "(2 3)"
    );
}

/// Compiled `aset` (the root-free variant-3 shim) answers exactly as the
/// builtin on every array kind and error: the returned value or signal, and
/// the array afterwards. Each case's array is built fresh, in one evaluation,
/// for both the compiled call and the reference call.
#[test]
fn compiled_aset_matches_the_builtin() {
    use crate::emacs_core::error::Flow;
    use crate::emacs_core::eval::Context;
    use crate::emacs_core::print::print_value;
    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context as *mut u8;
    // (lambda (a i v) (aset a i v))
    let ops = [
        Op::StackRef(2),
        Op::StackRef(2),
        Op::StackRef(2),
        Op::Aset,
        Op::Return,
    ];
    let leaf = lower_leaf(&ops, &[], 3).expect("aset lowers");
    let cases = [
        "(list (vector 1 2 3) 1 'x)",
        "(list (vector 1 2 3) 3 'x)",
        "(list (vector 1 2 3) -1 'x)",
        "(list (vector 1 2 3) 'a 'x)",
        "(list (copy-sequence \"abc\") 0 ?z)",
        "(list (copy-sequence \"abc\") 1 ?\\u00e9)",
        "(list (copy-sequence \"h\\u00e9llo\") 1 ?e)",
        "(list (copy-sequence \"abc\") 5 ?z)",
        "(list (copy-sequence \"abc\") 0 'notachar)",
        "(list (make-bool-vector 4 nil) 2 t)",
        "(list (make-char-table 'test) ?a 'val)",
        "(list (record 'foo 1 2) 1 'y)",
        "(list 'not-an-array 0 1)",
        "(list nil 0 1)",
    ];
    let show = |r: Result<Value, String>, arr: Value| -> String {
        match r {
            Ok(v) => format!("{} / {}", print_value(&v), print_value(&arr)),
            Err(e) => format!("{e} / {}", print_value(&arr)),
        }
    };
    for case in cases {
        let args = ev.eval_str(case).expect("args");
        let (a, i, v) = (
            args.cons_car(),
            args.cons_cdr().cons_car(),
            args.cons_cdr().cons_cdr().cons_car(),
        );
        let native = match leaf.call(ctx, &[a, i, v]) {
            NativeRun::Ok(bits) => Ok(Value::from_bits(bits)),
            NativeRun::Signal => match take_pending_flow() {
                Some(Flow::Signal(sig)) => Err(format!("signal {}", sig.symbol_name())),
                other => Err(format!("{other:?}")),
            },
            other => Err(format!("unexpected {other:?}")),
        };
        let got = show(native, a);
        let args = ev.eval_str(case).expect("args");
        let (a, i, v) = (
            args.cons_car(),
            args.cons_cdr().cons_car(),
            args.cons_cdr().cons_cdr().cons_car(),
        );
        let reference = match crate::emacs_core::builtins::builtin_aset_args(&[a, i, v]) {
            Ok(v) => Ok(v),
            Err(Flow::Signal(sig)) => Err(format!("signal {}", sig.symbol_name())),
            Err(other) => Err(format!("{other:?}")),
        };
        assert_eq!(got, show(reference, a), "(aset . {case})");
    }
}

/// A REDEFINED `aset` still runs from compiled code, through the rooted
/// fallback: the fast shim bounces it, and the fallback roots the residual
/// across a redefinition that collects exactly and reallocates. `h` lives
/// only in the residual.
///
///     (lambda (v) (let ((h (cons 1 2))) (aset v 0 9) h))
#[test]
fn redefined_aset_takes_the_rooted_fallback() {
    use crate::emacs_core::eval::Context;
    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context as *mut u8;
    let ops = [
        Op::Constant(0), // 1                 [v 1]
        Op::Constant(1), // 2                 [v 1 2]
        Op::Cons,        // h                 [v h]
        Op::StackRef(1), // v                 [v h v]
        Op::Constant(2), // 0                 [v h v 0]
        Op::Constant(3), // 9                 [v h v 0 9]
        Op::Aset,        // residual [v h]    [v h r]
        Op::Pop,         //                   [v h]
        Op::Return,      // h
    ];
    let constants = [
        Value::make_int(1),
        Value::make_int(2),
        Value::make_int(0),
        Value::make_int(9),
    ];
    let leaf = lower_leaf(&ops, &constants, 1).expect("lowers");
    // The builtin: the vector changes.
    let v = ev.eval_str("(vector 0 0)").expect("v");
    let NativeRun::Ok(bits) = leaf.call(ctx, &[v]) else {
        panic!("aset must run natively");
    };
    let h = Value::from_bits(bits);
    assert_eq!(h.cons_car(), Value::make_int(1));
    assert_eq!(crate::emacs_core::print::print_value(&v), "[9 0]");
    // Redefined: it collects and reallocates; `h` must survive, the vector
    // is untouched.
    ev.eval_str("(fset 'aset (lambda (a i x) (garbage-collect) (make-list 4096 0) 'redefined))")
        .expect("fset");
    for _ in 0..3 {
        let v = ev.eval_str("(vector 0 0)").expect("v");
        let NativeRun::Ok(bits) = leaf.call(ctx, &[v]) else {
            panic!("redefined aset must still run");
        };
        let h = Value::from_bits(bits);
        assert!(
            h.is_cons(),
            "h survived the redefinition's collection (got {h:?})"
        );
        assert_eq!(h.cons_car(), Value::make_int(1), "car intact");
        assert_eq!(h.cons_cdr(), Value::make_int(2), "cdr intact");
        assert_eq!(
            crate::emacs_core::print::print_value(&v),
            "[0 0]",
            "the redefinition ran instead of the builtin"
        );
    }
}

/// The tier gate, through the production compile path. A loop with a
/// shim-lowered op stays on the baseline until adapter-bearing loops are
/// qualified separately, although MIR lowering accepts it; the same op
/// outside a loop takes the MIR tier.
#[test]
fn tier_gate_sends_a_loop_with_a_shim_op_to_the_baseline() {
    use crate::emacs_core::eval::Context;
    use crate::emacs_core::intern::SymId;
    crate::emacs_core::jit::compile::force_profit_gate_for_test(false);
    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context as *mut u8;
    // (lambda (n l) (let ((i 0)) (while (< i n) (length l) (setq i (1+ i))) i))
    let loop_ops = vec![
        Op::Constant(0),   // 0: i = 0     [n l i]
        Op::StackRef(0),   // 1: i (head)  [n l i i]
        Op::StackRef(3),   // 2: n         [n l i i n]
        Op::Lss,           // 3            [n l i c]
        Op::GotoIfNil(12), // 4            [n l i]
        Op::StackRef(1),   // 5: l         [n l i l]
        Op::Length,        // 6            [n l i len]
        Op::Pop,           // 7            [n l i]
        Op::StackRef(0),   // 8: i         [n l i i]
        Op::Add1,          // 9            [n l i i1]
        Op::StackSet(1),   // 10           [n l i1]
        Op::Goto(1),       // 11
        Op::Return,        // 12: i
    ];
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: vec![SymId(1), SymId(2)],
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = loop_ops.clone();
    f.constants = vec![Value::make_int(0)].into();
    f.max_stack = 16;
    let mir = mir::build_mir(&loop_ops, &f.constants, 2).expect("MIR builds");
    assert!(
        lower_mir_pure(&mir).is_ok(),
        "the MIR lowering accepts the loop, so the gate is what rejects it"
    );
    let leaf = compile_bytecode_function_with(&f, Some(&ev.obarray)).expect("compiles");
    assert_eq!(
        leaf.tier,
        super::leaf::LeafTier::Baseline,
        "gate:loop-opaque"
    );
    let l = ev.eval_str("(list 1 2)").expect("l");
    match leaf.call(ctx, &[Value::make_int(5), l]) {
        NativeRun::Ok(bits) => assert_eq!(bits, Value::make_int(5).bits()),
        other => panic!("loop: {other:?}"),
    }
    // A single-block loop (block 0 jumps to itself) is a loop too.
    // (lambda (l n) (while (progn (length l) (setq n (1- n)) (> n 0))) n)
    let self_loop = vec![
        Op::StackRef(1),     // 0: l          [l n l]
        Op::Length,          // 1             [l n len]
        Op::Pop,             // 2             [l n]
        Op::StackRef(0),     // 3: n          [l n n]
        Op::Sub1,            // 4             [l n n1]
        Op::StackSet(1),     // 5             [l n1]
        Op::StackRef(0),     // 6: n          [l n n]
        Op::Constant(0),     // 7: 0          [l n n 0]
        Op::Gtr,             // 8             [l n c]
        Op::GotoIfNotNil(0), // 9             [l n]
        Op::Return,          // 10: n
    ];
    let mut h = ByteCodeFunction::new(LambdaParams {
        required: vec![SymId(1), SymId(2)],
        optional: Vec::new(),
        rest: None,
    });
    h.lexical = true;
    h.ops = self_loop.clone();
    h.constants = vec![Value::make_int(0)].into();
    h.max_stack = 16;
    let mir = mir::build_mir(&self_loop, &h.constants, 2).expect("MIR builds the self-loop");
    assert!(
        super::lowering::plan_mir_leaf(&mir).has_backedge,
        "block 0 jumps to itself"
    );
    let leaf = compile_bytecode_function_with(&h, Some(&ev.obarray)).expect("compiles");
    assert_eq!(
        leaf.tier,
        super::leaf::LeafTier::Baseline,
        "gate:loop-opaque (self-loop)"
    );
    // The same op, loop-free: the MIR tier.
    let mut g = ByteCodeFunction::new(LambdaParams {
        required: vec![SymId(1)],
        optional: Vec::new(),
        rest: None,
    });
    g.lexical = true;
    g.ops = vec![Op::StackRef(0), Op::Length, Op::Return];
    g.max_stack = 16;
    let leaf = compile_bytecode_function_with(&g, Some(&ev.obarray)).expect("compiles");
    assert_eq!(leaf.tier, super::leaf::LeafTier::Mir);
    let l = ev.eval_str("(list 1 2 3)").expect("l");
    match leaf.call(ctx, &[l]) {
        NativeRun::Ok(bits) => assert_eq!(bits, Value::make_int(3).bits()),
        other => panic!("length: {other:?}"),
    }
}

/// A body that inlines `f` and redefines it through an adapter checks the
/// epoch at the original call boundary, then resumes with the new definition.
///
///     (lambda (x new) (fset 'f new) (f x))   ; f = (lambda (y) (1+ y)) at compile time
///
/// Called with new = (lambda (y) (1- y)): GNU and the interpreter give 4.
#[test]
fn mir_inline_call_revalidates_after_fset_and_allows_state_reads() {
    use crate::emacs_core::eval::Context;
    use crate::emacs_core::intern::SymId;
    crate::emacs_core::jit::compile::force_profit_gate_for_test(false);
    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context as *mut u8;
    let f_sym = Value::symbol("jit-gate-redefined-f");
    let crate::emacs_core::value::ValueKind::Symbol(f_id) = f_sym.kind() else {
        panic!("symbol");
    };
    let unary = |op: Op| {
        let mut b = ByteCodeFunction::new(LambdaParams {
            required: vec![SymId(1)],
            optional: Vec::new(),
            rest: None,
        });
        b.lexical = true;
        b.ops = vec![Op::StackRef(0), op, Op::Return];
        b.max_stack = 16;
        Value::make_bytecode(b)
    };
    let inc = unary(Op::Add1);
    let dec = unary(Op::Sub1);
    ev.obarray.set_symbol_function_id(f_id, inc);
    let mut bar = ByteCodeFunction::new(LambdaParams {
        required: vec![SymId(2), SymId(3)],
        optional: Vec::new(),
        rest: None,
    });
    bar.lexical = true;
    bar.ops = vec![
        Op::Constant(0), // 'f           [x new f]
        Op::StackRef(1), // new          [x new f new]
        Op::Fset,        //              [x new r]
        Op::Pop,         //              [x new]
        Op::Constant(0), // 'f           [x new f]
        Op::StackRef(2), // x            [x new f x]
        Op::Call(1),     // (f x)        [x new r]
        Op::Return,
    ];
    bar.constants = vec![f_sym].into();
    bar.max_stack = 16;
    bar.seal_hand_assembled_ops();
    let _ = ev.debug_on_next_call_is_armed();
    let leaf = compile_bytecode_function_with(&bar, Some(&ev.obarray)).expect("compiles");
    assert_eq!(
        leaf.tier,
        super::leaf::LeafTier::Mir,
        "reentrant inlining has precise entry guards"
    );
    let result = leaf.call(ctx, &[Value::make_int(5), dec]);
    let NativeRun::DeoptAt(frame) = &result else {
        panic!("{result:?}")
    };
    assert_eq!(frame.pc, 6);
    assert_eq!(
        mir_inline_guards::resume(&mut ev, &bar, result),
        Value::make_int(4)
    );
    // A state read that leaves the dependency valid runs natively.
    ev.obarray.set_symbol_function_id(f_id, inc);
    let mut baz = ByteCodeFunction::new(LambdaParams {
        required: vec![SymId(2)],
        optional: Vec::new(),
        rest: None,
    });
    baz.lexical = true;
    baz.ops = vec![
        Op::VarRef(1),   // v            [x v]
        Op::Pop,         //              [x]
        Op::Constant(0), // 'f           [x f]
        Op::StackRef(1), // x            [x f x]
        Op::Call(1),     // (f x)        [x r]
        Op::Return,
    ];
    baz.constants = vec![f_sym, Value::symbol("jit-gate-some-var")].into();
    baz.max_stack = 16;
    ev.eval_str("(setq jit-gate-some-var 1)").unwrap();
    let leaf = compile_bytecode_function_with(&baz, Some(&ev.obarray)).expect("compiles");
    assert_eq!(
        leaf.tier,
        super::leaf::LeafTier::Mir,
        "state reads do not prevent guarded inlining"
    );
    assert_eq!(
        leaf.call(ctx, &[Value::make_int(5)]),
        NativeRun::Ok(Value::make_int(6).bits())
    );
}

/// A cons-only body has no rooting site (the cons shim is context-free), so
/// nothing is hoisted and it still runs with a null vmctx.
#[test]
fn mir_cons_only_body_does_not_hoist() {
    let ops = vec![Op::StackRef(1), Op::StackRef(1), Op::Cons, Op::Return];
    let mir = mir::build_mir(&ops, &[], 2).expect("MIR builds");
    let plan = super::lowering::plan_mir_leaf(&mir);
    assert!(plan.needs_rt && plan.rooting_sites == 0 && !plan.precise);
    let leaf = lower_mir_pure(&mir).expect("lowers");
    assert_eq!(
        super::lowering::rootwin_counters(),
        (0, 0),
        "no window, no stores"
    );
    let bits = leaf
        .call_for_test(&[Value::make_int(1), Value::make_int(2)])
        .expect("runs with a null vmctx");
    let c = Value::from_bits(bits);
    assert_eq!(c.cons_car().bits(), Value::make_int(1).bits());
    assert_eq!(c.cons_cdr().bits(), Value::make_int(2).bits());
}

/// The adapter: a variable read stays in the MIR tier, through the baseline's
/// `VarRef` arm — a bound special reads its value, an unbound one signals
/// `void-variable` through the shared signal exit, and the body is precise
/// (side-effecting) like any shim-bearing body.
#[test]
fn mir_adapter_lowers_a_variable_read() {
    use crate::emacs_core::error::Flow;
    use crate::emacs_core::eval::Context;
    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context as *mut u8;
    let sym = Value::symbol("jit-mir-adapter-var");
    let ops = vec![Op::VarRef(0), Op::Return];
    let mir = mir::build_mir(&ops, &[sym], 0).expect("MIR builds");
    let leaf = lower_mir_pure(&mir).expect("VarRef lowers via the adapter");
    assert!(leaf.has_side_effects, "an Opaque body is precise");
    assert_eq!(leaf.call(ctx, &[]), NativeRun::Signal, "unbound: signals");
    match take_pending_flow().expect("the signal is stashed") {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "void-variable"),
        other => panic!("expected void-variable, got {other:?}"),
    }
    ev.eval_str("(setq jit-mir-adapter-var 41)").expect("bind");
    match leaf.call(ctx, &[]) {
        NativeRun::Ok(bits) => assert_eq!(bits, Value::make_int(41).bits()),
        other => panic!("bound read: {other:?}"),
    }
}

/// A precise deopt AFTER a side-effecting Opaque resumes the interpreter past
/// it: `(setq special 7)` then `(1+ x)` — every guard fails under the forced-
/// deopt harness, and the resume must land at the `1+` (the assignment done
/// once), never rerun from the start.
#[test]
fn mir_adapter_deopt_after_a_varset_resumes_past_it() {
    use crate::emacs_core::eval::Context;
    // Every guard fails, for compiles on this thread only.
    crate::emacs_core::jit::compile::force_deopt_for_test(true);
    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context as *mut u8;
    let sym = Value::symbol("jit-mir-adapter-set");
    ev.eval_str("(setq jit-mir-adapter-set 0)").expect("init");
    // (lambda (x) (setq special 7) (1+ x)):
    //  0 Constant(0)=7; 1 VarSet(1); 2 StackRef(0); 3 Add1; 4 Return
    let ops = vec![
        Op::Constant(0),
        Op::VarSet(1),
        Op::StackRef(0),
        Op::Add1,
        Op::Return,
    ];
    let mir = mir::build_mir(&ops, &[Value::make_int(7), sym], 1).expect("MIR builds");
    let leaf = lower_mir_pure(&mir).expect("VarSet lowers via the adapter");
    match leaf.call(ctx, &[Value::make_int(5)]) {
        NativeRun::DeoptAt(resume) => {
            assert_eq!(resume.pc, 3, "resume AT the 1+, after the assignment");
            assert_eq!(
                resume.stack.iter().map(|v| v.bits()).collect::<Vec<_>>(),
                vec![Value::make_int(5).bits(), Value::make_int(5).bits()],
                "the framestate before the 1+: [x x]"
            );
        }
        other => panic!("expected a precise deopt at the 1+, got {other:?}"),
    }
    let v = ev.eval_str("jit-mir-adapter-set").expect("read");
    assert_eq!(
        v.bits(),
        Value::make_int(7).bits(),
        "the assignment ran once"
    );
}

/// The call-args scratch slot is sized to the widest operand set any Opaque
/// marshals, not to Call/Apply arity: a `List(5)` in a call-free body used to
/// store five words into a one-word slot.
#[test]
fn mir_adapter_sizes_the_args_slot_for_a_wide_list() {
    use crate::emacs_core::eval::Context;
    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context as *mut u8;
    let ops = vec![
        Op::StackRef(0),
        Op::StackRef(1),
        Op::StackRef(2),
        Op::StackRef(3),
        Op::StackRef(4),
        Op::List(5),
        Op::Return,
    ];
    let mir = mir::build_mir(&ops, &[], 1).expect("MIR builds");
    assert_eq!(super::lowering::plan_mir_leaf(&mir).max_call_args, 5);
    let leaf = lower_mir_pure(&mir).expect("List lowers via the adapter");
    let NativeRun::Ok(bits) = leaf.call(ctx, &[Value::make_int(9)]) else {
        panic!("(list a a a a a) must run natively");
    };
    let mut l = Value::from_bits(bits);
    for _ in 0..5 {
        assert!(l.is_cons(), "a 5-list");
        assert_eq!(l.cons_car().bits(), Value::make_int(9).bits());
        l = l.cons_cdr();
    }
    assert!(l.is_nil());
}

/// A cons live across an Opaque safepoint remains real, and the adapter's
/// residual rooting holds across a shim that REALLY collects:
///
///     (lambda (a b) (let ((p (cons (list a) b))) (garbage-collect) (make-list 4096 0) (car p)))
///
/// `p` is live across both builtin calls only through the residual stack.
/// `garbage-collect` is an exact collection (it ignores the native stack),
/// and the `make-list` reallocation reuses any cell it freed, so a missing
/// root reads back garbage instead of `(42)`.
#[test]
fn mir_adapter_keeps_conses_real_across_a_shim() {
    use crate::emacs_core::eval::Context;
    use crate::emacs_core::intern::intern;
    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context as *mut u8;
    let ops = vec![
        Op::StackRef(1),                                  // a          [a b a]
        Op::List(1),                                      // (a)        [a b (a)]
        Op::StackRef(1),                                  // b          [a b (a) b]
        Op::Cons,                                         // p          [a b p]
        Op::CallBuiltinSym(intern("garbage-collect"), 0), // residual [a b p]
        Op::Pop,                                          //            [a b p]
        Op::Constant(0),                                  // 4096
        Op::Constant(1),                                  // 0
        Op::CallBuiltinSym(intern("make-list"), 2),       // residual [a b p]
        Op::Pop,                                          //            [a b p]
        Op::Car,                                          // (car p)
        Op::Return,
    ];
    let constants = [Value::make_int(4096), Value::make_int(0)];
    let mir = mir::build_mir(&ops, &constants, 2).expect("MIR builds");
    let plan = super::lowering::plan_mir_leaf(&mir);
    assert!(plan.has_opaque && plan.precise);
    assert!(
        plan.cons_repl.iter().all(|c| c.is_none()),
        "a cons live across an Opaque safepoint cannot be elided"
    );
    let leaf = lower_mir_pure(&mir).expect("lowers via the adapter");
    for _ in 0..3 {
        let NativeRun::Ok(bits) = leaf.call(ctx, &[Value::make_int(42), Value::NIL]) else {
            panic!("must run natively");
        };
        let l = Value::from_bits(bits);
        assert!(l.is_cons(), "(car p) is the fresh list (got {l:?})");
        assert_eq!(
            l.cons_car(),
            Value::make_int(42),
            "car intact after the collection"
        );
        assert!(l.cons_cdr().is_nil(), "cdr intact");
    }
}

/// An INLINED predicate reads its own operand, not the call site's stack
/// tail: `(defun f (a b) (integerp a))` inlined at `(f s 5)` answers by `s`.
/// Through the production compile path (the inliner + the tier gate).
#[test]
fn mir_adapter_inlined_predicate_reads_its_own_operand() {
    use crate::emacs_core::eval::Context;
    use crate::emacs_core::intern::SymId;
    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context as *mut u8;
    let f_sym = Value::symbol("jit-mir-adapter-f");
    let crate::emacs_core::value::ValueKind::Symbol(f_id) = f_sym.kind() else {
        panic!("symbol");
    };
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: vec![SymId(1), SymId(2)],
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = vec![Op::StackRef(1), Op::Integerp, Op::Return];
    f.max_stack = 16;
    ev.obarray
        .set_symbol_function_id(f_id, Value::make_bytecode(f));
    let mut g = ByteCodeFunction::new(LambdaParams {
        required: vec![SymId(3)],
        optional: Vec::new(),
        rest: None,
    });
    g.lexical = true;
    // (lambda (s) (f s 5))
    g.ops = vec![
        Op::Constant(0),
        Op::StackRef(1),
        Op::Constant(1),
        Op::Call(2),
        Op::Return,
    ];
    g.constants = vec![f_sym, Value::make_int(5)].into();
    g.max_stack = 16;
    let leaf = compile_bytecode_function_with(&g, Some(&ev.obarray)).expect("g compiles");
    assert!(leaf.inline_epoch().is_some(), "f was inlined -> MIR tier");
    for (arg, want) in [
        (Value::symbol("jit-mir-not-an-int"), Value::NIL),
        (Value::make_int(3), Value::T),
        (Value::make_float(2.5), Value::NIL),
    ] {
        match leaf.call(ctx, &[arg]) {
            NativeRun::Ok(bits) => assert_eq!(bits, want.bits(), "(integerp {arg:?})"),
            other => panic!("(f {arg:?} 5): {other:?}"),
        }
    }
}

#[test]
fn bails_on_unsupported_op() {
    // MakeClosure (closure construction) is not in the supported subset ->
    // refuse, do not miscompile.
    let err = lower_nullary_leaf(
        &[Op::Nil, Op::Nil, Op::MakeClosure(0), Op::Nil, Op::Return],
        &[Value::NIL],
    )
    .unwrap_err();
    assert!(matches!(err, CompileError::UnsupportedOp("other")));
    // A Switch whose jump table is not a compile-time constant bails too
    // (the byte compiler always emits Constant(table) right before it).
    let err =
        lower_nullary_leaf(&[Op::Nil, Op::Nil, Op::Switch, Op::Nil, Op::Return], &[]).unwrap_err();
    assert!(matches!(err, CompileError::UnsupportedOp("switch-dynamic")));
}

#[test]
fn list_and_slice_builtins_run_natively() {
    use crate::emacs_core::print::print_value;
    let mut ev = crate::emacs_core::eval::Context::new_minimal_vm_harness();
    let ctx_ptr = &mut ev as *mut crate::emacs_core::eval::Context as *mut u8;

    // (list 1 2 3)
    let leaf = lower_nullary_leaf(
        &[
            Op::Constant(0),
            Op::Constant(1),
            Op::Constant(2),
            Op::List(3),
            Op::Return,
        ],
        &[Value::make_int(1), Value::make_int(2), Value::make_int(3)],
    )
    .expect("list body compiles");
    let NativeRun::Ok(bits) = leaf.call(ctx_ptr, &[]) else {
        panic!("native list failed");
    };
    assert_eq!(print_value(&Value::from_bits(bits)), "(1 2 3)");

    // (concat "foo" "bar")
    let leaf = lower_nullary_leaf(
        &[Op::Constant(0), Op::Constant(1), Op::Concat(2), Op::Return],
        &[Value::string("foo"), Value::string("bar")],
    )
    .expect("concat body compiles");
    let NativeRun::Ok(bits) = leaf.call(ctx_ptr, &[]) else {
        panic!("native concat failed");
    };
    assert_eq!(print_value(&Value::from_bits(bits)), "\"foobar\"");

    // (substring "hello" 1 3)
    let leaf = lower_nullary_leaf(
        &[
            Op::Constant(0),
            Op::Constant(1),
            Op::Constant(2),
            Op::Substring,
            Op::Return,
        ],
        &[
            Value::string("hello"),
            Value::make_int(1),
            Value::make_int(3),
        ],
    )
    .expect("substring body compiles");
    let NativeRun::Ok(bits) = leaf.call(ctx_ptr, &[]) else {
        panic!("native substring failed");
    };
    assert_eq!(print_value(&Value::from_bits(bits)), "\"el\"");

    // (nconc (list 1 2) (list 3)) — built natively end-to-end.
    let leaf = lower_nullary_leaf(
        &[
            Op::Constant(0),
            Op::Constant(1),
            Op::List(2),
            Op::Constant(2),
            Op::List(1),
            Op::Nconc,
            Op::Return,
        ],
        &[Value::make_int(1), Value::make_int(2), Value::make_int(3)],
    )
    .expect("nconc body compiles");
    let NativeRun::Ok(bits) = leaf.call(ctx_ptr, &[]) else {
        panic!("native nconc failed");
    };
    assert_eq!(print_value(&Value::from_bits(bits)), "(1 2 3)");

    // Signal path: (substring 5 0 1) is a wrong-type-argument.
    let leaf = lower_nullary_leaf(
        &[
            Op::Constant(0),
            Op::Constant(1),
            Op::Constant(2),
            Op::Substring,
            Op::Return,
        ],
        &[Value::make_int(5), Value::make_int(0), Value::make_int(1)],
    )
    .expect("substring body compiles");
    assert_eq!(leaf.call(ctx_ptr, &[]), NativeRun::Signal);
    let flow = take_pending_flow().expect("signal stashed");
    match flow {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "wrong-type-argument"),
        other => panic!("expected wrong-type-argument, got {other:?}"),
    }
}

#[test]
fn named_builtin_ops_run_natively() {
    // CallBuiltin/CallBuiltinSym need the full runtime's subr resolution
    // (covered by the eval_test seam differential); Aset's fast path runs
    // against the minimal harness.
    use crate::emacs_core::print::print_value;
    let mut ev = crate::emacs_core::eval::Context::new_minimal_vm_harness();
    let ctx_ptr = &mut ev as *mut crate::emacs_core::eval::Context as *mut u8;

    // Aset: mutate a constant vector natively, read back.
    let vec = Value::vector(vec![Value::make_int(0), Value::make_int(0)]);
    let leaf = lower_nullary_leaf(
        &[
            Op::Constant(0), // v
            Op::Constant(1), // 1
            Op::Constant(2), // 99
            Op::Aset,
            Op::Return,
        ],
        &[vec, Value::make_int(1), Value::make_int(99)],
    )
    .expect("aset body compiles");
    assert_eq!(
        leaf.call(ctx_ptr, &[]),
        NativeRun::Ok(Value::make_int(99).bits())
    );
    assert_eq!(print_value(&vec), "[0 99]");

    // Signal path: (aset 5 0 1) is a wrong-type-argument.
    let leaf = lower_nullary_leaf(
        &[
            Op::Constant(0),
            Op::Constant(1),
            Op::Constant(2),
            Op::Aset,
            Op::Return,
        ],
        &[Value::make_int(5), Value::make_int(0), Value::make_int(1)],
    )
    .expect("aset body compiles");
    assert_eq!(leaf.call(ctx_ptr, &[]), NativeRun::Signal);
    let _ = take_pending_flow().expect("signal stashed");
}

#[test]
fn cbsym_classifier_selects_shipset_by_name() {
    // `find_spec_sites` classifies CallBuiltinSym sites BY NAME, keyed at the
    // op's own index: the read set is Tier-A, every other plain builtin is
    // Tier-B (direct dispatch, GNU inline-opcode semantics), and only the
    // VM-owned special names stay generic.
    use crate::emacs_core::eval::Context;
    use crate::emacs_core::intern::intern;
    let ev = Context::new();
    let point = Op::CallBuiltinSym(intern("point"), 0);
    let insert = Op::CallBuiltinSym(intern("insert"), 1);
    let car = Op::CallBuiltinSym(intern("car"), 1); // plain builtin outside the old allowlist
    let gc = Op::CallBuiltinSym(intern("garbage-collect"), 0); // special name
    let goto = Op::CallBuiltinSym(intern("goto-char"), 1);
    let mbeg = Op::CallBuiltinSym(intern("match-beginning"), 1);
    let ops = [point, insert, car, gc, goto, mbeg, Op::Return];
    // The CBSym loop ignores `leaders`; pass the entry leader only.
    let sites = find_spec_sites(&ops, &[], &[0], &ev.obarray, false);
    assert_eq!(
        sites.get(&0).map(|s| s.kind),
        Some(SpecCalleeKind::CbsymTierA {
            which: CBSYM_A_POINT
        }),
        "point -> Tier-A read"
    );
    assert_eq!(
        sites.get(&1).map(|s| s.kind),
        Some(SpecCalleeKind::CbsymTierB),
        "insert -> Tier-B dispatch-skip"
    );
    assert_eq!(
        sites.get(&2).map(|s| s.kind),
        Some(SpecCalleeKind::CbsymTierB),
        "every plain builtin dispatches directly (car -> Tier-B)"
    );
    assert!(
        !sites.contains_key(&3),
        "garbage-collect is a dispatch_vm_builtin_unrooted special name"
    );
    assert_eq!(
        sites.get(&4).map(|s| s.kind),
        Some(SpecCalleeKind::CbsymTierB),
        "goto-char -> Tier-B"
    );
    assert_eq!(
        sites.get(&5).map(|s| s.kind),
        Some(SpecCalleeKind::CbsymTierA {
            which: CBSYM_A_MATCH_BEGINNING
        }),
        "match-beginning -> Tier-A (does a byte->char conversion; must delegate)"
    );
    // Every classified CBSym site reports `is_cbsym`; none report `is_round1_subr`.
    for idx in [0u32, 1, 2, 4, 5] {
        let k = sites[&(idx as usize)].kind;
        assert!(k.is_cbsym(), "{idx}: classified kind is CBSym");
        assert!(!k.is_round1_subr(), "{idx}: not an Op::Call subr kind");
    }
}

#[test]
fn cbsym_shipset_excludes_special_and_writeback_names() {
    // The `dispatch_vm_builtin_unrooted` special names + the writeback /
    // re-entrant names must NEVER classify: the fast shim funnels through
    // `funcall_general`, a DIFFERENT dispatch than the special-name arm, and
    // aset/fillarray carry a writeback protocol. Allowlist construction makes
    // this automatic; assert it functionally (a collision would classify one).
    use crate::emacs_core::eval::Context;
    use crate::emacs_core::intern::intern;
    let _ev = Context::new();
    for name in
        CBSYM_SPECIAL_NAMES
            .iter()
            .copied()
            .chain(["aset", "fillarray", "funcall", "apply", "eval"])
    {
        assert!(
            cbsym_spec_kind(intern(name), 0).is_none(),
            "excluded name {name:?} must never classify as a CBSym intrinsic"
        );
        assert!(
            cbsym_spec_kind(intern(name), 1).is_none(),
            "excluded name {name:?} (1-arg) must never classify"
        );
    }
}

#[test]
fn spec_sites_track_callee_through_computed_arguments() {
    // The abstract-stack widening: an Op::Call whose ARGUMENTS are computed
    // expressions still speculates, as long as the CALLEE slot provably
    // holds the constant symbol. The old trivial-push scan rejected any
    // intervening arithmetic — pinning fib-style self-recursion
    // (callee (- x 1)) to the generic shim forever.
    let (ev, sym_val) = harness_with_inc_callee("spec-computed-arg-callee");
    let consts = [sym_val, Value::make_int(1)];
    // (callee (- arg0 1)): Constant(sym); StackRef; Constant(1); Sub; Call(1)
    let ops = [
        Op::Constant(0),
        Op::StackRef(0),
        Op::Constant(1),
        Op::Sub,
        Op::Call(1),
        Op::Return,
    ];
    let sites = find_spec_sites(&ops, &consts, &[0], &ev.obarray, false);
    assert_eq!(
        sites.get(&4).map(|s| s.kind),
        Some(SpecCalleeKind::Bytecode),
        "computed-argument call must speculate on its constant callee"
    );
}

#[test]
fn spec_sites_track_both_calls_of_a_nested_call_argument() {
    // (callee (callee 5)): the inner call is an argument of the outer one;
    // BOTH callee slots hold the tracked constant, so both sites speculate
    // (the inner call's result correctly untags the arg slot, not the
    // outer callee's slot).
    let (ev, sym_val) = harness_with_inc_callee("spec-nested-call-callee");
    let consts = [sym_val, Value::make_int(5)];
    let ops = [
        Op::Constant(0),
        Op::Constant(0),
        Op::Constant(1),
        Op::Call(1),
        Op::Call(1),
        Op::Return,
    ];
    let sites = find_spec_sites(&ops, &consts, &[0], &ev.obarray, false);
    assert_eq!(
        sites.get(&3).map(|s| s.kind),
        Some(SpecCalleeKind::Bytecode),
        "inner call speculates"
    );
    assert_eq!(
        sites.get(&4).map(|s| s.kind),
        Some(SpecCalleeKind::Bytecode),
        "outer call speculates across the nested call"
    );
}

#[test]
fn spec_sites_reset_at_block_leaders() {
    // A block leader between the callee push and the call means the entry
    // stack is unknown — the tracker must forget the constant (values
    // reaching the call could come from another predecessor).
    let (ev, sym_val) = harness_with_inc_callee("spec-leader-reset-callee");
    let consts = [sym_val, Value::make_int(5)];
    let ops = [Op::Constant(0), Op::Constant(1), Op::Call(1), Op::Return];
    let sites = find_spec_sites(&ops, &consts, &[0, 2], &ev.obarray, false);
    assert!(
        !sites.contains_key(&2),
        "a leader between push and call must clear the tracking"
    );
}

/// `(lambda (a) (callee (if a 5 7)))`: the callee is pushed before the
/// conditional, so it reaches the call through a join. Every predecessor of
/// the join agrees on it, so the JIT (`cross_block`) keeps the site; the
/// block-local scan does not.
fn cross_block_call_ops() -> [Op; 8] {
    [
        Op::Constant(0),  // callee        [a callee]
        Op::StackRef(1),  // a             [a callee a]
        Op::GotoIfNil(5), //               [a callee]
        Op::Constant(1),  // 5
        Op::Goto(6),
        Op::Constant(2), // 7
        Op::Call(1),
        Op::Return,
    ]
}

#[test]
fn spec_sites_keep_a_callee_every_predecessor_agrees_on() {
    let (ev, sym_val) = harness_with_inc_callee("spec-cross-block-callee");
    let consts = [sym_val, Value::make_int(5), Value::make_int(7)];
    let ops = cross_block_call_ops();
    let leaders = [0, 3, 5, 6];
    let sites = find_spec_sites(&ops, &consts, &leaders, &ev.obarray, true);
    assert_eq!(
        sites.get(&6).map(|s| s.kind),
        Some(SpecCalleeKind::Bytecode),
        "both arms carry the callee to the call"
    );
    let block_local = find_spec_sites(&ops, &consts, &leaders, &ev.obarray, false);
    assert!(!block_local.contains_key(&6));

    // The arms push different callees: the join knows neither.
    let other = Value::symbol("spec-cross-block-other");
    let consts = [sym_val, Value::make_int(5), other];
    let ops = [
        Op::StackRef(0),  // a        [a a]
        Op::GotoIfNil(4), //          [a]
        Op::Constant(0),  // callee   [a callee]
        Op::Goto(5),
        Op::Constant(2), // other    [a other]
        Op::Constant(1), // 5
        Op::Call(1),
        Op::Return,
    ];
    let sites = find_spec_sites(&ops, &consts, &[0, 2, 4, 5], &ev.obarray, true);
    assert!(
        !sites.contains_key(&6),
        "predecessors that disagree on the callee must not speculate"
    );
}

fn cross_block_call_function(sym_val: Value) -> ByteCodeFunction {
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: vec![crate::emacs_core::intern::SymId(1)],
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = cross_block_call_ops().to_vec();
    f.constants = vec![sym_val, Value::make_int(5), Value::make_int(7)].into();
    f.max_stack = 8;
    f
}

/// The site selected across the join runs speculated (its callee's bits
/// checked at run time), and with that check forced to miss it runs the
/// plain generic call to the same answer.
#[test]
fn a_cross_block_call_site_speculates_behind_a_callee_check() {
    crate::emacs_core::jit::compile::force_profit_gate_for_test(false);
    let (mut ev, sym_val) = harness_with_inc_callee("spec-cross-block-run");
    let f = cross_block_call_function(sym_val);
    let ctx_ptr = &mut ev as *mut crate::emacs_core::eval::Context as *mut u8;
    for force_miss in [false, true] {
        super::lowering::force_spec_guard_miss_for_test(force_miss);
        let leaf = compile_bytecode_function_with(&f, Some(&ev.obarray)).expect("compiles");
        let before = super::shims::SPEC_CALL_COUNT.load(std::sync::atomic::Ordering::Relaxed);
        assert_eq!(
            leaf.call(ctx_ptr, &[Value::T]),
            NativeRun::Ok(Value::make_int(6).bits())
        );
        assert_eq!(
            leaf.call(ctx_ptr, &[Value::NIL]),
            NativeRun::Ok(Value::make_int(8).bits())
        );
        let spec_calls =
            super::shims::SPEC_CALL_COUNT.load(std::sync::atomic::Ordering::Relaxed) - before;
        if force_miss {
            assert_eq!(spec_calls, 0, "a missed check takes the generic call");
        } else {
            assert_eq!(spec_calls, 2, "both calls take the speculated path");
        }
    }
    super::lowering::force_spec_guard_miss_for_test(false);
}

/// The generic call a missed callee check enters skips every root-window
/// store the speculated path made, so it may not elide against them. A
/// `setq` stores `[a b]`; `a`'s slot then gets a fresh `c`; the guarded call's
/// speculated path stores `[c b]`. On the miss path slot 0 still holds `a`,
/// so its generic call must store `c` again rather than trust that record.
///
///     (lambda (a b) (setq v v) (let ((c (cons a a))) (setq a c) (callee (if c 5 7)) b))
#[test]
fn a_missed_callee_check_does_not_elide_the_speculated_path_stores() {
    crate::emacs_core::jit::compile::force_profit_gate_for_test(false);
    let (mut ev, sym_val) = harness_with_inc_callee("spec-cross-block-carry");
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: vec![
            crate::emacs_core::intern::SymId(1),
            crate::emacs_core::intern::SymId(2),
        ],
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = vec![
        Op::Constant(3),   // v             [a b v]
        Op::VarSet(3),     // stores [a b]  [a b]
        Op::StackRef(1),   // a             [a b a]
        Op::Dup,           //               [a b a a]
        Op::Cons,          // c             [a b c]
        Op::StackSet(2),   //               [c b]
        Op::Constant(0),   // callee        [c b callee]
        Op::StackRef(2),   // c             [c b callee c]
        Op::GotoIfNil(11), //               [c b callee]
        Op::Constant(1),   // 5
        Op::Goto(12),
        Op::Constant(2), // 7
        Op::Call(1),     // residual [c b]
        Op::Pop,
        Op::Return, // b
    ];
    f.constants = vec![
        sym_val,
        Value::make_int(5),
        Value::make_int(7),
        Value::symbol("spec-cross-block-carry-v"),
    ]
    .into();
    f.max_stack = 16;
    for baseline in [true, false] {
        let compile = || {
            if baseline {
                lower_leaf_full(&f.ops, &f.constants, 2, None, Some(&ev.obarray), 0)
            } else {
                compile_bytecode_function_with(&f, Some(&ev.obarray))
            }
        };
        let leaf = compile().expect("compiles");
        assert_eq!(
            leaf.tier,
            if baseline {
                super::leaf::LeafTier::Baseline
            } else {
                super::leaf::LeafTier::Mir
            }
        );
        assert_eq!(
            super::lowering::rootwin_counters(),
            (6, 0),
            "a+b, the speculated path's c+b (the call is at a join, which forgets the \
         record), the miss path's c+b; (4, 2) means the miss path trusted stores \
         only the speculated path made"
        );
        // The call is the guarded kind: with its check forced to miss, it runs
        // unspeculated.
        super::lowering::force_spec_guard_miss_for_test(true);
        let missing = compile().expect("compiles");
        super::lowering::force_spec_guard_miss_for_test(false);
        let ctx_ptr = &mut ev as *mut crate::emacs_core::eval::Context as *mut u8;
        let before = super::shims::SPEC_CALL_COUNT.load(std::sync::atomic::Ordering::Relaxed);
        let args = [Value::make_int(1), Value::make_int(2)];
        assert_eq!(
            missing.call(ctx_ptr, &args),
            NativeRun::Ok(Value::make_int(2).bits())
        );
        assert_eq!(
            super::shims::SPEC_CALL_COUNT.load(std::sync::atomic::Ordering::Relaxed),
            before
        );
        assert_eq!(
            leaf.call(ctx_ptr, &args),
            NativeRun::Ok(Value::make_int(2).bits())
        );
        assert_eq!(
            super::shims::SPEC_CALL_COUNT.load(std::sync::atomic::Ordering::Relaxed),
            before + 1
        );
    }
}

#[test]
fn spec_sites_respect_stackset_clobbering_the_callee_slot() {
    // StackSet overwrites the tracked callee slot with a computed value:
    // speculating here would call the WRONG function. The tracker must
    // model the in-place write.
    let (ev, sym_val) = harness_with_inc_callee("spec-stackset-clobber-callee");
    let consts = [sym_val, Value::make_int(5)];
    // [sym 5 nil] -> StackSet(2) moves nil into the callee slot -> [nil 5]
    let ops = [
        Op::Constant(0),
        Op::Constant(1),
        Op::Nil,
        Op::StackSet(2),
        Op::Call(1),
        Op::Return,
    ];
    let sites = find_spec_sites(&ops, &consts, &[0], &ev.obarray, false);
    assert!(
        !sites.contains_key(&4),
        "a clobbered callee slot must not speculate"
    );
}

#[test]
fn cbsym_intrinsic_ops_no_longer_veto_profitability() {
    // R2 COMMIT 3: an intrinsifiable CallBuiltinSym op no longer counts as a
    // call in `body_is_jit_profitable`, so a buffer-op-heavy loop that USED
    // to be NotProfitable (calls > arith) now tiers. A genuine call still
    // vetoes; a special-name CBSym (generic path) still counts.
    use crate::emacs_core::eval::Context;
    use crate::emacs_core::intern::intern;
    let _ev = Context::new(); // populate the subr table for cbsym_spec_kind
    force_profit_gate_for_test(true);
    force_gate_relax_for_test(false); // pin default (env-independent); Op::Call vetoes
    let point = Op::CallBuiltinSym(intern("point"), 0); // Tier-A eligible
    let goto = Op::CallBuiltinSym(intern("goto-char"), 1); // Tier-B eligible
    // Before the re-weight: calls=2 arith=0 -> NotProfitable. Now the two
    // intrinsifiable CBSym ops drop out of the call count -> profitable.
    assert!(
        body_is_jit_profitable(&[point, Op::Pop, goto, Op::Return], &[]),
        "an intrinsifiable buffer-op body now tiers"
    );
    // A genuine Op::Call (no arithmetic) still vetoes.
    assert!(
        !body_is_jit_profitable(&[Op::Constant(0), Op::Call(0), Op::Return], &[]),
        "a real call-dominated body still declines"
    );
    // A VM-owned special (`garbage-collect`) stays on the generic path, so it
    // still counts.
    assert!(
        !body_is_jit_profitable(
            &[Op::CallBuiltinSym(intern("garbage-collect"), 0), Op::Return],
            &[]
        ),
        "a generic-path CBSym still counts as a call"
    );
}

#[test]
fn gate_relax_lets_user_call_heavy_bodies_tier() {
    // NEOVM_JIT_GATE_RELAX: user-function Op::Call/Apply stop vetoing (measured
    // 2.31x net-positive tiered), while builtin calls stay counted (real
    // builtin-heavy = neutral, e.g. font-lock). Default OFF preserves
    // `calls <= arith`.
    use crate::emacs_core::eval::Context;
    let _ev = Context::new(); // subr table for cbsym_spec_kind
    force_profit_gate_for_test(true);
    // 4 user calls, 0 arith.
    let user_call_heavy = [
        Op::Constant(0),
        Op::Call(0),
        Op::Constant(0),
        Op::Call(0),
        Op::Constant(0),
        Op::Call(0),
        Op::Constant(0),
        Op::Call(0),
        Op::Return,
    ];
    // 4 name-based builtin calls (`Op::CallBuiltin`, the override-aware op),
    // 0 arith — the font-lock shape. GNU inline opcodes (`Op::CallBuiltinSym`)
    // dispatch their primitive directly and no longer count as calls.
    let builtin_heavy = [
        Op::CallBuiltin(0, 1),
        Op::CallBuiltin(0, 1),
        Op::CallBuiltin(0, 1),
        Op::CallBuiltin(0, 1),
        Op::Return,
    ];

    // Default (relax OFF): both decline (calls > arith), unchanged behavior.
    force_gate_relax_for_test(false);
    assert!(
        !body_is_jit_profitable(&user_call_heavy, &[]),
        "relax OFF: user-call-heavy still declines (unchanged)"
    );
    assert!(
        !body_is_jit_profitable(&builtin_heavy, &[]),
        "relax OFF: builtin-heavy declines"
    );

    // Relax ON: user calls no longer veto; builtin calls still do.
    force_gate_relax_for_test(true);
    assert!(
        body_is_jit_profitable(&user_call_heavy, &[]),
        "relax ON: user-call-heavy now tiers (measured 2.31x net-positive)"
    );
    assert!(
        !body_is_jit_profitable(&builtin_heavy, &[]),
        "relax ON: builtin-call-heavy still declines (font-lock ~1.0x, correctly declined)"
    );
    force_gate_relax_for_test(false);
}

/// A `pcase`-shaped body keeps its known-fixnum elision: the switch's edges
/// carry the entry set to every arm. Unmodelled, `Op::Switch` made
/// `compute_known_fixnum_slots` bail for the WHOLE function, so no guard
/// anywhere in a body with a jump table was elided.
#[test]
fn a_switch_carries_known_fixnum_slots_to_its_arms() {
    use crate::emacs_core::value::HashTableTest;
    let mut ev = crate::emacs_core::eval::Context::new_minimal_vm_harness();
    let ctx_ptr = &mut ev as *mut crate::emacs_core::eval::Context as *mut u8;
    let table = Value::hash_table(HashTableTest::Eq);
    let _ = table.with_hash_table_mut(|ht| {
        let key = Value::symbol("jit-sw2-foo").to_hash_key(&ht.test);
        ht.insert(key, Value::symbol("jit-sw2-foo"), Value::fixnum(16));
    });
    // (let ((k (1+ x))) (pcase 'foo (...)) (1+ k)) in both arms: only the
    // first `1+` needs a fixnum guard.
    let ops = [
        Op::StackRef(0), // [x x]
        Op::Add1,        // [x k]        guard on x
        Op::Constant(1), // [x k 'foo]
        Op::Constant(0), // [x k 'foo table]
        Op::Switch,      // [x k]        -> 8 on a hit, 5 on a miss
        Op::StackRef(0), // 5: [x k k]
        Op::Add1,        // [x k k+1]    k is known fixnum: no guard
        Op::Return,
        Op::StackRef(0), // 8: [x k k]
        Op::Add1,        // [x k k+1]    no guard
        Op::Return,
    ];
    let constants = [table, Value::symbol("jit-sw2-foo")];
    let map = vec![GnuByteOffsetMapEntry::new(16, 8)];
    // The analysis itself: every block's entry set exists (an unmodelled op
    // would make the whole map empty) and both arms know slot 1 is a fixnum.
    let cfg = super::analyze_cfg(&ops, &constants, Some(&map), 1).expect("cfg");
    let known = super::compute_known_fixnum_slots(&ops, &constants, &cfg);
    assert!(
        !known.is_empty(),
        "a switch must not bail the known-fixnum analysis"
    );
    for arm in [5usize, 8] {
        assert_eq!(
            known.get(&arm).map(|k| k.as_slice()),
            Some([false, true].as_slice()),
            "arm at {arm}: [x unknown, k fixnum]"
        );
    }
    let leaf = lower_leaf_with_map(&ops, &constants, 1, Some(&map)).expect("compiles");
    // The answers still match the interpreter, for a fixnum and for a
    // non-fixnum argument (which must deopt, not compute).
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: vec![crate::emacs_core::intern::SymId(1)],
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = ops.to_vec();
    f.constants = constants.to_vec().into();
    f.max_stack = 8;
    f.gnu_byte_offset_map = Some(map.clone());
    for arg in [Value::make_int(5), Value::make_int(-3)] {
        let native = leaf.call(ctx_ptr, &[arg]);
        let interp = {
            let mut vm = crate::emacs_core::bytecode::Vm::from_context(&mut ev);
            vm.execute(&f, vec![arg]).expect("interprets")
        };
        assert_eq!(native, NativeRun::Ok(interp.bits()), "arg {arg:?}");
    }
    assert!(
        matches!(
            leaf.call(ctx_ptr, &[Value::symbol("x")]),
            NativeRun::Deopt | NativeRun::DeoptAt(_)
        ),
        "a non-fixnum argument deopts at the first guard"
    );
}

#[test]
fn switch_jump_table_dispatches_natively() {
    // Mirror vm_switch_branches_using_hash_table_jump_table: a constant
    // eq jump table {foo -> byte offset 8} resolving through the GNU
    // byte-offset map to instruction 5. Hit -> 20, miss -> 10.
    use crate::emacs_core::value::HashTableTest;
    let mut ev = crate::emacs_core::eval::Context::new_minimal_vm_harness();
    let ctx_ptr = &mut ev as *mut crate::emacs_core::eval::Context as *mut u8;
    let table = Value::hash_table(HashTableTest::Eq);
    let _ = table.with_hash_table_mut(|ht| {
        let key = Value::symbol("jit-sw-foo").to_hash_key(&ht.test);
        ht.insert(key, Value::symbol("jit-sw-foo"), Value::fixnum(8));
    });
    let map = vec![GnuByteOffsetMapEntry::new(8, 5)];
    let leaf = lower_leaf_with_map(
        &[
            Op::StackRef(0), // [x x]
            Op::Constant(0), // [x x table]
            Op::Switch,      // [x], jump or fall through
            Op::Constant(1), // miss: 10
            Op::Return,
            Op::Constant(2), // 5: hit: 20
            Op::Return,
        ],
        &[table, Value::make_int(10), Value::make_int(20)],
        1,
        Some(&map),
    )
    .expect("switch body compiles");
    let hit = leaf.call(ctx_ptr, &[Value::symbol("jit-sw-foo")]);
    assert_eq!(hit, NativeRun::Ok(Value::make_int(20).bits()));
    let miss = leaf.call(ctx_ptr, &[Value::symbol("jit-sw-bar")]);
    assert_eq!(miss, NativeRun::Ok(Value::make_int(10).bits()));
}

/// `(lambda (x) (switch x TABLE))` lowered natively: a miss answers 10, byte
/// offset 8 (instruction 5) answers 20, byte offset 12 (instruction 7) 30.
fn lower_switch_on_arg(table: Value) -> CompiledLeaf {
    let map = vec![
        GnuByteOffsetMapEntry::new(8, 5),
        GnuByteOffsetMapEntry::new(12, 7),
    ];
    lower_leaf_with_map(
        &[
            Op::StackRef(0), // [x x]
            Op::Constant(0), // [x x table]
            Op::Switch,      // [x], jump or fall through
            Op::Constant(1), // miss: 10
            Op::Return,
            Op::Constant(2), // 5: 20
            Op::Return,
            Op::Constant(3), // 7: 30
            Op::Return,
        ],
        &[
            table,
            Value::make_int(10),
            Value::make_int(20),
            Value::make_int(30),
        ],
        1,
        Some(&map),
    )
    .expect("switch body compiles")
}

/// A `pcase` backquote dispatch: an `equal` jump table keyed by conses,
/// answered natively through the table's switch plan.
#[test]
fn switch_equal_cons_table_dispatches_natively() {
    use crate::emacs_core::value::HashTableTest;
    let mut ev = crate::emacs_core::eval::Context::new_minimal_vm_harness();
    let ctx_ptr = &mut ev as *mut crate::emacs_core::eval::Context as *mut u8;
    let (a, b) = (
        Value::symbol("jit-sw-cons-a"),
        Value::symbol("jit-sw-cons-b"),
    );
    let table = Value::hash_table(HashTableTest::Equal);
    let _ = table.with_hash_table_mut(|ht| {
        for (key, offset) in [(Value::list(vec![a, b]), 8), (Value::list(vec![a]), 12)] {
            ht.insert(key.to_hash_key(&ht.test), key, Value::fixnum(offset));
        }
    });
    let leaf = lower_switch_on_arg(table);
    let answer = |arg: Value| match leaf.call(ctx_ptr, &[arg]) {
        NativeRun::Ok(bits) => Value::from_bits(bits).as_fixnum(),
        other => panic!("expected a native answer for {arg:?}, got {other:?}"),
    };
    for _ in 0..4 {
        assert_eq!(answer(Value::list(vec![a, b])), Some(20));
        assert_eq!(answer(Value::list(vec![a])), Some(30));
        assert_eq!(answer(Value::list(vec![a, b, a])), Some(10));
        assert_eq!(answer(Value::cons(a, b)), Some(10));
        assert_eq!(answer(a), Some(10));
        assert_eq!(answer(Value::make_int(7)), Some(10));
    }
    assert!(
        table
            .as_hash_table()
            .expect("a hash table")
            .data
            .switch_plan
            .shape()
            .is_some(),
        "native dispatch planned the jump table"
    );
}

/// A jump table mutated after the function was compiled: a new key whose
/// target is in the compiled target set is followed; a target outside it,
/// or one that is not a byte offset, raises the stale-table signal exactly
/// as before the table had a plan.
#[test]
fn switch_follows_mutation_after_compile() {
    use crate::emacs_core::value::HashTableTest;
    let mut ev = crate::emacs_core::eval::Context::new_minimal_vm_harness();
    let ctx_ptr = &mut ev as *mut crate::emacs_core::eval::Context as *mut u8;
    let [a, b, c, d, e] =
        ["a", "b", "c", "d", "e"].map(|n| Value::symbol(format!("jit-sw-mut-{n}")));
    let table = Value::hash_table(HashTableTest::Eq);
    let _ = table.with_hash_table_mut(|ht| {
        for (key, offset) in [(a, 8), (b, 12)] {
            ht.insert(key.to_hash_key(&ht.test), key, Value::fixnum(offset));
        }
    });
    let leaf = lower_switch_on_arg(table);
    let put = |key: Value, target: Value| {
        let _ = table.with_hash_table_mut(|ht| {
            ht.insert(key.to_hash_key(&ht.test), key, target);
        });
    };
    for _ in 0..3 {
        assert_eq!(
            leaf.call(ctx_ptr, &[a]),
            NativeRun::Ok(Value::make_int(20).bits())
        );
        assert_eq!(
            leaf.call(ctx_ptr, &[c]),
            NativeRun::Ok(Value::make_int(10).bits())
        );
    }
    put(c, Value::fixnum(12));
    assert_eq!(
        leaf.call(ctx_ptr, &[c]),
        NativeRun::Ok(Value::make_int(30).bits()),
        "a new key targeting a compiled offset"
    );
    put(a, Value::fixnum(12));
    assert_eq!(
        leaf.call(ctx_ptr, &[a]),
        NativeRun::Ok(Value::make_int(30).bits()),
        "a retargeted key"
    );
    let stale = |arg: Value| {
        assert_eq!(leaf.call(ctx_ptr, &[arg]), NativeRun::Signal, "{arg:?}");
        match take_pending_flow().expect("the stale-table signal is stashed") {
            crate::emacs_core::error::Flow::Signal(sig) => {
                assert_eq!(sig.symbol_name(), "error");
            }
            other => panic!("expected an error signal, got {other:?}"),
        }
    };
    put(d, Value::fixnum(16));
    stale(d);
    put(e, Value::symbol("jit-sw-mut-not-an-offset"));
    stale(e);
    assert_eq!(
        leaf.call(ctx_ptr, &[b]),
        NativeRun::Ok(Value::make_int(30).bits()),
        "untouched keys still dispatch"
    );
}

#[test]
fn handler_analysis_bails_on_unbalanced_pophandler() {
    // PopHandler with no statically active handler frame.
    let err = lower_nullary_leaf(&[Op::PopHandler, Op::Nil, Op::Return], &[]).unwrap_err();
    assert!(matches!(
        err,
        CompileError::UnsupportedOp("unbalanced-pophandler")
    ));
}

#[test]
fn handler_body_compiles_and_runs_catch_throw_natively() {
    // (catch 'tag (throw 'tag 42)) — the throw is caught by this same
    // frame's PushCatch via the match shim, natively.
    let mut ev = crate::emacs_core::eval::Context::new_minimal_vm_harness();
    let ctx_ptr = &mut ev as *mut crate::emacs_core::eval::Context as *mut u8;
    let tag = Value::symbol("jit-unit-tag");
    let leaf = lower_nullary_leaf(
        &[
            Op::Constant(0),  // 'tag
            Op::PushCatch(5), // frame, handler target 5
            Op::Constant(0),  // 'tag
            Op::Constant(1),  // 42
            Op::Throw,
            Op::Return, // 5: handler entry [thrown]
        ],
        &[tag, Value::make_int(42)],
    )
    .expect("handler body compiles");
    let base = ev.condition_stack.len();
    match leaf.call(ctx_ptr, &[]) {
        NativeRun::Ok(bits) => {
            assert_eq!(Value::from_bits(bits), Value::make_int(42));
        }
        other => panic!("expected native catch, got {other:?}"),
    }
    assert_eq!(ev.condition_stack.len(), base, "frame popped by the catch");
}

#[test]
fn handler_frames_unwound_on_propagation() {
    // (catch 'a (throw 'b 1)) — no frame matches: the flow propagates as
    // STATUS_SIGNAL (no-catch) and our registered frame is unwound.
    let mut ev = crate::emacs_core::eval::Context::new_minimal_vm_harness();
    let ctx_ptr = &mut ev as *mut crate::emacs_core::eval::Context as *mut u8;
    let leaf = lower_nullary_leaf(
        &[
            Op::Constant(0),  // 'a
            Op::PushCatch(5), // frame, handler target 5
            Op::Constant(1),  // 'b
            Op::Constant(2),  // 1
            Op::Throw,
            Op::Return, // 5: handler (reachable only via the frame)
        ],
        &[
            Value::symbol("jit-unit-a"),
            Value::symbol("jit-unit-b"),
            Value::make_int(1),
        ],
    )
    .expect("handler body compiles");
    let base = ev.condition_stack.len();
    assert_eq!(leaf.call(ctx_ptr, &[]), NativeRun::Signal);
    let flow = take_pending_flow().expect("no-catch flow stashed");
    match flow {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "no-catch"),
        other => panic!("expected no-catch signal, got {other:?}"),
    }
    assert_eq!(ev.condition_stack.len(), base, "frames unwound");
}

#[test]
fn compiles_varref_and_varset() {
    let mut ev = crate::emacs_core::eval::Context::new_minimal_vm_harness();
    let ctx_ptr = &mut ev as *mut crate::emacs_core::eval::Context as *mut u8;
    let var = Value::symbol("jit-test-dynvar");
    let crate::emacs_core::value::ValueKind::Symbol(var_id) = var.kind() else {
        panic!("symbol expected");
    };
    ev.obarray.set_symbol_value_id(var_id, Value::make_int(33));

    // VarRef reads the live value.
    let read = lower_nullary_leaf(&[Op::VarRef(0), Op::Return], &[var]).unwrap();
    assert_eq!(
        read.call(ctx_ptr, &[]),
        NativeRun::Ok(Value::make_int(33).bits())
    );

    // VarSet stores; read back through the runtime.
    let write = lower_nullary_leaf(
        &[Op::Constant(1), Op::VarSet(0), Op::Nil, Op::Return],
        &[var, Value::make_int(44)],
    )
    .unwrap();
    assert_eq!(write.call(ctx_ptr, &[]), NativeRun::Ok(Value::NIL.bits()));
    assert_eq!(
        read.call(ctx_ptr, &[]),
        NativeRun::Ok(Value::make_int(44).bits()),
        "VarSet must be visible to a subsequent VarRef"
    );

    // Reading an unbound variable signals (void-variable) -> Signal.
    let unbound = Value::symbol("jit-test-unbound-var");
    let bad = lower_nullary_leaf(&[Op::VarRef(0), Op::Return], &[unbound]).unwrap();
    assert_eq!(bad.call(ctx_ptr, &[]), NativeRun::Signal);
    assert!(take_pending_flow().is_some());
}

#[test]
fn compiles_varbind_unbind_with_full_unwind_semantics() {
    use crate::emacs_core::bytecode::Vm;
    let mut ev = crate::emacs_core::eval::Context::new_minimal_vm_harness();
    let ctx_ptr = &mut ev as *mut crate::emacs_core::eval::Context as *mut u8;
    let var = Value::symbol("jit-test-bind-var");
    let crate::emacs_core::value::ValueKind::Symbol(var_id) = var.kind() else {
        panic!("symbol expected");
    };
    ev.obarray.set_symbol_value_id(var_id, Value::make_int(99));
    let read = lower_nullary_leaf(&[Op::VarRef(0), Op::Return], &[var]).unwrap();
    let global_now = |ev: &mut crate::emacs_core::eval::Context| {
        let p = ev as *mut crate::emacs_core::eval::Context as *mut u8;
        match read.call(p, &[]) {
            NativeRun::Ok(bits) => Value::from_bits(bits),
            other => panic!("global read failed: {other:?}"),
        }
    };

    // Balanced let: bind 5, read it, unbind, return. Matches the
    // interpreter on the same body.
    let ops = [
        Op::Constant(1), // 5
        Op::VarBind(0),
        Op::VarRef(0),
        Op::Unbind(1),
        Op::Return,
    ];
    let consts = [var, Value::make_int(5)];
    let balanced = lower_nullary_leaf(&ops, &consts).unwrap();
    assert_eq!(
        balanced.call(ctx_ptr, &[]),
        NativeRun::Ok(Value::make_int(5).bits())
    );
    assert_eq!(global_now(&mut ev), Value::make_int(99), "binding popped");
    let interp = {
        let mut f = ByteCodeFunction::new(LambdaParams {
            required: Vec::new(),
            optional: Vec::new(),
            rest: None,
        });
        f.lexical = true;
        f.ops = ops.to_vec();
        f.constants = consts.to_vec().into();
        f.max_stack = 16;
        let mut vm = Vm::from_context(&mut ev);
        vm.execute(&f, vec![]).expect("interp runs let")
    };
    assert_eq!(interp, Value::make_int(5), "interpreter agrees");
    assert_eq!(global_now(&mut ev), Value::make_int(99));

    // Early return with the binding still active: the frame unwind must
    // restore the global (cleanup_bytecode_frame parity).
    let early = lower_nullary_leaf(
        &[Op::Constant(1), Op::VarBind(0), Op::True, Op::Return],
        &consts,
    )
    .unwrap();
    assert_eq!(early.call(ctx_ptr, &[]), NativeRun::Ok(Value::T.bits()));
    assert_eq!(
        global_now(&mut ev),
        Value::make_int(99),
        "early return must unwind the dangling binding"
    );

    // Signal inside the dynamic extent: the binding must also unwind.
    let sig = lower_nullary_leaf(
        &[
            Op::Constant(1),
            Op::VarBind(0),
            Op::Constant(2), // undefined function symbol
            Op::Call(0),
            Op::Return,
        ],
        &[
            var,
            Value::make_int(5),
            Value::symbol("jit-bind-no-such-fn"),
        ],
    )
    .unwrap();
    assert_eq!(sig.call(ctx_ptr, &[]), NativeRun::Signal);
    assert!(take_pending_flow().is_some());
    assert_eq!(
        global_now(&mut ev),
        Value::make_int(99),
        "signal must unwind the dangling binding"
    );
}

/// A compiled `let` of a plain global binds on the shim's swap-and-push
/// fast path with no root and no general dispatch. The symbol's shape is
/// read again when the binding is POPPED, as GNU's `do_one_unbind` does: a
/// watcher added inside the body, or a buffer-local made there, sends the
/// unbind to the general path, so the watcher sees `unlet` with the old
/// value and the global is restored; the fast pop never stores blind.
#[test]
fn a_compiled_let_reads_the_symbol_shape_again_when_it_pops() {
    let mut ev = crate::emacs_core::eval::Context::new();
    ev.eval_str(
        "(progn
           (setq jit-fastlet-var 1 jit-fastlet-log nil)
           (fset 'jit-fastlet-watch
                 (lambda (_symbol new-value operation _where)
                   (setq jit-fastlet-log
                         (cons (list operation new-value) jit-fastlet-log)))))",
    )
    .expect("setup");
    let ctx = &mut ev as *mut crate::emacs_core::eval::Context as *mut u8;
    // (let ((jit-fastlet-var 2))
    //   (add-variable-watcher 'jit-fastlet-var 'jit-fastlet-watch)
    //   jit-fastlet-var)
    let leaf = lower_nullary_leaf(
        &[
            Op::Constant(1), // 2
            Op::VarBind(0),
            Op::Constant(2), // add-variable-watcher
            Op::Constant(0), // 'jit-fastlet-var
            Op::Constant(3), // 'jit-fastlet-watch
            Op::Call(2),
            Op::DiscardN(1),
            Op::VarRef(0),
            Op::Unbind(1),
            Op::Return,
        ],
        &[
            Value::symbol("jit-fastlet-var"),
            Value::make_int(2),
            Value::symbol("add-variable-watcher"),
            Value::symbol("jit-fastlet-watch"),
        ],
    )
    .expect("compiles");
    let base = ev.specpdl.len();
    assert_eq!(
        leaf.call(ctx, &[]),
        NativeRun::Ok(Value::make_int(2).bits())
    );
    assert_eq!(ev.specpdl.len(), base, "the binding popped");
    assert_eq!(
        ev.eval_str("(list jit-fastlet-var jit-fastlet-log)")
            .map(|v| crate::emacs_core::print::print_value(&v))
            .expect("read"),
        "(1 ((unlet 1)))",
        "the watcher added inside the body saw the restore, once, with the old value"
    );
    // The same body again: the bind is now watched too (the swap refuses,
    // the general path binds and notifies), and the pop still notifies.
    assert_eq!(
        leaf.call(ctx, &[]),
        NativeRun::Ok(Value::make_int(2).bits())
    );
    assert_eq!(
        ev.eval_str("(list jit-fastlet-var jit-fastlet-log)")
            .map(|v| crate::emacs_core::print::print_value(&v))
            .expect("read"),
        "(1 ((unlet 1) (let 2) (unlet 1)))"
    );
}

#[test]
fn compiled_unbind_and_frame_exit_propagate_restore_watcher_signals() {
    fn install_restore_watcher(variable: &str) -> crate::emacs_core::eval::Context {
        let mut eval = crate::emacs_core::eval::Context::new();
        let source = format!(
            r#"(progn
                 (setq {variable} 9)
                 (fset 'jit-unbind-error-watcher
                       (lambda (_symbol _new-value operation _where)
                         (if (eq operation 'unlet)
                             (signal 'error '("restore"))
                           nil)))
                 (add-variable-watcher '{variable}
                                       'jit-unbind-error-watcher))"#
        );
        eval.eval_str(&source).expect("install restore watcher");
        eval
    }

    let variable = "jit-test-explicit-unbind-error";
    let mut explicit_ctx = install_restore_watcher(variable);
    let explicit_base = explicit_ctx.specpdl.len();
    let explicit_ptr = &mut explicit_ctx as *mut crate::emacs_core::eval::Context as *mut u8;
    let explicit = lower_nullary_leaf(
        &[
            Op::Constant(1),
            Op::VarBind(0),
            Op::True,
            Op::Unbind(1),
            Op::Return,
        ],
        &[Value::symbol(variable), Value::make_int(1)],
    )
    .expect("explicit unbind body compiles");
    assert_eq!(explicit.call(explicit_ptr, &[]), NativeRun::Signal);
    assert!(matches!(take_pending_flow(), Some(Flow::Signal(_))));
    assert_eq!(explicit_ctx.specpdl.len(), explicit_base);

    let variable = "jit-test-frame-unbind-error";
    let mut frame_ctx = install_restore_watcher(variable);
    let frame_base = frame_ctx.specpdl.len();
    let frame_ptr = &mut frame_ctx as *mut crate::emacs_core::eval::Context as *mut u8;
    let dangling = lower_nullary_leaf(
        &[Op::Constant(1), Op::VarBind(0), Op::True, Op::Return],
        &[Value::symbol(variable), Value::make_int(1)],
    )
    .expect("dangling binding body compiles");
    assert_eq!(dangling.call(frame_ptr, &[]), NativeRun::Signal);
    assert!(matches!(take_pending_flow(), Some(Flow::Signal(_))));
    assert_eq!(frame_ctx.specpdl.len(), frame_base);
}

#[test]
fn cleanup_flow_does_not_pop_an_outer_callers_handler() {
    use crate::emacs_core::eval::{ConditionFrame, ResumeTarget, SpecBinding};

    let mut ctx = crate::emacs_core::eval::Context::new();
    let outer_tag = Value::symbol("jit-test-outer-caller-tag");
    let local_tag = Value::symbol("jit-test-unmatched-local-tag");
    let inner_tag = Value::symbol("jit-test-inner-tag");

    // Model a caller-owned catch below two handlers owned by this native
    // leaf. The inner catch is selected by the original throw. Unwinding
    // it runs a cleanup that throws to the caller, so the resumed search
    // must pop only the one remaining leaf-local handler.
    ctx.push_condition_frame(ConditionFrame::Catch {
        tag: outer_tag,
        resume: ResumeTarget::InterpreterCatch,
    });
    ctx.push_condition_frame(ConditionFrame::Catch {
        tag: local_tag,
        resume: ResumeTarget::VmCatch {
            resume_id: 1,
            target: 10,
            stack_len: 0,
            spec_depth: 0,
            bind_stack_len: 0,
        },
    });
    ctx.push_condition_frame(ConditionFrame::Catch {
        tag: inner_tag,
        resume: ResumeTarget::VmCatch {
            resume_id: 2,
            target: 20,
            stack_len: 0,
            spec_depth: 0,
            bind_stack_len: 0,
        },
    });

    let quoted_outer = Value::list(vec![Value::symbol("quote"), outer_tag]);
    let cleanup_form = Value::list(vec![
        Value::symbol("throw"),
        quoted_outer,
        Value::make_int(42),
    ]);
    ctx.specpdl.push(SpecBinding::UnwindProtect {
        forms: Value::list(vec![cleanup_form]),
        lexenv: ctx.lexenv,
    });

    stash_pending_flow(Flow::throw(inner_tag, Value::make_int(1)));
    let mut out = 0i64;
    let ctx_ptr = &mut ctx as *mut crate::emacs_core::eval::Context as *mut u8;
    assert_eq!(neovm_jit_match_handler(ctx_ptr, 2, &mut out), -1);
    assert_eq!(ctx.condition_stack.len(), 1, "caller handler survives");
    assert_eq!(ctx.specpdl.len(), 0, "cleanup extent fully unwound");
    let flow = take_pending_flow().expect("cleanup throw propagated to caller");
    let Flow::Throw(thrown) = flow else {
        panic!("expected cleanup throw, got {flow:?}");
    };
    assert_eq!(thrown.tag, outer_tag);
    assert_eq!(thrown.value, Value::make_int(42));
}

#[test]
fn guard_after_varbind_and_unbalanced_unbind_bail() {
    // Precise deopt: a guard after a binding compiles (a failing guard
    // transfers the bind to the resumed interpreter frame).
    lower_nullary_leaf(
        &[
            Op::Constant(1),
            Op::VarBind(0),
            Op::Constant(1),
            Op::Add1,
            Op::Return,
        ],
        &[Value::symbol("jit-test-bind-poison"), Value::make_int(1)],
    )
    .expect("guard after a binding compiles under precise deopt");

    // Unbinding more than this function bound bails to the interpreter.
    let err = lower_nullary_leaf(&[Op::Unbind(1), Op::Nil, Op::Return], &[]).unwrap_err();
    assert!(matches!(
        err,
        CompileError::UnsupportedOp("unbalanced-unbind")
    ));
}

#[test]
fn guard_after_varset_compiles_and_runs() {
    // Precise deopt: a guard after an assignment compiles and runs; the
    // assignment is NOT replayed on a later deopt (resume is mid-frame).
    let mut ev = crate::emacs_core::eval::Context::new_minimal_vm_harness();
    let ctx_ptr = &mut ev as *mut crate::emacs_core::eval::Context as *mut u8;
    let leaf = lower_nullary_leaf(
        &[
            Op::Constant(1),
            Op::VarSet(0),
            Op::Constant(1),
            Op::Add1,
            Op::Return,
        ],
        &[Value::symbol("jit-test-poison-var"), Value::make_int(1)],
    )
    .expect("guard after an assignment compiles under precise deopt");
    assert_eq!(
        leaf.call(ctx_ptr, &[]),
        NativeRun::Ok(Value::make_int(2).bits())
    );
}

#[test]
fn compiles_fixnum_mul() {
    let mul = |a: i64, b: i64| {
        lower_nullary_leaf(
            &[Op::Constant(0), Op::Constant(1), Op::Mul, Op::Return],
            &[Value::make_int(a), Value::make_int(b)],
        )
        .unwrap()
        .call_for_test(&[])
    };
    assert_eq!(mul(6, 7), Some(Value::make_int(42).bits()));
    assert_eq!(mul(-6, 7), Some(Value::make_int(-42).bits()));
    assert_eq!(mul(0, 12345), Some(Value::make_int(0).bits()));
    // Product overflowing fixnum range -> deopt.
    assert_eq!(mul(Value::MOST_POSITIVE_FIXNUM, 2), None);
    assert_eq!(mul(1 << 40, 1 << 40), None); // 2^80, way out of range
}

#[test]
fn mul_non_fixnum_deopts() {
    let leaf = lower_nullary_leaf(
        &[Op::Constant(0), Op::Nil, Op::Mul, Op::Return],
        &[Value::make_int(5)],
    )
    .unwrap();
    assert_eq!(leaf.call_for_test(&[]), None);
}

#[test]
fn compiles_type_predicates() {
    // Inspects only tag bits; never dereferences, so heap values needn't be
    // kept alive (no GC safepoint in the JIT call).
    fn pred(op: Op, v: Value) -> Option<usize> {
        lower_nullary_leaf(&[Op::Constant(0), op, Op::Return], &[v])
            .unwrap()
            .call_for_test(&[])
    }
    let t = Some(Value::T.bits());
    let nil = Some(Value::NIL.bits());
    let cons = Value::cons(Value::make_int(1), Value::make_int(2));
    let s = Value::string("hi");

    // null / not: only nil is null; fixnum 0 is NOT nil.
    assert_eq!(pred(Op::Null, Value::NIL), t);
    assert_eq!(pred(Op::Null, Value::make_int(0)), nil);
    assert_eq!(pred(Op::Not, Value::T), nil);
    assert_eq!(pred(Op::Not, Value::NIL), t);
    // consp
    assert_eq!(pred(Op::Consp, cons), t);
    assert_eq!(pred(Op::Consp, Value::NIL), nil);
    assert_eq!(pred(Op::Consp, Value::make_int(5)), nil);
    // stringp
    assert_eq!(pred(Op::Stringp, s), t);
    assert_eq!(pred(Op::Stringp, Value::make_int(5)), nil);
    // listp: nil or cons
    assert_eq!(pred(Op::Listp, cons), t);
    assert_eq!(pred(Op::Listp, Value::NIL), t);
    assert_eq!(pred(Op::Listp, Value::make_int(5)), nil);
}

#[test]
fn compiles_car_cdr() {
    // No GC safepoint in the JIT call, so the cons local stays alive across it.
    let cons = Value::cons(Value::make_int(11), Value::make_int(22));
    let car_ops = [Op::Constant(0), Op::Car, Op::Return];
    let cdr_ops = [Op::Constant(0), Op::Cdr, Op::Return];

    // car/cdr of a cons load the fields; differential vs the interpreter.
    // Direct value assertions, not an interp differential: interp_nullary
    // builds a Context whose heap is installed as the thread-local TAGGED_HEAP
    // and left dangling on drop, which would crash the later cons allocation.
    // car/cdr correctness is fully pinned by the expected values here.
    assert_eq!(
        lower_nullary_leaf(&car_ops, &[cons])
            .unwrap()
            .call_for_test(&[]),
        Some(Value::make_int(11).bits())
    );
    assert_eq!(
        lower_nullary_leaf(&cdr_ops, &[cons])
            .unwrap()
            .call_for_test(&[]),
        Some(Value::make_int(22).bits())
    );

    // car/cdr of nil -> nil.
    assert_eq!(
        lower_nullary_leaf(&car_ops, &[Value::NIL])
            .unwrap()
            .call_for_test(&[]),
        Some(Value::NIL.bits())
    );
    assert_eq!(
        lower_nullary_leaf(&cdr_ops, &[Value::NIL])
            .unwrap()
            .call_for_test(&[]),
        Some(Value::NIL.bits())
    );

    // car of a non-list -> deopt (interpreter signals wrong-type-argument).
    assert_eq!(
        lower_nullary_leaf(&car_ops, &[Value::make_int(5)])
            .unwrap()
            .call_for_test(&[]),
        None
    );

    // Chained: (car (cdr (11 22))) = 22.
    let list = Value::cons(
        Value::make_int(11),
        Value::cons(Value::make_int(22), Value::NIL),
    );
    let cadr =
        lower_nullary_leaf(&[Op::Constant(0), Op::Cdr, Op::Car, Op::Return], &[list]).unwrap();
    assert_eq!(cadr.call_for_test(&[]), Some(Value::make_int(22).bits()));
}

#[test]
fn compiles_cons() {
    // (cons 1 2): allocates a cons cell. No GC between the call and the deref
    // (nothing allocates), so the fresh cons stays valid.
    let leaf = lower_nullary_leaf(
        &[Op::Constant(0), Op::Constant(1), Op::Cons, Op::Return],
        &[Value::make_int(1), Value::make_int(2)],
    )
    .unwrap();
    let cell = Value::from_bits(leaf.call_for_test(&[]).expect("cons runs"));
    assert!(cell.is_cons());
    assert_eq!(cell.cons_car(), Value::make_int(1));
    assert_eq!(cell.cons_cdr(), Value::make_int(2));
}

#[test]
fn compiles_nested_cons_list() {
    // (cons 7 (cons 8 nil)) = (7 8). The inner cons leaves 7 live below it on
    // the operand stack, exercising the gc_push rooting path.
    let leaf = lower_nullary_leaf(
        &[
            Op::Constant(0),
            Op::Constant(1),
            Op::Nil,
            Op::Cons,
            Op::Cons,
            Op::Return,
        ],
        &[Value::make_int(7), Value::make_int(8)],
    )
    .unwrap();
    let result = Value::from_bits(leaf.call_for_test(&[]).expect("nested cons runs"));
    assert_eq!(result.cons_car(), Value::make_int(7));
    let tail = result.cons_cdr();
    assert!(tail.is_cons());
    assert_eq!(tail.cons_car(), Value::make_int(8));
    assert!(tail.cons_cdr().is_nil());
}

/// Build a harness Context with `name` bound to a lexical one-arg bytecode
/// callee `(lambda (y) (1+ y))`, returning (ctx, callee symbol Value).
fn harness_with_inc_callee(name: &str) -> (crate::emacs_core::eval::Context, Value) {
    let mut ev = crate::emacs_core::eval::Context::new_minimal_vm_harness();
    let sym_val = Value::symbol(name);
    let crate::emacs_core::value::ValueKind::Symbol(sym_id) = sym_val.kind() else {
        panic!("Value::symbol must produce a symbol");
    };
    let mut callee = ByteCodeFunction::new(LambdaParams {
        required: vec![crate::emacs_core::intern::SymId(1)],
        optional: Vec::new(),
        rest: None,
    });
    callee.lexical = true;
    callee.ops = vec![Op::StackRef(0), Op::Add1, Op::Return];
    callee.max_stack = 16;
    ev.obarray
        .set_symbol_function_id(sym_id, Value::make_bytecode(callee));
    (ev, sym_val)
}

#[test]
fn compiles_call_to_bytecode_callee() {
    // (lambda () (callee 41)) where callee = (lambda (y) (1+ y)).
    // The native code re-enters the runtime through the call shim; the
    // callee runs on the interpreter and the result flows back.
    let (mut ev, sym_val) = harness_with_inc_callee("jit-test-inc-callee");
    let leaf = lower_nullary_leaf(
        &[Op::Constant(0), Op::Constant(1), Op::Call(1), Op::Return],
        &[sym_val, Value::make_int(41)],
    )
    .unwrap();
    let ctx_ptr = &mut ev as *mut crate::emacs_core::eval::Context as *mut u8;
    assert_eq!(
        leaf.call(ctx_ptr, &[]),
        NativeRun::Ok(Value::make_int(42).bits())
    );
}

#[test]
fn call_with_live_values_below_roots_and_returns() {
    // (lambda () (let ((keep 7)) (+0-guard-free use of keep after a call)).
    // Body: push keep=7, push sym, push 41, Call(1) -> keep stays live below
    // the call (exercises the gc_save/gc_push rooting path), then combine:
    // [keep, result] -> StackSet(1) folds result into keep slot -> Return.
    let (mut ev, sym_val) = harness_with_inc_callee("jit-test-inc-callee-2");
    let leaf = lower_nullary_leaf(
        &[
            Op::Constant(2), // keep = 7
            Op::Constant(0), // sym
            Op::Constant(1), // 41
            Op::Call(1),     // -> [keep, 42]
            Op::StackSet(1), // -> [42]
            Op::Return,
        ],
        &[sym_val, Value::make_int(41), Value::make_int(7)],
    )
    .unwrap();
    let ctx_ptr = &mut ev as *mut crate::emacs_core::eval::Context as *mut u8;
    assert_eq!(
        leaf.call(ctx_ptr, &[]),
        NativeRun::Ok(Value::make_int(42).bits())
    );
}

#[test]
fn call_signal_propagates() {
    // Calling an unbound function must surface as NativeRun::Signal with the
    // Flow stashed for the caller — not a deopt, not a crash.
    let mut ev = crate::emacs_core::eval::Context::new_minimal_vm_harness();
    let sym_val = Value::symbol("jit-test-no-such-function");
    let leaf = lower_nullary_leaf(&[Op::Constant(0), Op::Call(0), Op::Return], &[sym_val]).unwrap();
    let ctx_ptr = &mut ev as *mut crate::emacs_core::eval::Context as *mut u8;
    assert_eq!(leaf.call(ctx_ptr, &[]), NativeRun::Signal);
    assert!(
        take_pending_flow().is_some(),
        "STATUS_SIGNAL must stash the Flow"
    );
}

#[test]
fn guard_after_call_deopts_without_replaying_the_call() {
    // THE precise-deopt capability test: a guard after a side-effecting
    // call compiles; when it fails, the interpreter resumes AT the guard
    // op — the call's side effect happened exactly once (rerun-from-start
    // would have replayed it). Full Context: the resumed 1+ promotes to a
    // bignum through the real builtin dispatch.
    let mut ev = crate::emacs_core::eval::Context::new();
    let ctx_ptr = &mut ev as *mut crate::emacs_core::eval::Context as *mut u8;
    // Callee (lambda (x) (setcar CELL (1+ (car CELL))) x): observable
    // side effect (counter cons), returns its argument unchanged.
    let cell = Value::cons(Value::make_int(0), Value::NIL);
    let sym_val = Value::symbol("jit-test-effect-callee");
    let crate::emacs_core::value::ValueKind::Symbol(sym_id) = sym_val.kind() else {
        panic!("symbol expected");
    };
    let mut callee = ByteCodeFunction::new(LambdaParams {
        required: vec![crate::emacs_core::intern::SymId(1)],
        optional: Vec::new(),
        rest: None,
    });
    callee.lexical = true;
    callee.ops = vec![
        Op::Constant(0), // CELL
        Op::Constant(0), // CELL
        Op::Car,
        Op::Add1,
        Op::Setcar,
        Op::Pop,
        Op::StackRef(0),
        Op::Return,
    ];
    callee.constants = vec![cell].into();
    callee.max_stack = 16;
    ev.obarray
        .set_symbol_function_id(sym_id, Value::make_bytecode(callee));

    // Caller: (1+ (callee MOST-POSITIVE-FIXNUM)) — the 1+ guard fails
    // AFTER the call ran.
    let ops = vec![
        Op::Constant(0), // 'callee
        Op::Constant(1), // MOST_POSITIVE
        Op::Call(1),
        Op::Add1, // pc 3: deopts (overflow)
        Op::Return,
    ];
    let constants = vec![sym_val, Value::make_int(Value::MOST_POSITIVE_FIXNUM)];
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: Vec::new(),
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = ops.clone();
    f.constants = constants.clone().into();
    f.max_stack = 16;
    f.seal_hand_assembled_ops();
    let leaf = lower_nullary_leaf(&ops, &constants).expect("guard after call compiles now");
    let native = match leaf.call(ctx_ptr, &[]) {
        NativeRun::DeoptAt(resume) => {
            let DeoptResume {
                pc,
                stack,
                handlers,
                binds,
                spec_base,
                cond_base,
            } = *resume;
            assert_eq!(pc, 3, "deopt at the 1+ after the call");
            assert_eq!(
                cell.cons_car(),
                Value::make_int(1),
                "the call's side effect ran exactly once before the deopt"
            );
            let mut vm = Vm::from_context(&mut ev);
            vm.run_resumed_frame(
                &f,
                Value::NIL,
                pc,
                &stack,
                handlers,
                &binds,
                spec_base,
                cond_base,
            )
            .expect("resume computes the bignum")
        }
        other => panic!("expected a precise deopt after the call, got {other:?}"),
    };
    assert_eq!(
        cell.cons_car(),
        Value::make_int(1),
        "resume must NOT replay the call"
    );
    // Differential: the pure interpreter on the same body (fresh counter
    // state) computes the same bignum and also increments exactly once.
    b::builtin_setcar_2(&mut ev, cell, Value::make_int(0)).expect("reset counter");
    let interp = {
        let mut vm = Vm::from_context(&mut ev);
        vm.execute(&f, vec![]).expect("interpreter computes")
    };
    assert_eq!(
        crate::emacs_core::print::print_value(&native),
        crate::emacs_core::print::print_value(&interp),
        "resume result must equal the interpreter's"
    );
    assert_eq!(cell.cons_car(), Value::make_int(1));
}

#[test]
fn guard_before_call_compiles_and_deopts_cleanly() {
    // Guards strictly before the first call are fine: a deopt there reruns
    // the interpreter with no side effect having happened.
    let (mut ev, sym_val) = harness_with_inc_callee("jit-test-inc-callee-3");
    let ops = [
        Op::Constant(0), // sym
        Op::Constant(1), // n
        Op::Add1,        // guard BEFORE the call
        Op::Call(1),
        Op::Return,
    ];
    // In-range: runs natively end-to-end: (1+ 40) = 41 -> callee -> 42.
    let leaf = lower_nullary_leaf(&ops, &[sym_val, Value::make_int(40)]).unwrap();
    let ctx_ptr = &mut ev as *mut crate::emacs_core::eval::Context as *mut u8;
    assert_eq!(
        leaf.call(ctx_ptr, &[]),
        NativeRun::Ok(Value::make_int(42).bits())
    );
    // Boundary input: the pre-call guard now deopts PRECISELY at the 1+
    // op (pc 2) with the pre-op stack captured — the resume would rerun
    // exactly that op on the interpreter.
    let leaf2 = lower_nullary_leaf(
        &ops,
        &[sym_val, Value::make_int(Value::MOST_POSITIVE_FIXNUM)],
    )
    .unwrap();
    match leaf2.call(ctx_ptr, &[]) {
        NativeRun::DeoptAt(resume) => {
            let DeoptResume {
                pc,
                stack,
                handlers,
                binds,
                ..
            } = *resume;
            assert_eq!(pc, 2, "deopt at the Add1 op");
            assert_eq!(stack.len(), 2, "pre-op stack: [callee-sym, arg]");
            assert_eq!(stack[1], Value::make_int(Value::MOST_POSITIVE_FIXNUM));
            assert_eq!(handlers, 0);
            assert!(binds.is_empty());
        }
        other => panic!("expected a precise deopt, got {other:?}"),
    }
}

#[test]
fn compiles_fixnum_div_rem() {
    let run = |op: Op, a: i64, b: i64| {
        lower_nullary_leaf(
            &[Op::Constant(0), Op::Constant(1), op, Op::Return],
            &[Value::make_int(a), Value::make_int(b)],
        )
        .unwrap()
        .call_for_test(&[])
    };
    // Truncation toward zero, matching the interpreter / C.
    assert_eq!(run(Op::Div, 42, 5), Some(Value::make_int(8).bits()));
    assert_eq!(run(Op::Div, -42, 5), Some(Value::make_int(-8).bits()));
    assert_eq!(run(Op::Div, 42, -5), Some(Value::make_int(-8).bits()));
    assert_eq!(run(Op::Rem, 42, 5), Some(Value::make_int(2).bits()));
    assert_eq!(run(Op::Rem, -42, 5), Some(Value::make_int(-2).bits()));
    // Zero divisor -> deopt (interpreter signals arith-error).
    assert_eq!(run(Op::Div, 1, 0), None);
    assert_eq!(run(Op::Rem, 1, 0), None);
    // Non-fixnum operand -> deopt.
    let nf = lower_nullary_leaf(
        &[Op::Constant(0), Op::Nil, Op::Div, Op::Return],
        &[Value::make_int(4)],
    )
    .unwrap();
    assert_eq!(nf.call_for_test(&[]), None);
}

#[test]
fn div_wrap_case_matches_interpreter() {
    // MOST_NEGATIVE_FIXNUM / -1 overflows fixnum range (= 2^60). The interpreter
    // wraps it; the unboxed JIT (raw_fixnum_divrem) range-checks and DEOPTS
    // rather than keep an out-of-range raw value, then a precise-deopt resume
    // reruns Op::Div in the interpreter and wraps to the same bits. Resume-value
    // parity is covered by the THRESHOLD=1 differential gate + the straight-line
    // fuzz (which generates Div over these boundary constants); here we assert
    // the deopt itself (call_for_test returns None on deopt).
    let ops = [Op::Constant(0), Op::Constant(1), Op::Div, Op::Return];
    let consts = [
        Value::make_int(Value::MOST_NEGATIVE_FIXNUM),
        Value::make_int(-1),
    ];
    let leaf = lower_nullary_leaf(&ops, &consts).unwrap();
    assert_eq!(
        leaf.call_for_test(&[]),
        None,
        "fixnum-overflow division must deopt to the interpreter, not native-wrap"
    );
}

#[test]
fn compiles_eq_and_symbolp() {
    // One live Context for the vmctx-reading slow paths (symbols-with-pos
    // is disabled by default, so differing bits -> nil).
    let mut ev = crate::emacs_core::eval::Context::new_minimal_vm_harness();
    let ctx_ptr = &mut ev as *mut crate::emacs_core::eval::Context as *mut u8;
    let sym_a = Value::symbol("jit-eq-sym-a");
    let s = Value::string("eq-str");

    let eq2 = |a: Value, b: Value, ctx: *mut u8| {
        lower_nullary_leaf(
            &[Op::Constant(0), Op::Constant(1), Op::Eq, Op::Return],
            &[a, b],
        )
        .unwrap()
        .call(ctx, &[])
    };
    let t = NativeRun::Ok(Value::T.bits());
    let nil = NativeRun::Ok(Value::NIL.bits());
    // Identical bits -> t (fast path, no shim).
    assert_eq!(eq2(Value::make_int(7), Value::make_int(7), ctx_ptr), t);
    assert_eq!(eq2(sym_a, sym_a, ctx_ptr), t);
    assert_eq!(eq2(Value::NIL, Value::NIL, ctx_ptr), t);
    // Differing bits -> slow shim -> nil (swp disabled).
    assert_eq!(eq2(Value::make_int(7), Value::make_int(8), ctx_ptr), nil);
    assert_eq!(eq2(sym_a, Value::make_int(7), ctx_ptr), nil);

    let symp = |v: Value, ctx: *mut u8| {
        lower_nullary_leaf(&[Op::Constant(0), Op::Symbolp, Op::Return], &[v])
            .unwrap()
            .call(ctx, &[])
    };
    // Symbol tag -> t natively (nil and t are symbols).
    assert_eq!(symp(sym_a, ctx_ptr), t);
    assert_eq!(symp(Value::NIL, ctx_ptr), t);
    assert_eq!(symp(Value::T, ctx_ptr), t);
    // Non-symbol -> slow shim -> nil (swp disabled).
    assert_eq!(symp(Value::make_int(5), ctx_ptr), nil);
    assert_eq!(symp(s, ctx_ptr), nil);
}

#[test]
fn compiles_apply_with_spread() {
    // (apply 'inc (list 41)) -> 42: the last argument spreads as the list.
    let (mut ev, sym_val) = harness_with_inc_callee("jit-test-inc-apply");
    let arg_list = Value::cons(Value::make_int(41), Value::NIL);
    let leaf = lower_nullary_leaf(
        &[Op::Constant(0), Op::Constant(1), Op::Apply(1), Op::Return],
        &[sym_val, arg_list],
    )
    .unwrap();
    let ctx_ptr = &mut ev as *mut crate::emacs_core::eval::Context as *mut u8;
    assert_eq!(
        leaf.call(ctx_ptr, &[]),
        NativeRun::Ok(Value::make_int(42).bits())
    );
}

#[test]
fn compiles_apply_with_leading_args() {
    // (apply 'add2 40 (list 2)) -> 42: leading args + spread tail.
    let mut ev = crate::emacs_core::eval::Context::new_minimal_vm_harness();
    let sym_val = Value::symbol("jit-test-add2-apply");
    let crate::emacs_core::value::ValueKind::Symbol(sym_id) = sym_val.kind() else {
        panic!("symbol expected");
    };
    let mut callee = ByteCodeFunction::new(LambdaParams {
        required: vec![
            crate::emacs_core::intern::SymId(1),
            crate::emacs_core::intern::SymId(2),
        ],
        optional: Vec::new(),
        rest: None,
    });
    callee.lexical = true;
    callee.ops = vec![Op::StackRef(1), Op::StackRef(1), Op::Add, Op::Return];
    callee.max_stack = 16;
    ev.obarray
        .set_symbol_function_id(sym_id, Value::make_bytecode(callee));

    let tail = Value::cons(Value::make_int(2), Value::NIL);
    let leaf = lower_nullary_leaf(
        &[
            Op::Constant(0), // sym
            Op::Constant(1), // 40
            Op::Constant(2), // (2)
            Op::Apply(2),
            Op::Return,
        ],
        &[sym_val, Value::make_int(40), tail],
    )
    .unwrap();
    let ctx_ptr = &mut ev as *mut crate::emacs_core::eval::Context as *mut u8;
    assert_eq!(
        leaf.call(ctx_ptr, &[]),
        NativeRun::Ok(Value::make_int(42).bits())
    );
}

#[test]
fn bails_on_missing_return() {
    let err = lower_nullary_leaf(&[Op::Nil], &[]).unwrap_err();
    assert!(matches!(err, CompileError::NoReturn));
}

#[test]
fn bails_on_argument_taking_function() {
    let mut f = nullary();
    f.params.required.push(crate::emacs_core::intern::SymId(1));
    f.ops = vec![Op::Nil, Op::Return];
    let err = compile_bytecode_function(&f).unwrap_err();
    assert!(matches!(err, CompileError::TakesArguments));
}

#[test]
fn bails_on_stack_underflow() {
    let err = lower_nullary_leaf(&[Op::Return], &[]).unwrap_err();
    assert!(matches!(err, CompileError::StackUnderflow));
}

#[test]
fn compile_bytecode_function_handles_nullary_leaf() {
    let mut f = nullary();
    let c = Value::make_int(123);
    f.constants = vec![c].into();
    f.ops = vec![Op::Constant(0), Op::Return];
    let leaf = compile_bytecode_function(&f).unwrap();
    assert_eq!(leaf.call_for_test(&[]), Some(c.bits()));
}

#[test]
fn one_arg_identity_and_increment() {
    // (lambda (x) x)
    let id = lower_leaf(&[Op::StackRef(0), Op::Return], &[], 1).unwrap();
    assert_eq!(id.arity(), 1);
    assert_eq!(
        id.call_for_test(&[Value::make_int(7)]),
        Some(Value::make_int(7).bits())
    );
    // (lambda (x) (1+ x))
    let inc = lower_leaf(&[Op::StackRef(0), Op::Add1, Op::Return], &[], 1).unwrap();
    assert_eq!(
        inc.call_for_test(&[Value::make_int(41)]),
        Some(Value::make_int(42).bits())
    );
}

#[test]
fn two_arg_addition_preserves_args_via_stackref() {
    // (lambda (a b) (+ a b)); each StackRef(1) reaches an original arg as the
    // model stack grows: seed [a,b] -> push a -> push b -> Add -> a+b.
    let add = lower_leaf(
        &[Op::StackRef(1), Op::StackRef(1), Op::Add, Op::Return],
        &[],
        2,
    )
    .unwrap();
    assert_eq!(
        add.call_for_test(&[Value::make_int(40), Value::make_int(2)]),
        Some(Value::make_int(42).bits())
    );
    // A non-fixnum argument makes the speculative Add deopt.
    assert_eq!(add.call_for_test(&[Value::make_int(40), Value::NIL]), None);
}

#[test]
fn compile_bytecode_function_accepts_required_args_when_lexical() {
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: vec![
            crate::emacs_core::intern::SymId(1),
            crate::emacs_core::intern::SymId(2),
        ],
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = vec![Op::StackRef(1), Op::StackRef(1), Op::Add, Op::Return];
    let leaf = compile_bytecode_function(&f).unwrap();
    assert_eq!(leaf.arity(), 2);
    assert_eq!(
        leaf.call_for_test(&[Value::make_int(1), Value::make_int(41)]),
        Some(Value::make_int(42).bits())
    );
}

#[test]
fn compile_bytecode_function_bails_on_dynamic_params() {
    // Required params but dynamic binding (not lexical, arglist not a
    // fixnum) -> params are not on the stack -> bail.
    let mut dynp = ByteCodeFunction::new(LambdaParams {
        required: vec![crate::emacs_core::intern::SymId(1)],
        optional: Vec::new(),
        rest: None,
    });
    dynp.lexical = false;
    dynp.ops = vec![Op::StackRef(0), Op::Return];
    assert!(!params_on_stack(&dynp));
    assert!(matches!(
        compile_bytecode_function(&dynp),
        Err(CompileError::TakesArguments)
    ));
}

#[test]
fn compiles_optional_params_with_nil_padding() {
    // (lambda (a &optional b) b): frame = [a, b]; missing b is nil-padded.
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: vec![crate::emacs_core::intern::SymId(1)],
        optional: vec![crate::emacs_core::intern::SymId(2)],
        rest: None,
    });
    f.lexical = true;
    f.ops = vec![Op::StackRef(0), Op::Return]; // top of frame = b
    f.max_stack = 16;
    let leaf = compile_bytecode_function(&f).unwrap();
    assert!(leaf.accepts(1) && leaf.accepts(2));
    assert!(!leaf.accepts(0) && !leaf.accepts(3));
    // One arg: b is nil.
    assert_eq!(
        leaf.call(core::ptr::null_mut(), &[Value::make_int(5)]),
        NativeRun::Ok(Value::NIL.bits())
    );
    // Two args: b is supplied.
    assert_eq!(
        leaf.call(
            core::ptr::null_mut(),
            &[Value::make_int(5), Value::make_int(6)]
        ),
        NativeRun::Ok(Value::make_int(6).bits())
    );
}

#[test]
fn compiles_rest_param_as_list() {
    // (lambda (&rest xs) xs): frame = [xs]; surplus args become a list.
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: Vec::new(),
        optional: Vec::new(),
        rest: Some(crate::emacs_core::intern::SymId(1)),
    });
    f.lexical = true;
    f.ops = vec![Op::StackRef(0), Op::Return];
    f.max_stack = 16;
    let leaf = compile_bytecode_function(&f).unwrap();
    assert!(leaf.accepts(0) && leaf.accepts(5));
    // No args: xs = nil.
    assert_eq!(
        leaf.call(core::ptr::null_mut(), &[]),
        NativeRun::Ok(Value::NIL.bits())
    );
    // Two args: xs = (10 20).
    let NativeRun::Ok(bits) = leaf.call(
        core::ptr::null_mut(),
        &[Value::make_int(10), Value::make_int(20)],
    ) else {
        panic!("rest call must succeed");
    };
    let xs = Value::from_bits(bits);
    assert_eq!(xs.cons_car(), Value::make_int(10));
    assert_eq!(xs.cons_cdr().cons_car(), Value::make_int(20));
    assert!(xs.cons_cdr().cons_cdr().is_nil());
}

/// Run a nullary body through the Tier-0 interpreter (the correctness
/// oracle) and return its result.
fn interp_nullary(ops: &[Op], constants: &[Value]) -> Value {
    use crate::emacs_core::bytecode::Vm;
    use crate::emacs_core::eval::Context;
    let mut eval = Context::new_minimal_vm_harness();
    let mut f = nullary();
    f.ops = ops.to_vec();
    f.constants = constants.to_vec().into();
    f.max_stack = 16;
    let mut vm = Vm::from_context(&mut eval);
    vm.execute(&f, vec![]).expect("interpreter runs the body")
}

#[test]
fn jit_matches_interpreter_on_supported_bodies() {
    // The ultimate parity proof: when the JIT compiles a body and does not
    // deopt, its result must be bit-identical to the interpreter's.
    let cases: &[(&[Op], &[Value])] = &[
        (&[Op::Constant(0), Op::Return], &[Value::make_int(42)]),
        (&[Op::Nil, Op::Return], &[]),
        (&[Op::True, Op::Return], &[]),
        (
            &[Op::Constant(0), Op::Constant(1), Op::Add, Op::Return],
            &[Value::make_int(40), Value::make_int(2)],
        ),
        (
            &[Op::Constant(0), Op::Constant(1), Op::Sub, Op::Return],
            &[Value::make_int(3), Value::make_int(10)],
        ),
        (
            &[Op::Constant(0), Op::Constant(1), Op::Mul, Op::Return],
            &[Value::make_int(-6), Value::make_int(7)],
        ),
        (&[Op::Nil, Op::Null, Op::Return], &[]),
        (
            &[Op::Constant(0), Op::Null, Op::Return],
            &[Value::make_int(0)],
        ),
        (
            &[Op::Constant(0), Op::Consp, Op::Return],
            &[Value::make_int(5)],
        ),
        (
            &[Op::Constant(0), Op::Listp, Op::Return],
            &[Value::make_int(5)],
        ),
        (
            &[Op::Constant(0), Op::Add1, Op::Return],
            &[Value::make_int(41)],
        ),
        (
            &[Op::Constant(0), Op::Sub1, Op::Return],
            &[Value::make_int(43)],
        ),
        (
            &[Op::Constant(0), Op::Negate, Op::Return],
            &[Value::make_int(42)],
        ),
        (
            &[
                Op::Constant(0),
                Op::Constant(1),
                Op::Add,
                Op::Constant(2),
                Op::Sub,
                Op::Return,
            ],
            &[Value::make_int(1), Value::make_int(2), Value::make_int(4)],
        ),
        (
            &[Op::Constant(0), Op::Constant(1), Op::Lss, Op::Return],
            &[Value::make_int(3), Value::make_int(5)],
        ),
        (
            &[Op::Constant(0), Op::Constant(1), Op::Gtr, Op::Return],
            &[Value::make_int(3), Value::make_int(5)],
        ),
        (
            &[Op::Constant(0), Op::Constant(1), Op::Eqlsign, Op::Return],
            &[Value::make_int(5), Value::make_int(5)],
        ),
        (
            &[
                Op::Constant(0),
                Op::Constant(1),
                Op::Constant(2),
                Op::DiscardN(2),
                Op::Return,
            ],
            &[
                Value::make_int(10),
                Value::make_int(20),
                Value::make_int(30),
            ],
        ),
        (
            &[
                Op::Constant(0),
                Op::Constant(1),
                Op::Constant(2),
                Op::DiscardN(0x82),
                Op::Return,
            ],
            &[
                Value::make_int(10),
                Value::make_int(20),
                Value::make_int(30),
            ],
        ),
    ];
    for (i, (ops, consts)) in cases.iter().enumerate() {
        let want = interp_nullary(ops, consts).bits();
        let got = lower_nullary_leaf(ops, consts).unwrap().call_for_test(&[]);
        assert_eq!(got, Some(want), "JIT/interpreter mismatch on case {i}");
    }
}

#[test]
fn jit_matches_interpreter_with_args() {
    use crate::emacs_core::bytecode::Vm;
    use crate::emacs_core::eval::Context;
    // (lambda (a b) (+ a b)), lexical.
    let ops = [Op::StackRef(1), Op::StackRef(1), Op::Add, Op::Return];
    let args = [Value::make_int(40), Value::make_int(2)];

    let mut eval = Context::new_minimal_vm_harness();
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: vec![
            crate::emacs_core::intern::SymId(1),
            crate::emacs_core::intern::SymId(2),
        ],
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = ops.to_vec();
    f.max_stack = 16;
    let want = {
        let mut vm = Vm::from_context(&mut eval);
        vm.execute(&f, args.to_vec())
            .expect("interpreter runs")
            .bits()
    };

    let got = lower_leaf(&ops, &[], 2).unwrap().call_for_test(&args);
    assert_eq!(got, Some(want), "JIT must match the interpreter with args");
}

// Note: the JIT's deopt *boundary* (out-of-range -> None) is covered by
// `add_overflowing_fixnum_range_deopts` and `unary_boundary_inputs_deopt`.
// A differential check against the interpreter's bignum-promotion path is
// intentionally omitted here because `new_minimal_vm_harness` does not wire
// the full `+`/bignum builtins (it signals on that fallback), so it cannot
// serve as the oracle for the slow path.

/// Phase-8 micro-benchmark: the hot fixnum countdown loop, Tier 0 vs JIT.
/// `#[ignore]`d (timing does not belong in CI); run explicitly, in release:
/// `cargo nextest run --cargo-profile release --features jit --run-ignored all jit_bench`
#[test]
#[ignore = "manual perf measurement; run in release"]
fn jit_bench_countdown_loop() {
    use crate::emacs_core::bytecode::Vm;
    use crate::emacs_core::eval::Context;
    use std::time::Instant;

    // (lambda (n) (while (> n 0) (setq n (1- n))) n)
    let ops = [
        Op::StackRef(0),
        Op::Constant(0),
        Op::Gtr,
        Op::GotoIfNil(8),
        Op::StackRef(0),
        Op::Sub1,
        Op::StackSet(1),
        Op::Goto(0),
        Op::StackRef(0),
        Op::Return,
    ];
    let constants = [Value::make_int(0)];
    let iters: i64 = 3_000_000;
    let calls = 5;

    let mut ev = Context::new_minimal_vm_harness();

    // Tier 0.
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: vec![crate::emacs_core::intern::SymId(1)],
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = ops.to_vec();
    f.constants = constants.to_vec().into();
    f.max_stack = 16;
    let t0 = Instant::now();
    for _ in 0..calls {
        let mut vm = Vm::from_context(&mut ev);
        let r = vm.execute(&f, vec![Value::make_int(iters)]).unwrap();
        assert_eq!(r, Value::make_int(0));
    }
    let interp = t0.elapsed();

    // JIT.
    let leaf = lower_leaf(&ops, &constants, 1).unwrap();
    let ctx_ptr = &mut ev as *mut Context as *mut u8;
    let t1 = Instant::now();
    for _ in 0..calls {
        assert_eq!(
            leaf.call(ctx_ptr, &[Value::make_int(iters)]),
            NativeRun::Ok(Value::make_int(0).bits())
        );
    }
    let jit = t1.elapsed();

    eprintln!(
        "[jit-bench] countdown {iters}x{calls}: interp {interp:?}  jit {jit:?}  speedup {:.1}x",
        interp.as_secs_f64() / jit.as_secs_f64()
    );
}

/// Compare the previous baseline tier and the production MIR tier in one
/// process. The same Elisp workloads for GNU live in scripts/mir-loop-bench.el.
/// Compilation and warmup are excluded; alternate order and report medians.
#[test]
#[ignore = "manual perf measurement; pin to an idle core"]
fn jit_bench_mir_loops_against_baseline() {
    use crate::emacs_core::eval::Context;
    use crate::emacs_core::intern::SymId;
    use std::time::Instant;

    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context as *mut u8;
    let n = 1_000_000_i64;
    for name in ["countdown", "sum", "polynomial"] {
        let mut f = ByteCodeFunction::new(LambdaParams {
            required: vec![SymId(1)],
            optional: Vec::new(),
            rest: None,
        });
        f.lexical = true;
        f.constants = vec![Value::make_int(0), Value::make_int(3), Value::make_int(7)].into();
        f.max_stack = 8;
        f.ops.push(Op::Constant(0)); // [n acc]
        f.ops
            .extend([Op::StackRef(1), Op::Constant(0), Op::Gtr, Op::GotoIfNil(0)]);
        if name != "countdown" {
            f.ops.extend([Op::StackRef(0), Op::StackRef(2)]); // [n acc acc n]
            if name == "polynomial" {
                f.ops.extend([
                    Op::Dup,
                    Op::Mul,
                    Op::StackRef(3),
                    Op::Constant(1),
                    Op::Mul,
                    Op::Add,
                    Op::Constant(2),
                    Op::Add,
                ]);
            }
            f.ops.extend([Op::Add, Op::StackSet(1)]);
        }
        f.ops
            .extend([Op::StackRef(1), Op::Sub1, Op::StackSet(2), Op::Goto(1)]);
        f.ops[4] = Op::GotoIfNil(f.ops.len() as u32);
        f.ops.push(Op::Return);
        let expected = match name {
            "countdown" => 0,
            "sum" => n * (n + 1) / 2,
            "polynomial" => n * (n + 1) * (2 * n + 1) / 6 + 3 * n * (n + 1) / 2 + 7 * n,
            _ => unreachable!(),
        };
        let baseline = lower_leaf(&f.ops, &f.constants, 1).expect("baseline compiles");
        let optimized =
            compile_bytecode_function_with(&f, Some(&ev.obarray)).expect("MIR compiles");
        assert_eq!(optimized.tier, super::leaf::LeafTier::Mir);
        let args = [Value::make_int(n)];
        let mut samples = [Vec::new(), Vec::new()];
        for round in 0..8 {
            for which in [round % 2, 1 - round % 2] {
                let leaf = [&baseline, &optimized][which];
                let start = Instant::now();
                for _ in 0..5 {
                    assert_eq!(
                        leaf.call(ctx, &args),
                        NativeRun::Ok(Value::make_int(expected).bits())
                    );
                }
                if round != 0 {
                    samples[which].push(start.elapsed());
                }
            }
        }
        for sample in &mut samples {
            sample.sort();
        }
        let baseline_us = samples[0][3].as_secs_f64() * 1e6 / 5.0;
        let mir_us = samples[1][3].as_secs_f64() * 1e6 / 5.0;
        eprintln!(
            "BENCH mir-loop {name} n={n} baseline_us={baseline_us:.1} mir_us={mir_us:.1} speedup={:.3}",
            baseline_us / mir_us
        );
    }
}

/// Differential fuzzing (the Phase-9 discipline, brought forward): generate
/// seeded random straight-line bodies over the supported non-allocating op
/// subset, run each through BOTH tiers, and hold the tiering contract:
/// - `Ok(bits)`  -> the interpreter must produce exactly those bits;
/// - `Deopt`     -> the seam reruns the interpreter (sound by the poisoning
///   analysis), so any interpreter outcome is acceptable;
/// - `Signal`    -> the interpreter must also signal.
#[test]
fn fuzz_straightline_bodies_match_interpreter() {
    use crate::emacs_core::bytecode::Vm;
    use crate::emacs_core::eval::Context;

    // Deterministic xorshift64* — no external randomness (reproducible; on
    // failure the seed in the assert message reproduces the body).
    fn next(state: &mut u64) -> u64 {
        let mut x = *state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        *state = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    let mut ev = Context::new_minimal_vm_harness();
    let ctx_ptr = &mut ev as *mut Context as *mut u8;

    // Constant pool: small fixnums, the fixnum boundaries, nil and t —
    // enough to hit fast paths, deopt boundaries, and type guards. No heap
    // values in the pool; the one heap result a body can make is a bignum
    // from an overflow, a fresh object in each engine, so results compare
    // by `eql` (exactly by bits for everything else).
    let same = |got: Value, want: Value| {
        got.bits() == want.bits() || crate::emacs_core::value::eql_value(&got, &want)
    };
    let constants: Vec<Value> = vec![
        Value::make_int(0),
        Value::make_int(1),
        Value::make_int(-1),
        Value::make_int(2),
        Value::make_int(3),
        Value::make_int(Value::MOST_POSITIVE_FIXNUM),
        Value::make_int(Value::MOST_NEGATIVE_FIXNUM),
        Value::NIL,
        Value::T,
    ];

    for seed in 1u64..=600 {
        let mut rng = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15);
        let len = 1 + (next(&mut rng) % 18) as usize;
        let mut ops: Vec<Op> = Vec::with_capacity(len + 2);
        let mut depth: usize = 0;
        for _ in 0..len {
            let r = (next(&mut rng) % 100) as usize;
            let op = if depth == 0 || r < 30 {
                // Pushes (always valid).
                match next(&mut rng) % 3 {
                    0 => Op::Nil,
                    1 => Op::True,
                    _ => Op::Constant((next(&mut rng) % constants.len() as u64) as u16),
                }
            } else if depth >= 2 && r < 60 {
                // Binary ops.
                match next(&mut rng) % 11 {
                    0 => Op::Add,
                    1 => Op::Sub,
                    2 => Op::Mul,
                    3 => Op::Div,
                    4 => Op::Rem,
                    5 => Op::Eqlsign,
                    6 => Op::Lss,
                    7 => Op::Gtr,
                    8 => Op::Leq,
                    9 => Op::Geq,
                    _ => Op::Eq,
                }
            } else if r < 85 {
                // Unary ops (depth >= 1).
                match next(&mut rng) % 10 {
                    0 => Op::Add1,
                    1 => Op::Sub1,
                    2 => Op::Negate,
                    3 => Op::Null,
                    4 => Op::Not,
                    5 => Op::Consp,
                    6 => Op::Stringp,
                    7 => Op::Listp,
                    8 => Op::Symbolp,
                    _ => Op::Dup,
                }
            } else {
                // Stack shuffles.
                match next(&mut rng) % 3 {
                    0 => Op::Dup,
                    1 => Op::StackRef((next(&mut rng) % depth as u64) as u16),
                    _ if depth >= 2 => {
                        Op::StackSet(1 + (next(&mut rng) % (depth as u64 - 1)) as u16)
                    }
                    _ => Op::Pop,
                }
            };
            let (needs, delta) = simple_effect(&op).expect("generator emits supported ops");
            if depth < needs {
                continue; // skip an op the current depth can't support
            }
            depth = (depth as i64 + delta) as usize;
            ops.push(op);
        }
        if depth == 0 {
            ops.push(Op::Constant(0));
        }
        ops.push(Op::Return);

        // Tier 0 (oracle).
        let mut f = ByteCodeFunction::new(LambdaParams {
            required: Vec::new(),
            optional: Vec::new(),
            rest: None,
        });
        f.lexical = true;
        f.ops = ops.clone();
        f.constants = constants.clone().into();
        f.max_stack = 64;
        // `execute` seals a hand-assembled chunk on its own; the precise-deopt
        // resume below (`run_resumed_frame`) does not, and rejects the chunk
        // as invalid bytecode. That rejection went unnoticed while every
        // resumed body also failed in the interpreter (this bare harness has
        // no registered arithmetic subrs); now that the arithmetic opcodes
        // answer integers directly, as GNU's `Bsub1` does, it would not.
        f.seal_hand_assembled_ops_for_test();
        let interp = {
            let mut vm = Vm::from_context(&mut ev);
            vm.execute(&f, vec![])
        };

        // JIT.
        let leaf = lower_leaf(&ops, &constants, 0)
            .unwrap_or_else(|e| panic!("seed {seed}: body must compile, got {e}: {ops:?}"));
        match leaf.call(ctx_ptr, &[]) {
            NativeRun::Ok(bits) => {
                let want = interp.as_ref().unwrap_or_else(|e| {
                    panic!("seed {seed}: JIT Ok but interpreter erred ({e:?}): {ops:?}")
                });
                assert!(
                    same(Value::from_bits(bits), *want),
                    "seed {seed}: JIT/interpreter mismatch on {ops:?}"
                );
            }
            NativeRun::Deopt => {
                // The seam reruns the interpreter; nothing further to hold.
            }
            NativeRun::DeoptAt(resume) => {
                let DeoptResume {
                    pc,
                    stack,
                    handlers,
                    binds,
                    spec_base,
                    cond_base,
                } = *resume;
                // Precise deopt: resume mid-function and the result must
                // match the pure-interpreter run exactly.
                let mut vm = crate::emacs_core::bytecode::Vm::from_context(&mut ev);
                let resumed = vm.run_resumed_frame(
                    &f,
                    Value::NIL,
                    pc,
                    &stack,
                    handlers,
                    &binds,
                    spec_base,
                    cond_base,
                );
                match (&resumed, &interp) {
                    (Ok(got), Ok(want)) => assert!(
                        same(*got, *want),
                        "seed {seed}: resume/interpreter mismatch on {ops:?}: {} vs {}",
                        crate::emacs_core::print::print_value(got),
                        crate::emacs_core::print::print_value(want),
                    ),
                    (Err(_), Err(_)) => {}
                    other => panic!(
                        "seed {seed}: resume/interpreter outcome mismatch {other:?}: {ops:?}"
                    ),
                }
            }
            NativeRun::Signal => {
                let _ = take_pending_flow();
                assert!(
                    interp.is_err(),
                    "seed {seed}: JIT signaled but interpreter succeeded: {ops:?}"
                );
            }
        }

        // Also exercise the typed-MIR Tier-2 path (build_mir + lower_mir_pure)
        // on the same body, skipping bodies the pure subset bails on. Localizes
        // lower_mir_pure miscompiles (the module-test failures under MIR wiring).
        if let Ok(mir) = mir::build_mir(&ops, &constants, 0)
            && let Ok(mleaf) = lower_mir_pure(&mir)
        {
            match mleaf.call(ctx_ptr, &[]) {
                NativeRun::Ok(bits) => {
                    if let Ok(want) = &interp {
                        assert!(
                            same(Value::from_bits(bits), *want),
                            "seed {seed}: MIR/interpreter mismatch on {ops:?}"
                        );
                    }
                }
                NativeRun::Deopt | NativeRun::DeoptAt(_) => {}
                NativeRun::Signal => {
                    let _ = take_pending_flow();
                }
            }
        }
    }
}

/// Differential fuzzing for SIDE EFFECTS — the gap the return-value fuzzer
/// above leaves open. Bodies mix arithmetic with `VarSet`/`VarRef` on seeded
/// special variables and run through the REAL tier dispatch
/// (`compile_bytecode_function`: MIR if it claims the body, else baseline),
/// comparing the return value AND the FINAL VALUE OF EVERY SEEDED VARIABLE
/// against the interpreter. Return-value comparison alone missed the
/// 0-result-opaque-drop bug for a month — a compiled `setq` returned the
/// right value (via the bytecode `Dup`) while silently skipping the
/// assignment — so this pins the state contract: a dropped or mis-lowered
/// side-effecting op in ANY tier fails here, whichever tier serves the body.
///
/// Extra invariant held: a plain `Deopt` (rerun-from-start) may only come
/// from a guard BEFORE any side effect (the poisoning analysis), so on
/// `Deopt` every seeded variable must still hold its initial value.
#[test]
fn fuzz_varset_bodies_match_interpreter_state() {
    use crate::emacs_core::bytecode::Vm;
    use crate::emacs_core::eval::Context;
    use crate::emacs_core::intern::SymId;

    fn next(state: &mut u64) -> u64 {
        let mut x = *state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        *state = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    let mut ev = Context::new_minimal_vm_harness();
    let ctx_ptr = &mut ev as *mut Context as *mut u8;

    // Seeded special variables at constant indices 0..VARS; every stored
    // value in the op soup is an immediate, so state compares exactly by bits.
    const VARS: usize = 3;
    let var_vals: Vec<Value> = ["fuzz-jit-var-a", "fuzz-jit-var-b", "fuzz-jit-var-c"]
        .iter()
        .map(Value::symbol)
        .collect();
    let var_ids: Vec<SymId> = var_vals
        .iter()
        .map(|v| match v.kind() {
            crate::emacs_core::value::ValueKind::Symbol(id) => id,
            _ => panic!("symbol expected"),
        })
        .collect();
    let init = [Value::make_int(10), Value::make_int(-7), Value::NIL];

    let mut constants: Vec<Value> = var_vals.clone();
    constants.extend([
        Value::make_int(0),
        Value::make_int(1),
        Value::make_int(-1),
        Value::make_int(3),
        Value::make_int(Value::MOST_POSITIVE_FIXNUM),
        Value::NIL,
        Value::T,
    ]);

    fn reset(ev: &mut Context, ids: &[SymId], init: &[Value]) {
        for (id, v) in ids.iter().zip(init.iter()) {
            ev.obarray.set_symbol_value_id(*id, *v);
        }
    }
    fn snap(ev: &Context, ids: &[SymId]) -> Vec<Option<usize>> {
        ids.iter()
            .map(|id| ev.obarray.symbol_value_id(*id).copied().map(|v| v.bits()))
            .collect()
    }

    for seed in 1u64..=300 {
        let mut rng = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15);
        let len = 2 + (next(&mut rng) % 16) as usize;
        let mut ops: Vec<Op> = Vec::with_capacity(len + 3);
        let mut depth: usize = 0;
        let mut emitted_varset = false;
        for _ in 0..len {
            let r = (next(&mut rng) % 100) as usize;
            let op = if depth == 0 || r < 25 {
                match next(&mut rng) % 4 {
                    0 => Op::Nil,
                    1 => Op::True,
                    2 => Op::VarRef((next(&mut rng) % VARS as u64) as u16),
                    _ => Op::Constant(
                        (VARS as u64 + next(&mut rng) % (constants.len() - VARS) as u64) as u16,
                    ),
                }
            } else if r < 45 {
                // The point of this fuzzer: a side-effecting VarSet.
                emitted_varset = true;
                Op::VarSet((next(&mut rng) % VARS as u64) as u16)
            } else if depth >= 2 && r < 70 {
                match next(&mut rng) % 8 {
                    0 => Op::Add,
                    1 => Op::Sub,
                    2 => Op::Mul,
                    3 => Op::Div,
                    4 => Op::Eqlsign,
                    5 => Op::Lss,
                    6 => Op::Gtr,
                    _ => Op::Eq,
                }
            } else if r < 90 {
                match next(&mut rng) % 6 {
                    0 => Op::Add1,
                    1 => Op::Sub1,
                    2 => Op::Negate,
                    3 => Op::Null,
                    4 => Op::Not,
                    _ => Op::Dup,
                }
            } else {
                match next(&mut rng) % 2 {
                    0 => Op::Dup,
                    _ => Op::StackRef((next(&mut rng) % depth as u64) as u16),
                }
            };
            let (needs, delta) = simple_effect(&op).expect("generator emits supported ops");
            if depth < needs {
                continue;
            }
            depth = (depth as i64 + delta) as usize;
            ops.push(op);
        }
        // Guarantee at least one VarSet per body so no seed degenerates into
        // the pure fuzzer above.
        if !emitted_varset {
            ops.push(Op::Constant(VARS as u16)); // a fixnum
            ops.push(Op::VarSet((seed % VARS as u64) as u16));
        }
        if depth == 0 {
            ops.push(Op::Constant(VARS as u16));
        }
        ops.push(Op::Return);

        let mut f = ByteCodeFunction::new(LambdaParams {
            required: Vec::new(),
            optional: Vec::new(),
            rest: None,
        });
        f.lexical = true;
        f.ops = ops.clone();
        f.constants = constants.clone().into();
        f.max_stack = 64;
        // Sealed for the precise-deopt resume, as in the fuzzer above.
        f.seal_hand_assembled_ops_for_test();

        // Tier 0 (oracle): result + final variable state.
        reset(&mut ev, &var_ids, &init);
        let interp = {
            let mut vm = Vm::from_context(&mut ev);
            vm.execute(&f, vec![])
        };
        let want_state = snap(&ev, &var_ids);

        // Hold BOTH tiers to the interpreter's result and side-effect state,
        // independently of which one the tier gate would pick: a loop-free
        // VarSet body now takes the MIR tier (through the baseline-emitter
        // adapter), which would otherwise leave the baseline's own arms
        // unfuzzed. The baseline must always lower these bodies.
        let baseline = lower_leaf(&ops, &constants, 0)
            .unwrap_or_else(|e| panic!("seed {seed}: baseline must lower, got {e}: {ops:?}"));
        check_state_contract(
            &mut ev,
            ctx_ptr,
            &baseline,
            &f,
            &ops,
            &interp,
            &want_state,
            &var_ids,
            &init,
            seed,
            "baseline",
        );
        if let Ok(mir) = mir::build_mir(&ops, &constants, 0)
            && let Ok(mleaf) = lower_mir_pure(&mir)
        {
            check_state_contract(
                &mut ev,
                ctx_ptr,
                &mleaf,
                &f,
                &ops,
                &interp,
                &want_state,
                &var_ids,
                &init,
                seed,
                "mir",
            );
        }
    }

    /// Run `leaf` from the initial variable state and hold it to the
    /// interpreter's result and final state, resuming any deopt on the Tier-0
    /// interpreter exactly as the dispatch would.
    #[allow(clippy::too_many_arguments)]
    fn check_state_contract<E: std::fmt::Debug>(
        ev: &mut Context,
        ctx_ptr: *mut u8,
        leaf: &CompiledLeaf,
        f: &ByteCodeFunction,
        ops: &[Op],
        interp: &Result<Value, E>,
        want_state: &[Option<usize>],
        var_ids: &[SymId],
        init: &[Value],
        seed: u64,
        tier: &str,
    ) {
        reset(ev, var_ids, init);
        let init_state = snap(ev, var_ids);
        match leaf.call(ctx_ptr, &[]) {
            NativeRun::Ok(bits) => {
                let want = interp.as_ref().unwrap_or_else(|e| {
                    panic!("seed {seed} [{tier}]: JIT Ok but interpreter erred ({e:?}): {ops:?}")
                });
                assert_eq!(
                    bits,
                    want.bits(),
                    "seed {seed} [{tier}]: result mismatch on {ops:?}"
                );
                assert_eq!(
                    snap(ev, var_ids),
                    want_state,
                    "seed {seed} [{tier}]: SIDE-EFFECT STATE mismatch (a VarSet was dropped or mis-lowered) on {ops:?}"
                );
            }
            NativeRun::Deopt => {
                // Rerun-from-start is only sound if no side effect ran yet
                // (VarSet poisons later guards) — the vars must be untouched.
                assert_eq!(
                    snap(ev, var_ids),
                    init_state,
                    "seed {seed} [{tier}]: rerun-from-start deopt AFTER a side effect on {ops:?}"
                );
                let rerun = {
                    let mut vm = Vm::from_context(ev);
                    vm.execute(f, vec![])
                };
                match (&rerun, interp) {
                    (Ok(got), Ok(want)) => {
                        assert_eq!(got.bits(), want.bits(), "seed {seed} [{tier}]: {ops:?}");
                        assert_eq!(
                            snap(ev, var_ids),
                            want_state,
                            "seed {seed} [{tier}]: {ops:?}"
                        );
                    }
                    // Both signal at the same op of the same deterministic
                    // prefix, so the partial writes must agree too.
                    (Err(_), Err(_)) => assert_eq!(
                        snap(ev, var_ids),
                        want_state,
                        "seed {seed} [{tier}]: state after a deopt-rerun error: {ops:?}"
                    ),
                    other => {
                        panic!("seed {seed} [{tier}]: deopt-rerun mismatch {other:?}: {ops:?}")
                    }
                }
            }
            NativeRun::DeoptAt(resume) => {
                let DeoptResume {
                    pc,
                    stack,
                    handlers,
                    binds,
                    spec_base,
                    cond_base,
                } = *resume;
                // Precise deopt: resume mid-function on the MUTATED state.
                let resumed = {
                    let mut vm = Vm::from_context(ev);
                    vm.run_resumed_frame(
                        f,
                        Value::NIL,
                        pc,
                        &stack,
                        handlers,
                        &binds,
                        spec_base,
                        cond_base,
                    )
                };
                match (&resumed, interp) {
                    (Ok(got), Ok(want)) => {
                        assert_eq!(got.bits(), want.bits(), "seed {seed} [{tier}]: {ops:?}");
                        assert_eq!(
                            snap(ev, var_ids),
                            want_state,
                            "seed {seed} [{tier}]: {ops:?}"
                        );
                    }
                    (Err(_), Err(_)) => assert_eq!(
                        snap(ev, var_ids),
                        want_state,
                        "seed {seed} [{tier}]: state after a resumed error: {ops:?}"
                    ),
                    other => panic!("seed {seed} [{tier}]: resume mismatch {other:?}: {ops:?}"),
                }
            }
            NativeRun::Signal => {
                let _ = take_pending_flow();
                assert!(
                    interp.is_err(),
                    "seed {seed} [{tier}]: JIT signaled but interpreter succeeded: {ops:?}"
                );
                // Same deterministic prefix ran on both engines before the
                // signal, so the partial writes must agree too.
                assert_eq!(
                    snap(ev, var_ids),
                    want_state,
                    "seed {seed} [{tier}]: state mismatch after signal on {ops:?}"
                );
            }
        }
    }
}

/// B1 (C1): a slot the AOT loader DISARMED (`epoch == SPEC_EPOCH_DISARMED`)
/// reports NOT-armed and NEVER re-arms — even when the live binding would
/// otherwise re-validate. Proves the shared subr/pred/eq arming helper
/// short-circuits on the sentinel BEFORE any obarray re-validation, so a
/// mis-baked kind can never run the wrong op. (JIT never sets DISARMED; this
/// path is reached only by loader-armed AOT leaves — see the x-session tests.)
#[test]
fn disarmed_spec_slot_never_arms_and_does_not_rearm() {
    use crate::emacs_core::eval::Context;
    let ev = Context::new();
    // Control precondition: no compiler function overrides active (else the
    // helper returns false regardless — the assumption the control relies on).
    assert!(
        !ev.compiler_function_overrides_active(),
        "test assumes no active compiler function overrides"
    );
    // `car` is a canonical builtin fbound in every obarray; use its real
    // binding as the (would-be) callee VALUE so the helper COULD re-validate.
    let car = match Value::symbol("car").kind() {
        crate::emacs_core::value::ValueKind::Symbol(id) => id,
        _ => panic!("symbol"),
    };
    let expected = ev
        .obarray
        .symbol_function_id(car)
        .expect("car fbound")
        .bits() as i64;
    let disarmed = SpecSlot::at_epoch(SPEC_EPOCH_DISARMED);
    // Even though (sym, expected) MATCHES the live binding, the DISARMED
    // sentinel forces `false` and leaves the epoch untouched (no re-arm).
    assert!(
        !subr_spec_armed(&ev, car.0 as i64, expected, &disarmed),
        "a DISARMED slot must report not-armed"
    );
    assert_eq!(
        disarmed.epoch.load(Ordering::Relaxed),
        SPEC_EPOCH_DISARMED,
        "a DISARMED slot must not re-arm (epoch unchanged)"
    );
    // Control: the SAME (sym, expected) on a fresh slot DOES arm via the
    // re-validate path — so the assertion above proves the guard, not a dead
    // binding. (A fresh epoch of 0 forces the re-validate branch, which stores
    // the live epoch and returns true because `expected` matches the cell.)
    let fresh = SpecSlot::at_epoch(0);
    assert!(
        subr_spec_armed(&ev, car.0 as i64, expected, &fresh),
        "control: a matching live binding arms a non-disarmed slot"
    );
}

/// B1 (C2): `SPEC_EPOCH_DISARMED` is the reserved `u64::MAX`, and the obarray
/// never hands out a live `function_epoch` equal to it (the bump skips it).
#[test]
fn function_epoch_never_equals_disarmed_sentinel() {
    assert_eq!(SPEC_EPOCH_DISARMED, u64::MAX);
    let mut ev = crate::emacs_core::eval::Context::new();
    for _ in 0..8 {
        ev.obarray
            .bump_function_epoch(crate::emacs_core::symbol::FunctionEpochBump::SubrRewrite);
        assert_ne!(
            ev.obarray.function_epoch(),
            SPEC_EPOCH_DISARMED,
            "a live function_epoch must never equal the DISARMED sentinel"
        );
    }
}

/// COMMIT A compile-time assert: not one `SubrFn::Many` allowlist name is a
/// known `ManySlice` variadic — the two sets are disjoint by construction, so
/// no arithmetic/list ManySlice builtin can ever leak onto the allowlist.
#[test]
fn subr_spec_many_allowlist_disjoint_from_manyslice() {
    const MANYSLICE: &[&str] = &[
        "+",
        "logand",
        "logior",
        "logxor",
        "list",
        "vector",
        "append",
        "nconc",
        "string-match",
    ];
    for name in SUBR_MANY_ALLOWLIST {
        assert!(
            !MANYSLICE.contains(name),
            "{name:?} is a ManySlice variadic and must not be on SUBR_MANY_ALLOWLIST"
        );
    }
}

/// COMMIT A ManySlice-rejection: the classifier ACCEPTS every allowlisted
/// `SubrFn::Many` builtin (as `SubrGeneral`) at a representative in-range
/// arity, and REJECTS every registered `ManySlice` variadic
/// (`+`/logand/logior/logxor/list/vector/append/nconc/string-match). The
/// `SubrFn::Many` match in `subr_spec_kind` — NOT the allowlist — does the
/// ManySlice exclusion, so no ManySlice subr ever classifies regardless of
/// what the allowlist names.
#[test]
fn subr_spec_kind_rejects_registered_manyslice() {
    use crate::emacs_core::eval::Context;
    let ev = Context::new();
    let sid = |name: &str| match Value::symbol(name).kind() {
        crate::emacs_core::value::ValueKind::Symbol(id) => id,
        _ => panic!("symbol"),
    };
    // ACCEPT: each allowlisted Many builtin classifies as SubrGeneral.
    for (name, nargs) in [
        ("re-search-forward", 1usize),
        ("looking-at", 1),
        ("parse-partial-sexp", 2),
        ("match-data", 0),
        ("set-match-data", 1),
        ("scan-sexps", 2),
        ("intern-soft", 1),
        ("line-end-position", 0),
        ("syntax-table", 0),
        ("set-syntax-table", 1),
        ("put-text-property", 4),
    ] {
        let id = sid(name);
        let binding = ev
            .obarray
            .symbol_function_id(id)
            .unwrap_or_else(|| panic!("{name} fbound"));
        assert_eq!(
            subr_spec_kind(binding, id, nargs),
            Some(SpecCalleeKind::SubrGeneral),
            "{name} (allowlisted Many) must classify as SubrGeneral"
        );
    }
    // REJECT: every registered ManySlice variadic (EXCEPT the bitwise-arith
    // intrinsics logand/logior/logxor, checked separately below) stays generic
    // at any arity.
    for name in ["+", "list", "vector", "append", "nconc", "string-match"] {
        let id = sid(name);
        let binding = ev
            .obarray
            .symbol_function_id(id)
            .unwrap_or_else(|| panic!("{name} fbound"));
        for nargs in [0usize, 2, 4] {
            assert_eq!(
                subr_spec_kind(binding, id, nargs),
                None,
                "{name} is ManySlice and must NEVER classify (nargs={nargs})"
            );
        }
    }
}

/// The bitwise-arith intrinsics (logand/logior/logxor) — `ManySlice` variadics
/// that would otherwise get full generic dispatch — classify as
/// `ArithIntrinsic` at EXACTLY 2 args (the GC-free fixnum fast path), and stay
/// generic (`None`) at every other arity (0=const, 1=identity, ≥3=reduction).
#[test]
fn subr_spec_kind_classifies_bitwise_arith_at_two_args() {
    use crate::emacs_core::eval::Context;
    let ev = Context::new();
    let sid = |name: &str| match Value::symbol(name).kind() {
        crate::emacs_core::value::ValueKind::Symbol(id) => id,
        _ => panic!("symbol"),
    };
    // (name, op, the ONE arity that intrinsifies) — every other arity stays generic.
    for (name, op, good_arity) in [
        ("logand", ARITH_KIND_LOGAND as u8, 2usize),
        ("logior", ARITH_KIND_LOGIOR as u8, 2),
        ("logxor", ARITH_KIND_LOGXOR as u8, 2),
        ("ash", ARITH_KIND_ASH as u8, 2),
        ("lognot", ARITH_KIND_LOGNOT as u8, 1),
    ] {
        let id = sid(name);
        let binding = ev
            .obarray
            .symbol_function_id(id)
            .unwrap_or_else(|| panic!("{name} fbound"));
        assert_eq!(
            subr_spec_kind(binding, id, good_arity),
            Some(SpecCalleeKind::ArithIntrinsic { op }),
            "{name} at {good_arity} args must intrinsify with op {op}"
        );
        // Each op gets a distinct discriminant (AOT loader disarms on mismatch).
        assert_eq!(
            SpecCalleeKind::ArithIntrinsic { op }.to_spec_disc(),
            Some(5 + op),
            "{name} disc is 5+op"
        );
        // At any OTHER arity the site must not classify as ArithIntrinsic
        // (fixed-arity ash/lognot become None on arity mismatch; the ManySlice
        // and/or/xor become None too — never a bit-op intrinsic).
        for nargs in [0usize, 1, 2, 3, 4] {
            if nargs == good_arity {
                continue;
            }
            assert!(
                !matches!(
                    subr_spec_kind(binding, id, nargs),
                    Some(SpecCalleeKind::ArithIntrinsic { .. })
                ),
                "{name} must not arith-intrinsify at nargs={nargs}"
            );
        }
    }
    // The five discs are pairwise distinct and within DISC_COUNT.
    let discs: Vec<u8> = [
        ARITH_KIND_LOGAND,
        ARITH_KIND_LOGIOR,
        ARITH_KIND_LOGXOR,
        ARITH_KIND_ASH,
        ARITH_KIND_LOGNOT,
    ]
    .iter()
    .map(|&op| {
        SpecCalleeKind::ArithIntrinsic { op: op as u8 }
            .to_spec_disc()
            .unwrap()
    })
    .collect();
    assert_eq!(discs, vec![5, 6, 7, 8, 9]);
    assert!(discs.iter().all(|&d| d < SpecCalleeKind::DISC_COUNT));
}

/// The `ash_fixnum_fast` helper matches GNU `Fash` for the fixnum cases and
/// returns `None` exactly when the result would leave fixnum range (→ generic
/// bignum path).
#[test]
fn ash_fixnum_fast_matches_gnu_and_defers_overflow() {
    // Left shifts that stay in range.
    assert_eq!(ash_fixnum_fast(1, 4), Some(16));
    assert_eq!(ash_fixnum_fast(-1, 1), Some(-2));
    assert_eq!(ash_fixnum_fast(3, 0), Some(3));
    // Right shifts (arithmetic, floor toward -inf) — always a fixnum.
    assert_eq!(ash_fixnum_fast(16, -2), Some(4));
    assert_eq!(ash_fixnum_fast(-3, -1), Some(-2)); // floor(-1.5) = -2
    assert_eq!(ash_fixnum_fast(1, -100), Some(0)); // shifted away -> 0
    assert_eq!(ash_fixnum_fast(-1, -100), Some(-1)); // negative -> -1
    // Left shift that overflows fixnum range -> None (generic makes a bignum).
    assert_eq!(ash_fixnum_fast(1, 61), None); // 2^61 > MOST_POSITIVE_FIXNUM
    assert_eq!(ash_fixnum_fast(Value::MOST_POSITIVE_FIXNUM, 1), None);
    assert_eq!(ash_fixnum_fast(1, 64), None); // >= 64: undefined shift, defer
    assert_eq!(ash_fixnum_fast(1, 1000), None);
    // Largest in-range left shift boundary.
    assert_eq!(ash_fixnum_fast(1, 60), Some(1i64 << 60));
}

// ---- Panic containment at the shim boundary ----

/// A leaf whose `Op::Call` invokes the always-registered internal panic
/// subr: the panic originates in host code reached through
/// `neovm_jit_call`, the same class a buggy builtin would raise.
fn panicking_call_leaf(msg: &str) -> CompiledLeaf {
    lower_nullary_leaf(
        &[Op::Constant(0), Op::Constant(1), Op::Call(1), Op::Return],
        &[Value::symbol("neovm--internal-panic"), Value::string(msg)],
    )
    .expect("call body compiles")
}

#[test]
fn contained_shim_panic_surfaces_as_error_flow_and_vm_survives() {
    let mut ev = crate::emacs_core::eval::Context::new();
    let ctx_ptr = &mut ev as *mut crate::emacs_core::eval::Context as *mut u8;
    let leaf = panicking_call_leaf("shim-boom");
    assert_eq!(leaf.call(ctx_ptr, &[]), NativeRun::Signal);
    let flow = take_pending_flow().expect("contained panic stashes a flow");
    let Flow::Signal(sig) = flow else {
        panic!("expected Signal, got {flow:?}");
    };
    assert_eq!(sig.symbol_name(), "error");
    let msg = sig.data[0].as_str_owned().expect("string payload");
    assert!(
        msg.contains("neomacs internal error") && msg.contains("shim-boom"),
        "unexpected message: {msg}"
    );
    // No one-shot state: containment works again.
    assert_eq!(leaf.call(ctx_ptr, &[]), NativeRun::Signal);
    let _ = take_pending_flow().expect("second containment works");
    // The evaluator survives: a normal compiled call and a full GC run.
    let ok = lower_nullary_leaf(&[Op::Constant(0), Op::Return], &[Value::make_int(7)])
        .expect("trivial body compiles");
    assert_eq!(
        ok.call(ctx_ptr, &[]),
        NativeRun::Ok(Value::make_int(7).bits())
    );
    ev.funcall_general_untraced(Value::symbol("garbage-collect"), vec![])
        .expect("garbage-collect succeeds after containment");
}

#[test]
fn contained_shim_panic_restores_boundary_state() {
    let mut ev = crate::emacs_core::eval::Context::new();
    let ctx_ptr = &mut ev as *mut crate::emacs_core::eval::Context as *mut u8;
    // Interpreted middle function: dynamically binds, then calls the
    // panicking subr — the panic unwinds through its LIVE interpreter
    // frame, skipping cleanup_bytecode_frame (the bc_frames pop + depth
    // decrement) and its Unbind. Exactly the residue the leaf-exit
    // healing must truncate / the leaf-exit unwind must sweep.
    let var = Value::symbol("jit-t5-dynvar");
    let mid_sym = Value::symbol("jit-t5-middle");
    let crate::emacs_core::value::ValueKind::Symbol(mid_id) = mid_sym.kind() else {
        panic!("symbol expected");
    };
    let mut mid = ByteCodeFunction::new(LambdaParams {
        required: Vec::new(),
        optional: Vec::new(),
        rest: None,
    });
    mid.lexical = true;
    mid.ops = vec![
        Op::Constant(1), // 5
        Op::VarBind(0),  // bind jit-t5-dynvar := 5 (leaked by the panic)
        Op::Constant(2), // 'neovm--internal-panic
        Op::Constant(3), // "mid-boom"
        Op::Call(1),
        Op::Unbind(1),
        Op::Return,
    ];
    mid.constants = vec![
        var,
        Value::make_int(5),
        Value::symbol("neovm--internal-panic"),
        Value::string("mid-boom"),
    ]
    .into();
    mid.max_stack = 16;
    ev.obarray
        .set_symbol_function_id(mid_id, Value::make_bytecode(mid));
    // The leaf binds too, so it carries has_binds: its exit parity unwind
    // is the depth-based sweep that must also collect the middle's leaked
    // binding (the deferred-specpdl half of the recovery contract).
    let leaf = lower_nullary_leaf(
        &[
            Op::Constant(1), // 5
            Op::VarBind(0),  // leaf's own binding
            Op::Constant(2), // 'jit-t5-middle
            Op::Call(0),
            Op::Unbind(1),
            Op::Return,
        ],
        &[var, Value::make_int(5), mid_sym],
    )
    .expect("binding call body compiles");
    let depth0 = ev.depth;
    let frames0 = ev.bc_frames.len();
    let buf0 = ev.bc_buf.len();
    let cond0 = ev.condition_stack.len();
    let spec0 = ev.specpdl.len();
    let roots0 = crate::emacs_core::eval::save_scratch_gc_roots();
    assert_eq!(leaf.call(ctx_ptr, &[]), NativeRun::Signal);
    let _ = take_pending_flow().expect("panic flow stashed");
    assert_eq!(ev.depth, depth0, "lisp depth restored");
    assert_eq!(ev.bc_frames.len(), frames0, "bc_frames truncated");
    assert_eq!(ev.bc_buf.len(), buf0, "bc_buf truncated");
    assert_eq!(
        ev.condition_stack.len(),
        cond0,
        "condition frames truncated"
    );
    assert_eq!(
        ev.specpdl.len(),
        spec0,
        "specpdl unwound (leaf bind + leaked middle bind) at leaf exit"
    );
    assert_eq!(
        crate::emacs_core::eval::save_scratch_gc_roots(),
        roots0,
        "scratch roots restored"
    );
}

/// A panic contained inside a DIRECT callee's own call shim (a handler-free
/// leaf, run in the caller's extent) keeps its marker for the caller leaf's
/// healing exit on both routes through the spec shim -- the slow half that
/// arms the site and the fast path after it. Folding it into a signal on
/// the way back consumed the marker, and the residue of the panicked
/// extent (the operand buffer, the roots, the depth) stayed.
#[test]
fn a_panic_contained_in_a_direct_callee_is_healed_by_the_caller() {
    use crate::emacs_core::eval::Context;
    crate::emacs_core::jit::compile::force_profit_gate_for_test(false);
    let mut ev = Context::new();
    let ctx_ptr = &mut ev as *mut Context as *mut u8;
    let nullary = |ops: Vec<Op>, constants: Vec<Value>| -> ByteCodeFunction {
        let mut f = ByteCodeFunction::new(LambdaParams {
            required: Vec::new(),
            optional: Vec::new(),
            rest: None,
        });
        f.lexical = true;
        f.ops = ops;
        f.constants = constants.into();
        f.max_stack = 16;
        f
    };
    // (lambda () (neovm--internal-panic "raw-boom")): no binding, no
    // handler -- a direct-eligible leaf; the panic is contained by its own
    // generic call shim, which returns STATUS_SIGNAL to the leaf.
    let callee = Value::make_bytecode(nullary(
        vec![Op::Constant(0), Op::Constant(1), Op::Call(1), Op::Return],
        vec![
            Value::symbol("neovm--internal-panic"),
            Value::string("raw-boom"),
        ],
    ));
    let callee_sym = Value::symbol("jit-t7-callee");
    let crate::emacs_core::value::ValueKind::Symbol(callee_id) = callee_sym.kind() else {
        panic!("symbol expected");
    };
    ev.obarray.set_symbol_function_id(callee_id, callee);
    // (lambda () (let ((jit-t7-dynvar 5)) (jit-t7-callee))): the caller
    // speculates the call; its binding makes its exit the specpdl sweep.
    let caller = nullary(
        vec![
            Op::Constant(1),
            Op::VarBind(0),
            Op::Constant(2),
            Op::Call(0),
            Op::Unbind(1),
            Op::Return,
        ],
        vec![
            Value::symbol("jit-t7-dynvar"),
            Value::make_int(5),
            callee_sym,
        ],
    );
    let leaf = compile_bytecode_function_with(&caller, Some(&ev.obarray)).expect("compiles");
    let depth0 = ev.depth;
    let frames0 = ev.bc_frames.len();
    let buf0 = ev.bc_buf.len();
    let cond0 = ev.condition_stack.len();
    let spec0 = ev.specpdl.len();
    let roots0 = crate::emacs_core::eval::save_scratch_gc_roots();
    let shim_fast =
        || crate::emacs_core::jit::compile::SPEC_SHIM_FAST_COUNT.load(Ordering::Relaxed);
    for (i, label) in ["arming call (slow half)", "fast-path call"]
        .iter()
        .enumerate()
    {
        let before = shim_fast();
        assert_eq!(leaf.call(ctx_ptr, &[]), NativeRun::Signal, "{label}");
        if i == 1 {
            assert!(shim_fast() > before, "the second call takes the fast path");
        }
        let flow = take_pending_flow().expect("the contained panic stashes a flow");
        let Flow::Signal(sig) = flow else {
            panic!("{label}: expected Signal, got {flow:?}");
        };
        assert_eq!(sig.symbol_name(), "error", "{label}");
        let msg = sig.data[0].as_str_owned().expect("string payload");
        assert!(
            msg.contains("neomacs internal error") && msg.contains("raw-boom"),
            "{label}: unexpected message: {msg}"
        );
        assert_eq!(ev.depth, depth0, "{label}: lisp depth restored");
        assert_eq!(ev.bc_frames.len(), frames0, "{label}: bc_frames truncated");
        assert_eq!(ev.bc_buf.len(), buf0, "{label}: bc_buf truncated");
        assert_eq!(
            ev.condition_stack.len(),
            cond0,
            "{label}: condition frames truncated"
        );
        assert_eq!(
            ev.specpdl.len(),
            spec0,
            "{label}: specpdl unwound at the caller's exit"
        );
        assert_eq!(
            crate::emacs_core::eval::save_scratch_gc_roots(),
            roots0,
            "{label}: scratch roots restored"
        );
    }
}

/// A panic contained on the spec shim's FAST path -- raised by Lisp the
/// framed callee runs on its way out (an `unwind-protect` cleanup, run by
/// the interpreter, whose live frames the panic skips) -- leaves the
/// pending-panic marker for the caller leaf's healing exit exactly as the
/// slow half does: the caller's exit restores the boundary (depth, bytecode
/// frames, condition frames) and sweeps the specpdl. Folding the panic into
/// a signal inside the shim would consume the marker first and leave that
/// residue in place.
#[test]
fn a_panic_contained_on_the_spec_fast_path_is_healed_by_the_caller() {
    use crate::emacs_core::eval::Context;
    crate::emacs_core::jit::compile::force_profit_gate_for_test(false);
    let mut ev = Context::new();
    let ctx_ptr = &mut ev as *mut Context as *mut u8;
    let nullary = |ops: Vec<Op>, constants: Vec<Value>| -> ByteCodeFunction {
        let mut f = ByteCodeFunction::new(LambdaParams {
            required: Vec::new(),
            optional: Vec::new(),
            rest: None,
        });
        f.lexical = true;
        f.ops = ops;
        f.constants = constants.into();
        f.max_stack = 16;
        f
    };
    // (lambda () (neovm--internal-panic "fast-boom")): the cleanup, run by
    // the interpreter when the callee's unwind-protect record unwinds.
    let cleanup = Value::make_bytecode(nullary(
        vec![Op::Constant(0), Op::Constant(1), Op::Call(1), Op::Return],
        vec![
            Value::symbol("neovm--internal-panic"),
            Value::string("fast-boom"),
        ],
    ));
    // (lambda () (unwind-protect (signal 'error nil) CLEANUP)): a framed
    // callee (the unwind-protect record) whose body signals, so its exit
    // runs the cleanup.
    let callee = Value::make_bytecode(nullary(
        vec![
            Op::Constant(0),
            Op::UnwindProtectPop,
            Op::Constant(1),
            Op::Constant(2),
            Op::Constant(3),
            Op::Call(2),
            Op::Unbind(1),
            Op::Return,
        ],
        vec![
            cleanup,
            Value::symbol("signal"),
            Value::symbol("error"),
            Value::NIL,
        ],
    ));
    let callee_sym = Value::symbol("jit-t6-callee");
    let crate::emacs_core::value::ValueKind::Symbol(callee_id) = callee_sym.kind() else {
        panic!("symbol expected");
    };
    ev.obarray.set_symbol_function_id(callee_id, callee);
    // (lambda () (let ((jit-t6-dynvar 5)) (jit-t6-callee))): the caller
    // speculates the call; its own binding makes its exit the depth-based
    // specpdl sweep that collects what the panic left.
    let caller = nullary(
        vec![
            Op::Constant(1),
            Op::VarBind(0),
            Op::Constant(2),
            Op::Call(0),
            Op::Unbind(1),
            Op::Return,
        ],
        vec![
            Value::symbol("jit-t6-dynvar"),
            Value::make_int(5),
            callee_sym,
        ],
    );
    let leaf = compile_bytecode_function_with(&caller, Some(&ev.obarray)).expect("compiles");
    let depth0 = ev.depth;
    let frames0 = ev.bc_frames.len();
    let buf0 = ev.bc_buf.len();
    let cond0 = ev.condition_stack.len();
    let spec0 = ev.specpdl.len();
    let roots0 = crate::emacs_core::eval::save_scratch_gc_roots();
    let shim_fast =
        || crate::emacs_core::jit::compile::SPEC_SHIM_FAST_COUNT.load(Ordering::Relaxed);
    // The first call arms the site (the slow half contains the panic); the
    // second takes the fast path and must leave the same state behind.
    for (i, label) in ["arming call", "fast-path call"].iter().enumerate() {
        let before = shim_fast();
        assert_eq!(leaf.call(ctx_ptr, &[]), NativeRun::Signal, "{label}");
        if i == 1 {
            assert!(shim_fast() > before, "the second call takes the fast path");
        }
        let flow = take_pending_flow().expect("the contained panic stashes a flow");
        let Flow::Signal(sig) = flow else {
            panic!("{label}: expected Signal, got {flow:?}");
        };
        assert_eq!(sig.symbol_name(), "error", "{label}");
        let msg = sig.data[0].as_str_owned().expect("string payload");
        assert!(
            msg.contains("neomacs internal error") && msg.contains("fast-boom"),
            "{label}: unexpected message: {msg}"
        );
        assert_eq!(ev.depth, depth0, "{label}: lisp depth restored");
        assert_eq!(ev.bc_frames.len(), frames0, "{label}: bc_frames truncated");
        assert_eq!(ev.bc_buf.len(), buf0, "{label}: bc_buf truncated");
        assert_eq!(
            ev.condition_stack.len(),
            cond0,
            "{label}: condition frames truncated"
        );
        assert_eq!(
            ev.specpdl.len(),
            spec0,
            "{label}: specpdl unwound at the caller's exit"
        );
        assert_eq!(
            crate::emacs_core::eval::save_scratch_gc_roots(),
            roots0,
            "{label}: scratch roots restored"
        );
    }
}

#[test]
fn contained_shim_panic_is_caught_by_leaf_local_condition_case() {
    let mut ev = crate::emacs_core::eval::Context::new();
    let ctx_ptr = &mut ev as *mut crate::emacs_core::eval::Context as *mut u8;
    let cond0 = ev.condition_stack.len();
    // (condition-case around the panicking call, in the SAME compiled
    // function): the contained panic must flow through the match shim and
    // resume at this leaf's own handler, like any Lisp error.
    let leaf = lower_nullary_leaf(
        &[
            Op::PushConditionCase(6),
            Op::Constant(0), // 'neovm--internal-panic
            Op::Constant(1), // "caught-locally"
            Op::Call(1),
            Op::PopHandler,
            Op::Return,
            Op::Return, // 6: handler entry [err]
        ],
        &[
            Value::symbol("neovm--internal-panic"),
            Value::string("caught-locally"),
        ],
    )
    .expect("handler body compiles");
    let NativeRun::Ok(bits) = leaf.call(ctx_ptr, &[]) else {
        panic!("expected the leaf-local handler to catch the contained panic");
    };
    let err = Value::from_bits(bits);
    assert_eq!(
        err.cons_car().as_symbol_name(),
        Some("error"),
        "binding is (error ...)"
    );
    let msg = err
        .cons_cdr()
        .cons_car()
        .as_str_owned()
        .expect("message string");
    assert!(
        msg.contains("neomacs internal error") && msg.contains("caught-locally"),
        "unexpected message: {msg}"
    );
    assert_eq!(ev.condition_stack.len(), cond0, "handler frame consumed");
}

#[test]
fn contained_shim_panic_with_leaked_callee_handler_still_matches_leaf_handler() {
    let mut ev = crate::emacs_core::eval::Context::new();
    let ctx_ptr = &mut ev as *mut crate::emacs_core::eval::Context as *mut u8;
    // Interpreted middle function whose OWN condition-case protects the
    // panicking call: the Rust panic (not a Lisp signal) unwinds straight
    // through the interpreter, so the middle's handler never runs and its
    // condition frame is LEAKED above the leaf's — exactly the residue
    // that would desynchronize the match shim's count-based pops and let
    // the innermost-match scan select the dead frame. The match-entry
    // healing must truncate it so the LEAF's handler catches.
    let mid_sym = Value::symbol("jit-t5-shielded-middle");
    let crate::emacs_core::value::ValueKind::Symbol(mid_id) = mid_sym.kind() else {
        panic!("symbol expected");
    };
    let mut mid = ByteCodeFunction::new(LambdaParams {
        required: Vec::new(),
        optional: Vec::new(),
        rest: None,
    });
    mid.lexical = true;
    mid.ops = vec![
        Op::PushConditionCase(6),
        Op::Constant(0), // 'neovm--internal-panic
        Op::Constant(1), // "resid-boom"
        Op::Call(1),
        Op::PopHandler,
        Op::Return,
        Op::Return, // 6: mid's handler (unreachable — panics skip it)
    ];
    mid.constants = vec![
        Value::symbol("neovm--internal-panic"),
        Value::string("resid-boom"),
    ]
    .into();
    mid.max_stack = 16;
    ev.obarray
        .set_symbol_function_id(mid_id, Value::make_bytecode(mid));
    let leaf = lower_nullary_leaf(
        &[
            Op::PushConditionCase(5),
            Op::Constant(0), // 'jit-t5-shielded-middle
            Op::Call(0),
            Op::PopHandler,
            Op::Return,
            Op::Return, // 5: leaf's handler entry [err]
        ],
        &[mid_sym],
    )
    .expect("handler body compiles");
    // Warm one round (interning, lazies) before taking the bases.
    let NativeRun::Ok(_) = leaf.call(ctx_ptr, &[]) else {
        panic!("leaf handler must catch the contained panic");
    };
    let cond0 = ev.condition_stack.len();
    let depth0 = ev.depth;
    let frames0 = ev.bc_frames.len();
    let roots0 = crate::emacs_core::eval::save_scratch_gc_roots();
    for _ in 0..2 {
        let NativeRun::Ok(bits) = leaf.call(ctx_ptr, &[]) else {
            panic!("leaf handler must catch the contained panic");
        };
        let err = Value::from_bits(bits);
        assert_eq!(
            err.cons_car().as_symbol_name(),
            Some("error"),
            "binding is (error ...)"
        );
        let msg = err
            .cons_cdr()
            .cons_car()
            .as_str_owned()
            .expect("message string");
        assert!(
            msg.contains("neomacs internal error") && msg.contains("resid-boom"),
            "unexpected message: {msg}"
        );
    }
    assert_eq!(
        ev.condition_stack.len(),
        cond0,
        "leaked callee frame truncated + leaf frame consumed, every round"
    );
    assert_eq!(ev.depth, depth0, "lisp depth healed at the match shim");
    assert_eq!(
        ev.bc_frames.len(),
        frames0,
        "bc_frames healed at the match shim"
    );
    assert_eq!(
        crate::emacs_core::eval::save_scratch_gc_roots(),
        roots0,
        "root residue of locally-caught panics swept at leaf exit"
    );
}

#[test]
fn contained_shim_panics_leave_no_residue_over_repeats() {
    let mut ev = crate::emacs_core::eval::Context::new();
    let ctx_ptr = &mut ev as *mut crate::emacs_core::eval::Context as *mut u8;
    // A BINDING leaf: containment defers specpdl to the next depth-based
    // unwind, which for a `has_binds` leaf is its own exit parity unwind
    // (in production a bind-less leaf is always under an enclosing frame
    // whose `cleanup_bytecode_frame`/handler unwind does the same sweep;
    // this raw `leaf.call` harness has no enclosing frame, so the leaf
    // supplies the sweep itself — the production shape, minus the middle
    // man). The callee-dispatch backtrace entry the panic leaks each
    // round must be collected by that unwind.
    let leaf = lower_nullary_leaf(
        &[
            Op::Constant(2), // 5
            Op::VarBind(1),  // bind jit-t5-loopvar
            Op::Constant(0), // 'neovm--internal-panic
            Op::Constant(3), // "looped"
            Op::Call(1),
            Op::Unbind(1),
            Op::Return,
        ],
        &[
            Value::symbol("neovm--internal-panic"),
            Value::symbol("jit-t5-loopvar"),
            Value::make_int(5),
            Value::string("looped"),
        ],
    )
    .expect("binding call body compiles");
    // Warm one containment (interning, lazies) before taking the bases.
    assert_eq!(leaf.call(ctx_ptr, &[]), NativeRun::Signal);
    let _ = take_pending_flow();
    let roots0 = crate::emacs_core::eval::save_scratch_gc_roots();
    let base = (
        ev.depth,
        ev.bc_frames.len(),
        ev.bc_buf.len(),
        ev.condition_stack.len(),
        ev.specpdl.len(),
    );
    for _ in 0..16 {
        assert_eq!(leaf.call(ctx_ptr, &[]), NativeRun::Signal);
        let _ = take_pending_flow().expect("flow each iteration");
    }
    assert_eq!(
        crate::emacs_core::eval::save_scratch_gc_roots(),
        roots0,
        "scratch-root depth stable over repeated containments"
    );
    assert_eq!(
        (
            ev.depth,
            ev.bc_frames.len(),
            ev.bc_buf.len(),
            ev.condition_stack.len(),
            ev.specpdl.len(),
        ),
        base,
        "no per-containment residue"
    );
}

#[test]
fn gc_suspect_shim_panic_is_re_raised_not_contained() {
    let mut ev = crate::emacs_core::eval::Context::new();
    let ctx_ptr = &mut ev as *mut crate::emacs_core::eval::Context as *mut u8;
    ev.gc_driver_active = true;
    let payload: Box<dyn std::any::Any + Send> = Box::new("must-flee".to_string());
    let back = contain_jit_shim_panic(ctx_ptr, payload)
        .expect_err("GC-suspect panic must be re-raised, not contained");
    assert_eq!(back.downcast_ref::<String>().unwrap(), "must-flee");
    ev.gc_driver_active = false;
    assert!(
        take_pending_flow().is_none(),
        "nothing stashed on the re-raise path"
    );
}

#[test]
fn contained_panic_wins_over_stale_pending_flow() {
    // A shim body that stashed a real flow and THEN panicked before
    // completing its protocol: the panic must win at take time, and both
    // slots must be consumed.
    let mut ev = crate::emacs_core::eval::Context::new();
    let ctx_ptr = &mut ev as *mut crate::emacs_core::eval::Context as *mut u8;
    stash_pending_flow(signal("arith-error", vec![]));
    let payload: Box<dyn std::any::Any + Send> = Box::new("late-panic");
    contain_jit_shim_panic(ctx_ptr, payload).expect("containable");
    let flow = take_pending_flow().expect("panic flow present");
    let Flow::Signal(sig) = flow else {
        panic!("expected Signal");
    };
    assert_eq!(sig.symbol_name(), "error");
    assert!(
        sig.data[0]
            .as_str_owned()
            .expect("string payload")
            .contains("late-panic")
    );
    assert!(take_pending_flow().is_none(), "both slots consumed");
}

#[test]
fn parked_panic_survives_leaf_exit_cleanup_running_compiled_code() {
    // A contained panic in a has_binds leaf whose LEAKED unwind-protect
    // cleanup signals through COMPILED code: the leaf-exit parity unwind
    // runs the cleanup while the panic is parked, so the inner leaf's
    // stash/take cycle must see ITS arith-error (not the outer panic),
    // and the outer dispatcher's take must still get the panic error
    // afterwards (previously the inner take consumed it and the outer
    // take found nothing — an `.expect` panic inside recovery).
    let mut ev = crate::emacs_core::eval::Context::new();
    let ctx_ptr = &mut ev as *mut crate::emacs_core::eval::Context as *mut u8;
    ev.set_variable("jit-fx-witness", Value::NIL);
    // Cleanup: compiled condition-case around (signal 'arith-error nil),
    // recording the caught err object in the witness variable.
    let mut cleanup = ByteCodeFunction::new(LambdaParams {
        required: Vec::new(),
        optional: Vec::new(),
        rest: None,
    });
    cleanup.lexical = true;
    cleanup.ops = vec![
        Op::PushConditionCase(7),
        Op::Constant(0), // 'signal
        Op::Constant(1), // 'arith-error
        Op::Constant(2), // nil
        Op::Call(2),
        Op::PopHandler,
        Op::Return,
        Op::VarSet(3),   // 7: handler entry [err] -> jit-fx-witness
        Op::Constant(2), // nil
        Op::Return,
    ];
    cleanup.constants = vec![
        Value::symbol("signal"),
        Value::symbol("arith-error"),
        Value::NIL,
        Value::symbol("jit-fx-witness"),
    ]
    .into();
    cleanup.max_stack = 16;
    // Force the cleanup hot so its application inside the parity unwind
    // dispatches through the JIT (engagement asserted below — an
    // interpreted cleanup never touches the pending slots and would
    // pass this test vacuously). The profitability gate would reject
    // this call-only body (calls > arith); bypass it — the test needs
    // THIS body native, profitability is orthogonal.
    force_profit_gate_for_test(false);
    let cleanup_id = cleanup.jit_runtime().compiled_id_or_assign();
    cleanup.jit_runtime().set_hot_for_test();
    let cleanup_val = Value::make_bytecode(cleanup);
    let leaf = lower_nullary_leaf(
        &[
            Op::Constant(0), // cleanup fn value
            Op::UnwindProtectPop,
            Op::Constant(1), // 'neovm--internal-panic
            Op::Constant(2), // "park-boom"
            Op::Call(1),
            Op::Unbind(1),
            Op::Return,
        ],
        &[
            cleanup_val,
            Value::symbol("neovm--internal-panic"),
            Value::string("park-boom"),
        ],
    )
    .expect("unwind-protect body compiles");
    let spec0 = ev.specpdl.len();
    assert_eq!(leaf.call(ctx_ptr, &[]), NativeRun::Signal);
    // The outer dispatcher's take sees the PANIC error, not the
    // cleanup's arith-error and not an empty slot.
    let flow = take_pending_flow().expect("parked panic re-stashed for the dispatcher take");
    let Flow::Signal(sig) = flow else {
        panic!("expected Signal, got {flow:?}");
    };
    assert_eq!(sig.symbol_name(), "error");
    let msg = sig.data[0].as_str_owned().expect("string payload");
    assert!(
        msg.contains("neomacs internal error") && msg.contains("park-boom"),
        "unexpected message: {msg}"
    );
    // Engagement: the cleanup really tiered up and ran native.
    assert!(
        crate::emacs_core::jit::cache::is_compiled_for_test(cleanup_id),
        "cleanup must have compiled — the contamination scenario needs \
         its stash/take cycle to run through the JIT dispatcher"
    );
    // The inner handler saw ITS signal.
    let witness = ev
        .obarray
        .symbol_value("jit-fx-witness")
        .cloned()
        .unwrap_or(Value::NIL);
    assert_eq!(
        witness.cons_car().as_symbol_name(),
        Some("arith-error"),
        "inner handler must catch its own arith-error, not the parked panic"
    );
    assert_eq!(
        ev.specpdl.len(),
        spec0,
        "parity unwind swept the leaf's entries"
    );
    assert!(take_pending_flow().is_none(), "slots clean after the take");
}

#[test]
fn wide_arg_call_panic_releases_backtrace_args_cleanly() {
    // A >= 3-argument call stores its args as `BacktraceArgs::Evaluated`
    // (an index into backtrace_args_stack). A contained panic truncates
    // that stack at the boundary while the callee's Backtrace specpdl
    // entry survives for the deferred parity unwind — whose
    // release_backtrace_args must treat the healed residue as a no-op
    // instead of tripping its LIFO debug_assert (debug builds would
    // otherwise re-panic inside recovery).
    let mut ev = crate::emacs_core::eval::Context::new();
    let ctx_ptr = &mut ev as *mut crate::emacs_core::eval::Context as *mut u8;
    let var = Value::symbol("jit-fx-wide-dynvar");
    let mid_sym = Value::symbol("jit-fx-wide-middle");
    let crate::emacs_core::value::ValueKind::Symbol(mid_id) = mid_sym.kind() else {
        panic!("symbol expected");
    };
    // Interpreted 3-arg middle that panics: its dispatch stores a wide
    // Evaluated args entry, then the panic leaks it.
    let mut mid = ByteCodeFunction::new(LambdaParams {
        required: vec![intern("a"), intern("b"), intern("c")],
        optional: Vec::new(),
        rest: None,
    });
    mid.lexical = true;
    mid.ops = vec![
        Op::Constant(0), // 'neovm--internal-panic
        Op::Constant(1), // "wide-boom"
        Op::Call(1),
        Op::Return,
    ];
    mid.constants = vec![
        Value::symbol("neovm--internal-panic"),
        Value::string("wide-boom"),
    ]
    .into();
    mid.max_stack = 16;
    ev.obarray
        .set_symbol_function_id(mid_id, Value::make_bytecode(mid));
    let leaf = lower_nullary_leaf(
        &[
            Op::Constant(1), // 5
            Op::VarBind(0),  // has_binds: the exit parity unwind must run
            Op::Constant(2), // 'jit-fx-wide-middle
            Op::Constant(3), // 1
            Op::Constant(4), // 2
            Op::Constant(5), // 3
            Op::Call(3),
            Op::Unbind(1),
            Op::Return,
        ],
        &[
            var,
            Value::make_int(5),
            mid_sym,
            Value::make_int(1),
            Value::make_int(2),
            Value::make_int(3),
        ],
    )
    .expect("wide call body compiles");
    let spec0 = ev.specpdl.len();
    let args0 = ev.backtrace_args_stack_len_for_test();
    assert_eq!(leaf.call(ctx_ptr, &[]), NativeRun::Signal);
    let flow = take_pending_flow().expect("panic flow stashed");
    let Flow::Signal(sig) = flow else {
        panic!("expected Signal, got {flow:?}");
    };
    let msg = sig.data[0].as_str_owned().expect("string payload");
    assert!(msg.contains("wide-boom"), "unexpected message: {msg}");
    assert_eq!(
        ev.backtrace_args_stack_len_for_test(),
        args0,
        "backtrace args stack back at base"
    );
    assert_eq!(ev.specpdl.len(), spec0, "specpdl swept at leaf exit");
    // The deferred unwind ran clean: a second containment round behaves
    // identically (no cascading re-containment from the release path).
    assert_eq!(leaf.call(ctx_ptr, &[]), NativeRun::Signal);
    let _ = take_pending_flow().expect("second round stashes too");
    assert_eq!(ev.backtrace_args_stack_len_for_test(), args0);
    assert_eq!(ev.specpdl.len(), spec0);
}

#[test]
fn ctxless_shim_panic_re_raised_when_gc_locks_poisoned() {
    // The ctx-less wrapped shims probe the lock-poison half of the
    // unrecoverable check through the thread heap: with a poisoned GC
    // lock the panic must be re-raised (abort at the shim in
    // production), stashing nothing. Poison is permanent for this
    // process — fine under nextest's process-per-test.
    crate::tagged::gc::with_tagged_heap(|h| h.poison_gc_locks_for_test());
    let payload: Box<dyn std::any::Any + Send> = Box::new("poisoned-flee");
    let back = contain_jit_shim_panic(core::ptr::null_mut(), payload)
        .expect_err("poisoned GC locks must re-raise on the ctx-less path");
    assert_eq!(back.downcast_ref::<&str>().unwrap(), &"poisoned-flee");
    assert!(
        !shim_panic_pending(),
        "nothing stashed on the re-raise path"
    );
    assert!(take_pending_flow().is_none());
}

#[test]
fn contained_panic_in_load_unwinds_load_bookkeeping() {
    // `load` bookkeeping rides the specpdl: a panic contained mid-load
    // must leave `load-in-progress` nil and `loads_in_progress` empty
    // once the deferred unwind runs, and repeated containment must not
    // accumulate entries into a spurious "Recursive load".
    let mut ev = crate::emacs_core::eval::Context::new();
    let ctx_ptr = &mut ev as *mut crate::emacs_core::eval::Context as *mut u8;
    let dir = tempfile::tempdir().expect("tempdir");
    let fixture = dir.path().join("jit-fx-panic-load.el");
    std::fs::write(&fixture, "(neovm--internal-panic \"load-boom\")\n")
        .expect("write load fixture");
    let path_str = fixture.to_string_lossy().into_owned();
    let var = Value::symbol("jit-fx-load-dynvar");
    let leaf = lower_nullary_leaf(
        &[
            Op::Constant(1), // 5
            Op::VarBind(0),  // has_binds: exit parity unwind sweeps the leak
            Op::Constant(2), // 'load
            Op::Constant(3), // absolute fixture path
            Op::Call(1),
            Op::Unbind(1),
            Op::Return,
        ],
        &[
            var,
            Value::make_int(5),
            Value::symbol("load"),
            Value::string(&path_str),
        ],
    )
    .expect("load call body compiles");
    let spec0 = ev.specpdl.len();
    // GNU signals "Recursive load" once the same file is in flight five
    // times; five leaked entries would previously get there. Every round
    // must instead report the contained panic with clean state.
    for round in 0..5 {
        assert_eq!(leaf.call(ctx_ptr, &[]), NativeRun::Signal, "round {round}");
        let flow = take_pending_flow().expect("panic flow stashed");
        let Flow::Signal(sig) = flow else {
            panic!("round {round}: expected Signal, got {flow:?}");
        };
        let msg = sig.data[0].as_str_owned().expect("string payload");
        assert!(
            msg.contains("load-boom") && !msg.contains("Recursive load"),
            "round {round}: unexpected message: {msg}"
        );
        assert!(
            ev.loads_in_progress.is_empty(),
            "round {round}: loads_in_progress leaked"
        );
        assert_eq!(
            ev.obarray
                .symbol_value("load-in-progress")
                .cloned()
                .unwrap_or(Value::NIL),
            Value::NIL,
            "round {round}: load-in-progress wedged"
        );
        assert_eq!(ev.specpdl.len(), spec0, "round {round}: specpdl swept");
    }
    // And a healthy load of a well-formed file still works afterwards.
    let ok_file = dir.path().join("jit-fx-ok-load.el");
    std::fs::write(&ok_file, "(setq jit-fx-load-ok t)\n").expect("write ok fixture");
    ev.eval_str(&format!("(load {:?} nil t)", ok_file.to_string_lossy()))
        .expect("normal load succeeds after repeated containment");
    assert!(ev.loads_in_progress.is_empty());
}

/// `NEOVM_JIT_REGALLOC` accepts the two allocators under a few spellings and
/// leaves the policy alone for anything else.
#[test]
fn regalloc_knob_maps_spellings_to_allocators() {
    use super::lowering::{RegallocChoice, parse_regalloc_choice};
    assert_eq!(
        parse_regalloc_choice(Some("single_pass")),
        Some(RegallocChoice::Fast)
    );
    assert_eq!(
        parse_regalloc_choice(Some(" fastalloc ")),
        Some(RegallocChoice::Fast)
    );
    assert_eq!(
        parse_regalloc_choice(Some("backtracking")),
        Some(RegallocChoice::Full)
    );
    assert_eq!(
        parse_regalloc_choice(Some("ion")),
        Some(RegallocChoice::Full)
    );
    assert_eq!(parse_regalloc_choice(Some("linear-scan")), None);
    assert_eq!(parse_regalloc_choice(None), None);
}

/// A body that calls its own symbol is recursion, unbounded work per entry
/// like a loop, so it gets the full allocator's smaller frame. The check is
/// by object identity: another function with the same code, a non-calling
/// self-reference, and a caller with no obarray all keep the plain shape rule.
#[test]
fn a_body_that_calls_itself_is_classified_by_identity() {
    let mut ev = Context::new();
    let self_sym = Value::symbol("jit-self-recursive");
    let other_sym = Value::symbol("jit-other-callee");
    let mk = |ops: Vec<Op>, callee: Value| {
        let mut f = ByteCodeFunction::new(LambdaParams {
            required: vec![SymId(1)],
            optional: Vec::new(),
            rest: None,
        });
        f.lexical = true;
        f.ops = ops;
        f.constants = vec![callee].into();
        f.max_stack = 16;
        f
    };
    let calls = || vec![Op::Constant(0), Op::StackRef(1), Op::Call(1), Op::Return];
    let self_val = Value::make_bytecode(mk(calls(), self_sym));
    ev.obarray
        .set_symbol_function_id(self_sym.as_symbol_id().unwrap(), self_val);
    let other_val = Value::make_bytecode(mk(calls(), self_sym));
    ev.obarray
        .set_symbol_function_id(other_sym.as_symbol_id().unwrap(), other_val);
    let no_call_val = Value::make_bytecode(mk(vec![Op::Constant(0), Op::Return], self_sym));

    let self_data = self_val.get_bytecode_data().unwrap();
    assert!(body_calls_itself(self_data, Some(&ev.obarray)));
    assert!(
        !body_calls_itself(self_data, None),
        "no obarray, no verdict"
    );
    // Same code and constants, but the symbol's cell holds a different object.
    assert!(!body_calls_itself(
        other_val.get_bytecode_data().unwrap(),
        Some(&ev.obarray)
    ));
    // A self-reference without a call is not recursion.
    assert!(!body_calls_itself(
        no_call_val.get_bytecode_data().unwrap(),
        Some(&ev.obarray)
    ));
}

/// The allocator policy: forced wins; otherwise loops and `Full` requests get
/// the full allocator, straight-line entry compiles the fast one.
#[test]
fn regalloc_policy_is_forced_then_shape_then_request() {
    use super::lowering::{RegallocChoice, RegallocPolicy, choose_regalloc};
    let (a, f) = (RegallocPolicy::Auto, RegallocPolicy::Full);
    assert_eq!(choose_regalloc(None, a, false, false), RegallocChoice::Fast);
    assert_eq!(choose_regalloc(None, a, true, false), RegallocChoice::Full);
    assert_eq!(choose_regalloc(None, f, false, false), RegallocChoice::Full);
    assert_eq!(
        choose_regalloc(Some(RegallocChoice::Fast), f, true, false),
        RegallocChoice::Fast
    );
    assert_eq!(
        choose_regalloc(Some(RegallocChoice::Full), a, false, false),
        RegallocChoice::Full
    );
    // Call-heavy: fast even with a loop, even for a Full (OSR/re-tier) request.
    assert_eq!(choose_regalloc(None, a, true, true), RegallocChoice::Fast);
    assert_eq!(choose_regalloc(None, f, true, true), RegallocChoice::Fast);
    assert_eq!(
        choose_regalloc(Some(RegallocChoice::Full), a, false, true),
        RegallocChoice::Full
    );
    // The classifier behind it: more (non-intrinsified) calls than arithmetic.
    let length = crate::emacs_core::intern::intern("length");
    assert!(body_is_call_heavy(
        &[Op::Constant(0), Op::CallBuiltinSym(length, 1), Op::Return],
        &[Value::string("abc")]
    ));
    assert!(!body_is_call_heavy(
        &[Op::StackRef(0), Op::Add1, Op::Return],
        &[]
    ));
    // Back-edge detection: any jump to its own index or earlier.
    assert!(!has_back_edge(&[
        Op::Constant(0),
        Op::GotoIfNil(3),
        Op::Nil,
        Op::Return
    ]));
    assert!(has_back_edge(&[
        Op::Constant(0),
        Op::GotoIfNil(3),
        Op::Goto(0),
        Op::Return
    ]));
    assert!(has_back_edge(&[
        Op::Nil,
        Op::GotoIfNotNilElsePop(1),
        Op::Return
    ]));
}

/// Outside a compile scope the active choice is `Full` (unscoped paths compile
/// as before); an entry compile of a straight-line body is `Fast`, a loop body
/// is `Full`, and a `Full` request is honored.
#[test]
fn entry_compiles_pick_the_allocator_by_shape_and_request() {
    use super::lowering::{
        RegallocChoice, RegallocPolicy, active_regalloc_choice, forced_regalloc,
    };
    assert_eq!(active_regalloc_choice(), RegallocChoice::Full);
    if forced_regalloc().is_some() {
        return; // the A/B knob overrides the policy
    }
    let mut f = nullary();
    f.ops = vec![Op::Constant(0), Op::Return];
    f.constants = vec![Value::make_int(42)].into();
    let leaf = compile_bytecode_function_with(&f, None).expect("straight-line body compiles");
    assert_eq!(leaf.regalloc, RegallocChoice::Fast);
    let leaf = compile_bytecode_function_tiered(&f, None, RegallocPolicy::Full)
        .expect("compiles under the full allocator too");
    assert_eq!(leaf.regalloc, RegallocChoice::Full);
    assert_eq!(
        active_regalloc_choice(),
        RegallocChoice::Full,
        "scope restored"
    );
    // (lambda (n) (while (> n 0) (setq n (1- n))) n): a back-edge → Full.
    let mut lp = ByteCodeFunction::new(LambdaParams {
        required: vec![crate::emacs_core::intern::SymId(1)],
        optional: Vec::new(),
        rest: None,
    });
    lp.ops = vec![
        Op::StackRef(0),
        Op::Constant(0),
        Op::Gtr,
        Op::GotoIfNil(8),
        Op::StackRef(0),
        Op::Sub1,
        Op::StackSet(1),
        Op::Goto(0),
        Op::StackRef(0),
        Op::Return,
    ];
    lp.constants = vec![Value::make_int(0)].into();
    lp.max_stack = 16;
    lp.lexical = true;
    let leaf = compile_bytecode_function_with(&lp, None).expect("loop body compiles");
    assert_eq!(leaf.regalloc, RegallocChoice::Full);
}

/// Code the byte-compiler leaves after an unconditional exit is unreachable:
/// no entry depth, so the leaf lowering must not index it (it panicked with
/// "no entry found for key" on ebrowse/eglot/wdired/texinfo bodies once the
/// profitability gate stopped vetoing them). It lowers to a trap and the
/// reachable code runs as before.
#[test]
fn unreachable_block_after_an_exit_lowers_without_panicking() {
    use crate::emacs_core::eval::Context;
    for ops in [
        // (lambda () 7) followed by dead code after the return.
        vec![Op::Constant(0), Op::Return, Op::Constant(0), Op::Return],
        // A goto over dead code to the real return.
        vec![
            Op::Goto(3),
            Op::Constant(0),
            Op::Return,
            Op::Constant(0),
            Op::Return,
        ],
    ] {
        let mut f = nullary();
        f.ops = ops;
        f.constants = vec![Value::make_int(7)].into();
        let leaf = compile_bytecode_function_with(&f, None)
            .expect("dead code must not break the lowering");
        let mut ev = Context::new_minimal_vm_harness();
        let ctx_ptr = &mut ev as *mut Context as *mut u8;
        assert_eq!(
            leaf.call(ctx_ptr, &[]),
            NativeRun::Ok(Value::make_int(7).bits())
        );
    }
}

/// Commit 0 of the MIR-port plan: a PURE body with a `Float`-feedback
/// arithmetic site must NOT take the MIR tier. The MIR tier guards fixnum
/// and would deopt (rerun-from-start) on EVERY entry; the baseline has the
/// f64 lowering. The funnel records it as `TierRejected` under
/// `gate:float-site`.
#[test]
fn a_pure_body_with_a_float_feedback_site_is_rejected_from_the_mir_tier() {
    // (lambda (a b) (+ a b)) — pure, required-only: MIR-eligible.
    fn adder() -> ByteCodeFunction {
        let mut f = nullary();
        f.lexical = true;
        f.params.required = vec![
            crate::emacs_core::intern::SymId(1),
            crate::emacs_core::intern::SymId(2),
        ];
        f.ops = vec![Op::StackRef(1), Op::StackRef(1), Op::Add, Op::Return];
        f
    }
    use crate::emacs_core::jit::NumericFeedback as NF;

    // 1. Control — no feedback recorded: the pure body takes the MIR tier.
    let control = adder();
    let before = crate::emacs_core::jit::stats::compile_stats_snapshot();
    let leaf = compile_bytecode_function(&control).expect("compiles");
    let mid = crate::emacs_core::jit::stats::compile_stats_snapshot();
    assert_eq!(
        mid.mir_taken,
        before.mir_taken + 1,
        "control: pure fixnum body takes MIR"
    );
    assert_eq!(
        leaf.call_for_test(&[Value::make_int(2), Value::make_int(3)]),
        Some(Value::make_int(5).bits())
    );

    // 2. A SEPARATE function with Float recorded at the `+` site (pc 2) on its
    //    own feedback vector BEFORE its first compile — the compile publishes
    //    that snapshot and then marks the body's feedback CONSUMED, so
    //    recording after a compile (or reusing `control`) reaches nothing.
    //    Rejected to the baseline, which computes the float sum natively
    //    instead of deopting on every entry.
    let floaty = adder();
    floaty
        .jit_runtime()
        .record_numeric(2, floaty.ops.len(), NF::Float);
    assert_eq!(
        floaty.jit_runtime().numeric_feedback(2),
        NF::Float,
        "precondition: the Float record must be visible before compiling"
    );
    let leaf = compile_bytecode_function(&floaty).expect("compiles");
    let after = crate::emacs_core::jit::stats::compile_stats_snapshot();
    assert_eq!(
        after.mir_taken, mid.mir_taken,
        "a Float-site body must NOT take the MIR tier"
    );
    assert_eq!(
        after.mir_tier_rejected,
        mid.mir_tier_rejected + 1,
        "…and is counted as TierRejected"
    );
    // Drive the baseline leaf with a REAL Context: `call_for_test` passes a
    // null vmctx, which a runtime-needing baseline body refuses (Deopt) before
    // the float path ever runs.
    let mut eval = Context::new();
    let ctx_ptr = &mut eval as *mut Context as *mut u8;
    let got = match leaf.call(ctx_ptr, &[Value::make_float(1.5), Value::make_float(2.5)]) {
        NativeRun::Ok(bits) => Value::from_bits(bits),
        other => panic!("the baseline leaf must run the float path natively, got {other:?}"),
    };
    assert!(
        matches!(got.kind(), crate::emacs_core::value::ValueKind::Float),
        "expected a float, got {got:?}"
    );
    assert_eq!(got.xfloat(), 4.0);

    // The site is sticky-Float from one float pair, but GNU's `(+ 2 3)` is the
    // FIXNUM 5 — and it must run natively (no deopt) or every fixnum call
    // after a single float pair pays a rerun. Mixed operands promote.
    let fix = match leaf.call(ctx_ptr, &[Value::make_int(2), Value::make_int(3)]) {
        NativeRun::Ok(bits) => Value::from_bits(bits),
        other => panic!("fixnum operands at a Float site must run natively, got {other:?}"),
    };
    assert_eq!(
        fix.bits(),
        Value::make_int(5).bits(),
        "(+ 2 3) is the fixnum 5, not 5.0"
    );
    let mixed = match leaf.call(ctx_ptr, &[Value::make_int(2), Value::make_float(0.5)]) {
        NativeRun::Ok(bits) => Value::from_bits(bits),
        other => panic!("mixed operands at a Float site must promote natively, got {other:?}"),
    };
    assert!(matches!(
        mixed.kind(),
        crate::emacs_core::value::ValueKind::Float
    ));
    assert_eq!(mixed.xfloat(), 2.5);
    // A non-number still deopts (the interpreter signals wrong-type-argument).
    assert!(matches!(
        leaf.call(ctx_ptr, &[Value::symbol("x"), Value::make_int(1)]),
        NativeRun::Deopt | NativeRun::DeoptAt(_)
    ));
}

/// GNU's `arithcompare` re-decides a fixnum/float DOUBLE tie on exact
/// integers, because promoting a fixnum above 2^53 rounds. The polymorphic
/// compare arm must do the same or `(> 9007199254740993 9007199254740992.0)`
/// comes out nil and `(= most-positive-fixnum (float most-positive-fixnum))`
/// comes out t — natively, with no deopt to save it.
#[test]
fn a_float_site_compare_breaks_a_double_tie_on_exact_integers_like_gnu() {
    fn cmp_fn(op: Op) -> ByteCodeFunction {
        let mut f = nullary();
        f.lexical = true;
        f.params.required = vec![
            crate::emacs_core::intern::SymId(1),
            crate::emacs_core::intern::SymId(2),
        ];
        f.ops = vec![Op::StackRef(1), Op::StackRef(1), op, Op::Return];
        f
    }
    use crate::emacs_core::jit::NumericFeedback as NF;
    let mut eval = Context::new();
    let ctx_ptr = &mut eval as *mut Context as *mut u8;
    let t = Value::T.bits();
    let nil = Value::NIL.bits();
    let big = 9_007_199_254_740_993_i64; // 2^53 + 1
    let bigf = 9_007_199_254_740_992.0_f64; // 2^53
    let mpf = Value::MOST_POSITIVE_FIXNUM;

    let gtr = cmp_fn(Op::Gtr);
    gtr.jit_runtime()
        .record_numeric(2, gtr.ops.len(), NF::Float);
    let leaf = compile_bytecode_function(&gtr).expect("compiles");
    let run = |leaf: &CompiledLeaf, a: Value, b: Value| match leaf.call(ctx_ptr, &[a, b]) {
        NativeRun::Ok(bits) => bits,
        other => panic!("must run natively at a Float site, got {other:?}"),
    };
    assert_eq!(
        run(&leaf, Value::make_int(big), Value::make_float(bigf)),
        t,
        "(> 2^53+1 2^53.0) is t"
    );
    assert_eq!(
        run(&leaf, Value::make_float(bigf), Value::make_int(big)),
        nil,
        "(> 2^53.0 2^53+1) is nil"
    );
    assert_eq!(run(&leaf, Value::make_int(3), Value::make_float(2.5)), t);
    assert_eq!(
        run(&leaf, Value::make_int(3), Value::make_float(f64::NAN)),
        nil,
        "NaN compares false"
    );
    assert_eq!(
        run(&leaf, Value::make_int(5), Value::make_int(4)),
        t,
        "fixnum pair stays native"
    );

    let eq = cmp_fn(Op::Eqlsign);
    eq.jit_runtime().record_numeric(2, eq.ops.len(), NF::Float);
    let leaf = compile_bytecode_function(&eq).expect("compiles");
    assert_eq!(
        run(&leaf, Value::make_int(mpf), Value::make_float(mpf as f64)),
        nil,
        "(= most-positive-fixnum (float most-positive-fixnum)) is nil"
    );
    assert_eq!(
        run(&leaf, Value::make_int(3), Value::make_float(3.0)),
        t,
        "(= 3 3.0) is t"
    );
    assert_eq!(
        run(&leaf, Value::make_float(-0.0), Value::make_int(0)),
        t,
        "(= -0.0 0) is t"
    );
    assert_eq!(
        run(
            &leaf,
            Value::make_float(f64::NAN),
            Value::make_float(f64::NAN)
        ),
        nil
    );
}

// ---------------------------------------------------------------------------
// Generic fallback at arithmetic sites whose feedback says the fixnum path
// misses (`compile::arith_site_takes_generic`).
// ---------------------------------------------------------------------------

/// A function `(lambda (a [b]) (OP a [b]))` with `feedback` recorded at the
/// op's pc before its first compile.
fn generic_arith_fn(
    op: Op,
    nargs: usize,
    feedback: crate::emacs_core::jit::NumericFeedback,
) -> ByteCodeFunction {
    let mut f = nullary();
    f.lexical = true;
    f.params.required = (1..=nargs as u32)
        .map(crate::emacs_core::intern::SymId)
        .collect();
    f.ops = if nargs == 2 {
        vec![Op::StackRef(1), Op::StackRef(1), op, Op::Return]
    } else {
        vec![Op::StackRef(0), op, Op::Return]
    };
    f.max_stack = 8;
    if feedback != crate::emacs_core::jit::NumericFeedback::FixnumOnly {
        let pc = f.ops.len() - 2;
        f.jit_runtime().record_numeric(pc, f.ops.len(), feedback);
    }
    f
}

fn outcome_of_flow(flow: crate::emacs_core::error::Flow) -> String {
    match flow {
        crate::emacs_core::error::Flow::Signal(sig) => format!(
            "signal {} {:?}",
            sig.symbol_name(),
            sig.data
                .iter()
                .map(crate::emacs_core::print::print_value)
                .collect::<Vec<_>>()
        ),
        other => format!("{other:?}"),
    }
}

/// Every opcode with a generic fallback answers exactly as the interpreter's
/// opcode arm, natively — no deopt — on fixnums, the fixnum boundaries,
/// bignums, floats, zero divisors and non-numbers; and the interpreter's
/// answer on fixnums is the fixnum fast path's.
#[test]
fn generic_arith_sites_match_the_interpreter_without_deopting() {
    use crate::emacs_core::bytecode::Vm;
    use crate::emacs_core::eval::Context;
    use crate::emacs_core::jit::NumericFeedback as NF;
    use crate::emacs_core::print::print_value;
    let mut eval = Context::new();
    let ctx_ptr = &mut eval as *mut Context as *mut u8;
    let pool_src = [
        "0",
        "1",
        "-1",
        "7",
        "-3",
        "most-positive-fixnum",
        "most-negative-fixnum",
        "(1+ most-positive-fixnum)",
        "(1- most-negative-fixnum)",
        "(expt 2 100)",
        "(- (expt 3 80))",
        "1.5",
        "-0.0",
        "0.0e+NaN",
        "'x",
    ];
    let binary = [
        Op::Add,
        Op::Sub,
        Op::Mul,
        Op::Div,
        Op::Rem,
        Op::Max,
        Op::Min,
        Op::Eqlsign,
        Op::Lss,
        Op::Gtr,
        Op::Leq,
        Op::Geq,
    ];
    let unary = [Op::Add1, Op::Sub1, Op::Negate];
    let interp = |eval: &mut Context, op: &Op, nargs: usize, args: Vec<Value>| -> String {
        let f = generic_arith_fn(op.clone(), nargs, NF::FixnumOnly);
        let mut vm = Vm::from_context(eval);
        match vm.execute(&f, args) {
            Ok(v) => print_value(&v),
            Err(flow) => outcome_of_flow(flow),
        }
    };
    let mut checked = 0;
    for (ops, nargs) in [(&binary[..], 2usize), (&unary[..], 1usize)] {
        for op in ops {
            // Other for all; Float too where the op has no float lowering.
            let mut feedbacks = vec![NF::Other];
            if matches!(
                op,
                Op::Rem | Op::Max | Op::Min | Op::Add1 | Op::Sub1 | Op::Negate
            ) {
                feedbacks.push(NF::Float);
            }
            for fb in feedbacks {
                let f = generic_arith_fn(op.clone(), nargs, fb);
                let leaf = compile_bytecode_function(&f).expect("compiles");
                let firsts: &[&str] = &pool_src;
                let seconds: &[&str] = if nargs == 2 { &pool_src } else { &["0"] };
                for a_src in firsts {
                    for b_src in seconds {
                        let pair = eval
                            .eval_str(&format!("(cons {a_src} {b_src})"))
                            .expect("operands");
                        let args: Vec<Value> = if nargs == 2 {
                            vec![pair.cons_car(), pair.cons_cdr()]
                        } else {
                            vec![pair.cons_car()]
                        };
                        let want = interp(&mut eval, op, nargs, args.clone());
                        let got = match leaf.call(ctx_ptr, &args) {
                            NativeRun::Ok(bits) => print_value(&Value::from_bits(bits)),
                            NativeRun::Signal => {
                                outcome_of_flow(take_pending_flow().expect("flow stashed"))
                            }
                            other => panic!(
                                "{op:?} [{fb:?}] on ({a_src}, {b_src}) must not leave native code: {other:?}"
                            ),
                        };
                        assert_eq!(got, want, "{op:?} [{fb:?}] on ({a_src}, {b_src})");
                        checked += 1;
                    }
                }
            }
        }
    }
    assert!(checked > 2000, "checked {checked}");
}

/// The result of a generic-fallback site is not a fixnum — an overflowing `*`
/// returns a bignum — so the known-fixnum analysis must not carry it across a
/// block edge as one. Here it reaches a `FixnumOnly` `1+` through a leader;
/// had the analysis kept calling it a fixnum, `1+`'s guard would be elided
/// and the bignum's POINTER shifted as a fixnum, silently.
///
///     (lambda (a b c) (let ((x (* a b))) (if c) (1+ x)))
#[test]
fn a_generic_site_result_is_not_a_known_fixnum_across_a_block_edge() {
    use crate::emacs_core::eval::Context;
    use crate::emacs_core::jit::NumericFeedback as NF;
    let mut f = nullary();
    f.lexical = true;
    f.params.required = (1..=3).map(crate::emacs_core::intern::SymId).collect();
    f.ops = vec![
        Op::StackRef(2),  // 0: a        [a b c a]
        Op::StackRef(2),  // 1: b        [a b c a b]
        Op::Mul,          // 2: x        [a b c x]    (Other)
        Op::StackRef(1),  // 3: c        [a b c x c]
        Op::GotoIfNil(5), // 4: leader 5 from both edges
        Op::Add1,         // 5:          [a b c x+1]  (FixnumOnly)
        Op::Return,
    ];
    f.max_stack = 8;
    f.jit_runtime().record_numeric(2, f.ops.len(), NF::Other);
    let leaf = compile_bytecode_function(&f).expect("compiles");
    let mut eval = Context::new();
    let ctx_ptr = &mut eval as *mut Context as *mut u8;
    let two40 = Value::make_int(1 << 40);
    for c in [Value::T, Value::NIL] {
        match leaf.call(ctx_ptr, &[two40, two40, c]) {
            NativeRun::Ok(bits) => panic!(
                "(1+ bignum) at a FixnumOnly site cannot finish natively; got bits {bits:#x} ({})",
                crate::emacs_core::print::print_value(&Value::from_bits(bits))
            ),
            NativeRun::Deopt | NativeRun::DeoptAt(_) => {}
            other => panic!("unexpected {other:?}"),
        }
    }
    // Fixnum products still run natively through both paths.
    for c in [Value::T, Value::NIL] {
        assert_eq!(
            leaf.call(ctx_ptr, &[Value::make_int(6), Value::make_int(7), c]),
            NativeRun::Ok(Value::make_int(43).bits())
        );
    }
}

/// A body with a generic-fallback site stays out of the MIR tier, which
/// guards fixnum and would rerun-from-start on every miss.
#[test]
fn a_body_with_a_generic_arith_site_is_rejected_from_the_mir_tier() {
    use crate::emacs_core::jit::NumericFeedback as NF;
    let control = generic_arith_fn(Op::Add, 2, NF::FixnumOnly);
    let before = crate::emacs_core::jit::stats::compile_stats_snapshot();
    compile_bytecode_function(&control).expect("compiles");
    let mid = crate::emacs_core::jit::stats::compile_stats_snapshot();
    assert_eq!(mid.mir_taken, before.mir_taken + 1, "control takes MIR");
    for (op, nargs, fb) in [
        (Op::Add, 2, NF::Other),
        (Op::Gtr, 2, NF::Other),
        (Op::Add1, 1, NF::Float),
        (Op::Max, 2, NF::Other),
    ] {
        let s0 = crate::emacs_core::jit::stats::compile_stats_snapshot();
        compile_bytecode_function(&generic_arith_fn(op.clone(), nargs, fb)).expect("compiles");
        let s1 = crate::emacs_core::jit::stats::compile_stats_snapshot();
        assert_eq!(
            s1.mir_taken, s0.mir_taken,
            "{op:?} [{fb:?}] must not take MIR"
        );
        assert_eq!(
            s1.mir_tier_rejected,
            s0.mir_tier_rejected + 1,
            "{op:?} [{fb:?}] counted as TierRejected"
        );
    }
}

/// A signal from the generic fallback reaches a condition-case in the same
/// compiled body, with the handler's stack intact.
///
///     (lambda (a b) (condition-case err (+ a b) (error (list 'caught err))))
#[test]
fn a_generic_arith_signal_is_caught_by_a_leaf_local_handler() {
    use crate::emacs_core::eval::Context;
    use crate::emacs_core::jit::NumericFeedback as NF;
    let mut f = nullary();
    f.lexical = true;
    f.params.required = (1..=2).map(crate::emacs_core::intern::SymId).collect();
    f.ops = vec![
        Op::PushConditionCase(6), // 0
        Op::StackRef(1),          // 1 a
        Op::StackRef(1),          // 2 b
        Op::Add,                  // 3 (Other)
        Op::PopHandler,           // 4
        Op::Return,               // 5
        Op::Constant(0),          // 6: handler [a b err] -> 'caught
        Op::StackRef(1),          // 7 err
        Op::List(2),              // 8
        Op::Return,               // 9
    ];
    f.constants = vec![Value::symbol("caught")].into();
    f.max_stack = 8;
    f.jit_runtime().record_numeric(3, f.ops.len(), NF::Other);
    let leaf = compile_bytecode_function(&f).expect("compiles");
    let mut eval = Context::new();
    let ctx_ptr = &mut eval as *mut Context as *mut u8;
    let big = eval.eval_str("(expt 2 70)").expect("big");
    match leaf.call(ctx_ptr, &[big, Value::symbol("oops")]) {
        NativeRun::Ok(bits) => assert_eq!(
            crate::emacs_core::print::print_value(&Value::from_bits(bits)),
            "(caught (wrong-type-argument number-or-marker-p oops))"
        ),
        other => panic!("the handler must catch natively, got {other:?}"),
    }
    match leaf.call(ctx_ptr, &[big, Value::make_int(1)]) {
        NativeRun::Ok(bits) => assert_eq!(
            crate::emacs_core::print::print_value(&Value::from_bits(bits)),
            "1180591620717411303425"
        ),
        other => panic!("bignum + fixnum must run natively, got {other:?}"),
    }
}

/// Live values below a generic site survive its fallback call under GC
/// stress, and two such sites in a row (a stored record, then a meet) keep
/// the residual rooted.
///
///     (lambda (a b) (let ((h (cons 1 2))) (+ a b) (* a b) h))
#[test]
fn generic_arith_fallbacks_keep_the_residual_rooted() {
    use crate::emacs_core::eval::Context;
    use crate::emacs_core::jit::NumericFeedback as NF;
    let mut f = nullary();
    f.lexical = true;
    f.params.required = (1..=2).map(crate::emacs_core::intern::SymId).collect();
    f.ops = vec![
        Op::Constant(0), // [a b 1]
        Op::Constant(1), // [a b 1 2]
        Op::Cons,        // [a b h]
        Op::StackRef(2), // [a b h a]
        Op::StackRef(2), // [a b h a b]
        Op::Add,         // 5 (Other)   residual [a b h]
        Op::Pop,
        Op::StackRef(2),
        Op::StackRef(2),
        Op::Mul, // 9 (Other)
        Op::Pop,
        Op::Return, // h
    ];
    f.constants = vec![Value::make_int(1), Value::make_int(2)].into();
    f.max_stack = 8;
    f.jit_runtime().record_numeric(5, f.ops.len(), NF::Other);
    f.jit_runtime().record_numeric(9, f.ops.len(), NF::Other);
    let leaf = compile_bytecode_function(&f).expect("compiles");
    let mut eval = Context::new();
    eval.gc_stress = true;
    let ctx_ptr = &mut eval as *mut Context as *mut u8;
    for _ in 0..3 {
        let pair = eval
            .eval_str("(cons (expt 2 90) (expt 3 50))")
            .expect("operands");
        match leaf.call(ctx_ptr, &[pair.cons_car(), pair.cons_cdr()]) {
            NativeRun::Ok(bits) => {
                let h = Value::from_bits(bits);
                assert!(h.is_cons(), "h survived (got {h:?})");
                assert_eq!(h.cons_car(), Value::make_int(1));
                assert_eq!(h.cons_cdr(), Value::make_int(2));
            }
            other => panic!("must run natively, got {other:?}"),
        }
    }
}

/// The fallback's call can run Lisp: a signal from the builtin goes through
/// `signal-hook-function` before any handler sees it. Here the hook runs an
/// EXACT collection and reallocates, so a residual the fallback failed to
/// root would be swept and its slot reused before the leaf's own handler
/// returns it.
///
///     (lambda (a b) (let ((h (cons 1 2))) (condition-case nil (+ a b) (error h))))
#[test]
fn a_generic_arith_signal_hook_that_collects_keeps_the_residual_alive() {
    use crate::emacs_core::eval::Context;
    use crate::emacs_core::jit::NumericFeedback as NF;
    let mut f = nullary();
    f.lexical = true;
    f.params.required = (1..=2).map(crate::emacs_core::intern::SymId).collect();
    f.ops = vec![
        Op::Constant(0),          // 0  [a b 1]
        Op::Constant(1),          // 1  [a b 1 2]
        Op::Cons,                 // 2  [a b h]
        Op::PushConditionCase(9), // 3
        Op::StackRef(2),          // 4  [a b h a]
        Op::StackRef(2),          // 5  [a b h a b]
        Op::Add,                  // 6  (Other) residual [a b h]
        Op::PopHandler,           // 7
        Op::Return,               // 8  the sum
        Op::Pop,                  // 9  handler [a b h err] -> [a b h]
        Op::Return,               // 10 h
    ];
    f.constants = vec![Value::make_int(1), Value::make_int(2)].into();
    f.max_stack = 8;
    f.jit_runtime().record_numeric(6, f.ops.len(), NF::Other);
    let leaf = compile_bytecode_function(&f).expect("compiles");
    let mut eval = Context::new();
    eval.eval_str(
        "(setq signal-hook-function
               (lambda (_sym _data) (garbage-collect) (make-list 4096 (cons 0 0)) nil))",
    )
    .expect("hook");
    let ctx_ptr = &mut eval as *mut Context as *mut u8;
    for _ in 0..3 {
        let big = eval.eval_str("(expt 2 70)").expect("big");
        match leaf.call(ctx_ptr, &[big, Value::symbol("oops")]) {
            NativeRun::Ok(bits) => {
                let h = Value::from_bits(bits);
                assert!(h.is_cons(), "h survived the hook's collection (got {h:?})");
                assert_eq!(h.cons_car(), Value::make_int(1), "car intact");
                assert_eq!(h.cons_cdr(), Value::make_int(2), "cdr intact");
            }
            other => panic!("the handler must catch natively, got {other:?}"),
        }
    }
}

/// What the interpreter records at an arithmetic site's slow arm: a bignum
/// operand is `Other`; an all-fixnum miss (overflow, zero divisor, `1+` at
/// the boundary) records nothing, so one overflow during warm-up does not
/// turn a fixnum-hot site into a generic-fallback site for good — and does
/// not downgrade a `Float` site either.
#[test]
fn only_a_non_fixnum_operand_records_other_feedback() {
    use crate::emacs_core::bytecode::Vm;
    use crate::emacs_core::eval::Context;
    use crate::emacs_core::jit::NumericFeedback as NF;
    let mut eval = Context::new();
    let run = |eval: &mut Context, f: &ByteCodeFunction, args: Vec<Value>| {
        let mut vm = Vm::from_context(eval);
        let _ = vm.execute(f, args);
    };
    let mpf = Value::make_int(Value::MOST_POSITIVE_FIXNUM);
    // Overflow on fixnums: nothing recorded.
    let add = generic_arith_fn(Op::Add, 2, NF::FixnumOnly);
    run(&mut eval, &add, vec![mpf, Value::make_int(1)]);
    assert_eq!(
        add.jit_runtime().numeric_feedback(2),
        NF::FixnumOnly,
        "overflow"
    );
    let div = generic_arith_fn(Op::Div, 2, NF::FixnumOnly);
    run(
        &mut eval,
        &div,
        vec![Value::make_int(1), Value::make_int(0)],
    );
    assert_eq!(
        div.jit_runtime().numeric_feedback(2),
        NF::FixnumOnly,
        "zero divisor"
    );
    let add1 = generic_arith_fn(Op::Add1, 1, NF::FixnumOnly);
    run(&mut eval, &add1, vec![mpf]);
    assert_eq!(
        add1.jit_runtime().numeric_feedback(1),
        NF::FixnumOnly,
        "1+ at the boundary"
    );
    // A bignum operand: Other.
    let big = eval.eval_str("(expt 2 80)").expect("big");
    run(&mut eval, &add, vec![big, Value::make_int(1)]);
    assert_eq!(add.jit_runtime().numeric_feedback(2), NF::Other, "bignum");
    // A float site stays Float through a later fixnum overflow.
    let mul = generic_arith_fn(Op::Mul, 2, NF::FixnumOnly);
    run(
        &mut eval,
        &mul,
        vec![Value::make_float(1.5), Value::make_int(2)],
    );
    assert_eq!(mul.jit_runtime().numeric_feedback(2), NF::Float);
    run(&mut eval, &mul, vec![mpf, Value::make_int(4)]);
    assert_eq!(
        mul.jit_runtime().numeric_feedback(2),
        NF::Float,
        "overflow keeps Float"
    );
}

/// `type-of` and `cl-type-of` called through `Op::Call` are GC-free predicate
/// intrinsics (`SpecCalleeKind::PredTypeOf`/`PredClTypeOf`): armed, the site
/// answers from `neovm_jit_pred_spec` with no backtrace frame and no generic
/// dispatch. The answer must be the builtin's for every kind of value, the
/// fast path must actually be taken, and a redefinition of the function must
/// reach the generic call.
/// A JIT `type-of` / `cl-type-of` site answers a record inline: its type
/// slot, or its class record's name. The result is this site's own, not the
/// previous call's; a redefinition or an armed `debug-on-next-call` takes
/// the shim again.
#[test]
fn a_record_type_of_site_answers_inline() {
    use crate::emacs_core::eval::Context;
    use crate::emacs_core::intern::{SymId, intern};
    crate::emacs_core::jit::compile::force_profit_gate_for_test(false);
    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context as *mut u8;
    // (lambda (x) (cons (symbol-name 'zzz) (type-of x))): the subr call
    // before it leaves "zzz" in the call result slot.
    let site = |name: &str| {
        let mut f = ByteCodeFunction::new(LambdaParams {
            required: vec![SymId(1)],
            optional: Vec::new(),
            rest: None,
        });
        f.lexical = true;
        f.ops = vec![
            Op::Constant(0),
            Op::Constant(1),
            Op::Call(1),
            Op::Constant(2),
            Op::StackRef(2),
            Op::Call(1),
            Op::Cons,
            Op::Return,
        ];
        f.constants = vec![
            Value::symbol("symbol-name"),
            Value::symbol("zzz"),
            Value::symbol(name),
        ]
        .into();
        f.max_stack = 8;
        f
    };
    let pool = ev
        .eval_str(
            "(list (record 'foo 1) (record 'bar) (record (record 'eieio--class 'my-class) 2)
                   (record (record 'lone) 3) (record \"str\" 1) (record (current-buffer) 1)
                   (current-buffer) 5 'sym nil (cons 1 2) (vector 1))",
        )
        .expect("pool");
    crate::emacs_core::eval::push_scratch_gc_root(pool);
    let items = crate::emacs_core::value::list_to_vec(&pool).expect("list");
    for name in ["type-of", "cl-type-of"] {
        let f = site(name);
        let leaf = compile_bytecode_function_with(&f, Some(&ev.obarray)).expect("compiles");
        // Resolve the memoized debug-on-next-call cell the inline test reads.
        let _ = ev.debug_on_next_call_is_armed();
        let run = |item: Value| match leaf.call(ctx, &[item]) {
            NativeRun::Ok(bits) => crate::emacs_core::print::print_value(&Value::from_bits(bits)),
            other => format!("{other:?}"),
        };
        for &item in &items {
            let want = if name == "type-of" {
                crate::emacs_core::builtins::types::builtin_type_of(&[item])
            } else {
                crate::emacs_core::builtins::types::builtin_cl_type_of(&[item])
            }
            .expect("builtin answers");
            #[cfg(debug_assertions)]
            let shim0 = SUBR_SPEC_COUNT.load(Ordering::Relaxed);
            assert_eq!(
                run(item),
                format!(
                    "(\"zzz\" . {})",
                    crate::emacs_core::print::print_value(&want)
                ),
                "({name} {})",
                crate::emacs_core::print::print_value(&item)
            );
            // Two subr sites: symbol-name's always, type-of's for a non-record.
            #[cfg(debug_assertions)]
            assert_eq!(
                SUBR_SPEC_COUNT.load(Ordering::Relaxed) - shim0,
                if item.is_record() { 1 } else { 2 },
                "({name} {}) inline only for a record",
                crate::emacs_core::print::print_value(&item)
            );
        }
        let record = items[0];
        // debug-on-next-call armed: the site takes the shim. (A site of its
        // own: the call before it would consume the flag.)
        let lone = {
            let mut f = site(name);
            f.ops = vec![Op::Constant(2), Op::StackRef(1), Op::Call(1), Op::Return];
            compile_bytecode_function_with(&f, Some(&ev.obarray)).expect("compiles")
        };
        // A pending quit: the site takes the shim, which polls first and
        // signals.
        ev.set_quit_flag_value(Value::T);
        let quit = lone.call(ctx, &[record]);
        ev.set_quit_flag_value(Value::NIL);
        assert!(
            matches!(quit, NativeRun::Signal),
            "{name}: a pending quit signals at the call: {quit:?}"
        );
        let _ = take_pending_flow();
        // Every other attention source takes the shim too, which polls, finds
        // nothing to signal and answers: a bound `throw-on-input` (a batch
        // Context has no channel to poll), the profiler tick, a pending OS
        // signal, and a raised quit request of another Context.
        #[cfg(debug_assertions)]
        {
            use crate::emacs_core::eval::{ASYNC_ATTENTION, AsyncSource, QuitRequest};
            let other = QuitRequest::new();
            for source in [
                "throw-on-input",
                "profiler-tick",
                "os-signal",
                "quit-request",
            ] {
                match source {
                    "throw-on-input" => {
                        ev.eval_str("(setq throw-on-input 'neo-tag)").expect("bind");
                    }
                    "profiler-tick" => ASYNC_ATTENTION.raise(AsyncSource::ProfilerTick),
                    "os-signal" => ASYNC_ATTENTION.raise(AsyncSource::OsSignal),
                    _ => other.request(),
                }
                let shim0 = SUBR_SPEC_COUNT.load(Ordering::Relaxed);
                let answer = lone.call(ctx, &[record]);
                match source {
                    "throw-on-input" => {
                        ev.eval_str("(setq throw-on-input nil)").expect("unbind");
                    }
                    "profiler-tick" => {
                        ASYNC_ATTENTION.take(AsyncSource::ProfilerTick);
                    }
                    "os-signal" => {
                        ASYNC_ATTENTION.take(AsyncSource::OsSignal);
                    }
                    _ => other.clear(),
                }
                assert_eq!(
                    answer,
                    NativeRun::Ok(Value::symbol("foo").bits()),
                    "{name} under {source}"
                );
                assert_eq!(
                    SUBR_SPEC_COUNT.load(Ordering::Relaxed) - shim0,
                    1,
                    "{name}: {source} takes the shim"
                );
            }
        }
        let cell = ev
            .obarray
            .debug_on_next_call_bool_fwd(intern("debug-on-next-call"))
            .expect("debug-on-next-call cell");
        cell.set(true);
        #[cfg(debug_assertions)]
        let shim0 = SUBR_SPEC_COUNT.load(Ordering::Relaxed);
        let _ = lone.call(ctx, &[record]);
        cell.set(false);
        #[cfg(debug_assertions)]
        assert_eq!(
            SUBR_SPEC_COUNT.load(Ordering::Relaxed) - shim0,
            1,
            "{name}: an armed debug-on-next-call takes the shim"
        );
        // A redefinition: the inline answer no longer applies.
        let original = ev.obarray.symbol_function_id(intern(name)).expect("bound");
        ev.eval_str(&format!("(fset '{name} (lambda (_) 'redefined))"))
            .expect("fset");
        assert_eq!(run(record), "(\"zzz\" . redefined)", "{name} redefined");
        ev.obarray.set_symbol_function_id(intern(name), original);
        assert_eq!(run(record), "(\"zzz\" . foo)", "{name} restored");
    }
}

/// `(fboundp SYM)` through `Op::Call` is the GC-free predicate intrinsic
/// (`SpecCalleeKind::PredFboundp`): armed, the site answers from the symbol's
/// function cell with no backtrace frame; a non-symbol bounces to the generic
/// call and signals exactly as the builtin does; a redefinition of `fboundp`
/// reaches the generic call.
#[test]
fn fboundp_call_sites_answer_as_the_builtin_on_the_fast_path() {
    use crate::emacs_core::eval::Context;
    use crate::emacs_core::intern::{SymId, intern};
    crate::emacs_core::jit::compile::force_profit_gate_for_test(false);
    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context as *mut u8;
    // A bare Context: `defun`/`defmacro`/`defalias` are not loaded, so the
    // cells are set directly.
    ev.eval_str(
        "(progn (fset 'jit-fboundp-nil nil) (fmakunbound 'jit-fboundp-unbound)
                (fset 'jit-fboundp-fn (lambda () 1))
                (fset 'jit-fboundp-mac (cons 'macro (lambda () 1)))
                (fset 'jit-fboundp-alias 'car))",
    )
    .expect("setup");
    let pool_src = "(list 'car 'jit-fboundp-fn 'jit-fboundp-mac 'jit-fboundp-alias \
                          'jit-fboundp-nil 'jit-fboundp-unbound 'jit-fboundp-never \
                          nil t :kw 5 \"s\" (cons 1 2) 1.5)";
    let binding = ev
        .obarray
        .symbol_function_id(intern("fboundp"))
        .expect("bound");
    assert_eq!(
        super::subr_spec_kind(binding, intern("fboundp"), 1),
        Some(super::SpecCalleeKind::PredFboundp)
    );
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: vec![SymId(1)],
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = vec![Op::Constant(0), Op::StackRef(1), Op::Call(1), Op::Return];
    f.constants = vec![Value::symbol("fboundp")].into();
    f.max_stack = 8;
    let leaf = compile_bytecode_function_with(&f, Some(&ev.obarray)).expect("compiles");
    let pool = ev.eval_str(pool_src).expect("pool");
    crate::emacs_core::eval::push_scratch_gc_root(pool);
    let items = crate::emacs_core::value::list_to_vec(&pool).expect("list");
    let outcome = |r: Result<Value, crate::emacs_core::error::Flow>| -> String {
        match r {
            Ok(v) => crate::emacs_core::print::print_value(&v),
            Err(crate::emacs_core::error::Flow::Signal(sig)) => format!(
                "signal {} {:?}",
                sig.symbol_name(),
                sig.data
                    .iter()
                    .map(crate::emacs_core::print::print_value)
                    .collect::<Vec<_>>()
            ),
            Err(other) => format!("{other:?}"),
        }
    };
    #[cfg(debug_assertions)]
    let fast0 = SUBR_SPEC_FAST_COUNT.load(Ordering::Relaxed);
    let mut symbols = 0usize;
    for item in &items {
        let want = outcome(crate::emacs_core::builtins::symbols::builtin_fboundp_1(
            &mut ev, *item,
        ));
        let got = match leaf.call(ctx, &[*item]) {
            NativeRun::Ok(bits) => crate::emacs_core::print::print_value(&Value::from_bits(bits)),
            NativeRun::Signal => outcome(Err(take_pending_flow().expect("stashed flow"))),
            other => panic!(
                "(fboundp {}) : {other:?}",
                crate::emacs_core::print::print_value(item)
            ),
        };
        assert_eq!(
            got,
            want,
            "(fboundp {})",
            crate::emacs_core::print::print_value(item)
        );
        if item.as_symbol_id().is_some() || item.is_nil() || item.is_t() {
            symbols += 1;
        }
    }
    // Every symbol answered on the intrinsic; the rest bounced.
    #[cfg(debug_assertions)]
    assert!(
        SUBR_SPEC_FAST_COUNT.load(Ordering::Relaxed) - fast0 >= symbols as u64,
        "every symbol argument takes the intrinsic"
    );
    let _ = symbols;
    // At the eval-depth limit the site takes the generic call, which counts
    // the subr against `max-lisp-eval-depth` as GNU's `Bcall` does and
    // signals; one below the limit the intrinsic answers.
    let (depth, max_depth) = (ev.depth, ev.max_depth);
    ev.max_depth = 100;
    ev.depth = 100;
    match leaf.call(ctx, &[Value::symbol("car")]) {
        NativeRun::Signal => {
            let flow = take_pending_flow().expect("stashed flow");
            assert!(
                outcome(Err(flow)).contains("max-lisp-eval-depth"),
                "at the limit: the nesting error"
            );
        }
        other => panic!("(fboundp 'car) at the depth limit: {other:?}"),
    }
    ev.depth = 99;
    match leaf.call(ctx, &[Value::symbol("car")]) {
        NativeRun::Ok(bits) => assert_eq!(
            crate::emacs_core::print::print_value(&Value::from_bits(bits)),
            "t",
            "one below the limit"
        ),
        other => panic!("(fboundp 'car) one below the limit: {other:?}"),
    }
    ev.depth = depth;
    ev.max_depth = max_depth;
    // A redefinition is honoured: the site re-validates and takes the
    // generic call.
    let original = ev
        .obarray
        .symbol_function_id(intern("fboundp"))
        .expect("bound");
    ev.eval_str("(fset 'fboundp (lambda (_) 'redefined))")
        .expect("fset");
    match leaf.call(ctx, &[Value::symbol("car")]) {
        NativeRun::Ok(bits) => assert_eq!(
            crate::emacs_core::print::print_value(&Value::from_bits(bits)),
            "redefined"
        ),
        other => panic!("redefined fboundp: {other:?}"),
    }
    ev.obarray
        .set_symbol_function_id(intern("fboundp"), original);
    match leaf.call(ctx, &[Value::symbol("car")]) {
        NativeRun::Ok(bits) => assert_eq!(
            crate::emacs_core::print::print_value(&Value::from_bits(bits)),
            "t"
        ),
        other => panic!("restored fboundp: {other:?}"),
    }
}

/// `(autoload-do-load FUNDEF FUNNAME MACRO-ONLY)` through `Op::Call`, at one,
/// two and three arguments, is the GC-free predicate intrinsic
/// (`SpecCalleeKind::PredAutoloadDoLoad`): a FUNDEF that is not an autoload
/// object comes back as it is with no frame; an autoload object bounces to
/// the generic call, which loads (here: fails to, exactly as the builtin);
/// a redefinition of `autoload-do-load` reaches the generic call.
#[test]
fn autoload_do_load_call_sites_answer_as_the_builtin_on_the_fast_path() {
    use crate::emacs_core::eval::Context;
    use crate::emacs_core::intern::{SymId, intern};
    crate::emacs_core::jit::compile::force_profit_gate_for_test(false);
    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context as *mut u8;
    let binding = ev
        .obarray
        .symbol_function_id(intern("autoload-do-load"))
        .expect("bound");
    for nargs in 1..=3u8 {
        assert_eq!(
            super::subr_spec_kind(binding, intern("autoload-do-load"), nargs as usize),
            Some(super::SpecCalleeKind::PredAutoloadDoLoad),
            "{nargs} args"
        );
    }
    let pool_src = "(list (symbol-function 'car) (lambda (x) x) nil t 'sym 5 \"s\" (cons 1 2)
                          (list 'autoload \"jit-adl-missing-file\" nil nil nil)
                          (list 'autoload \"jit-adl-missing-file\" \"doc\" t 'macro))";
    let outcome = |r: Result<Value, crate::emacs_core::error::Flow>| -> String {
        match r {
            Ok(v) => crate::emacs_core::print::print_value(&v),
            Err(crate::emacs_core::error::Flow::Signal(sig)) => format!(
                "signal {} {:?}",
                sig.symbol_name(),
                sig.data
                    .iter()
                    .map(crate::emacs_core::print::print_value)
                    .collect::<Vec<_>>()
            ),
            Err(other) => format!("{other:?}"),
        }
    };
    for nargs in 1..=3u8 {
        // (autoload-do-load x ['jit-adl-name ['macro]]): the site's callee
        // is a constant symbol, the FUNDEF a StackRef, the rest constants.
        let mut f = ByteCodeFunction::new(LambdaParams {
            required: vec![SymId(1)],
            optional: Vec::new(),
            rest: None,
        });
        f.lexical = true;
        let mut ops = vec![Op::Constant(0), Op::StackRef(1)];
        if nargs >= 2 {
            ops.push(Op::Constant(1));
        }
        if nargs >= 3 {
            ops.push(Op::Constant(2));
        }
        ops.push(Op::Call(nargs as u16));
        ops.push(Op::Return);
        f.ops = ops;
        f.constants = vec![
            Value::symbol("autoload-do-load"),
            Value::symbol("jit-adl-name"),
            Value::symbol("macro"),
        ]
        .into();
        f.max_stack = 8;
        let leaf = compile_bytecode_function_with(&f, Some(&ev.obarray)).expect("compiles");
        let pool = ev.eval_str(pool_src).expect("pool");
        crate::emacs_core::eval::push_scratch_gc_root(pool);
        let items = crate::emacs_core::value::list_to_vec(&pool).expect("list");
        #[cfg(debug_assertions)]
        let fast0 = SUBR_SPEC_FAST_COUNT.load(Ordering::Relaxed);
        let mut plain = 0usize;
        for item in &items {
            let funname = if nargs >= 2 {
                Value::symbol("jit-adl-name")
            } else {
                Value::NIL
            };
            let macro_only = if nargs >= 3 {
                Value::symbol("macro")
            } else {
                Value::NIL
            };
            let want = outcome(crate::emacs_core::autoload::builtin_autoload_do_load_3(
                &mut ev, *item, funname, macro_only,
            ));
            let got = match leaf.call(ctx, &[*item]) {
                NativeRun::Ok(bits) => {
                    crate::emacs_core::print::print_value(&Value::from_bits(bits))
                }
                NativeRun::Signal => outcome(Err(take_pending_flow().expect("stashed flow"))),
                other => panic!(
                    "(autoload-do-load {} ...) with {nargs} args: {other:?}",
                    crate::emacs_core::print::print_value(item)
                ),
            };
            assert_eq!(
                got,
                want,
                "(autoload-do-load {} ...) with {nargs} args",
                crate::emacs_core::print::print_value(item)
            );
            if !crate::emacs_core::autoload::is_autoload_value(item) {
                plain += 1;
            }
        }
        #[cfg(debug_assertions)]
        assert!(
            SUBR_SPEC_FAST_COUNT.load(Ordering::Relaxed) - fast0 >= plain as u64,
            "{nargs} args: every non-autoload FUNDEF takes the intrinsic"
        );
        let _ = plain;
    }
    // A redefinition is honoured: the site re-validates and takes the
    // generic call.
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: vec![SymId(1)],
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = vec![Op::Constant(0), Op::StackRef(1), Op::Call(1), Op::Return];
    f.constants = vec![Value::symbol("autoload-do-load")].into();
    f.max_stack = 8;
    let leaf = compile_bytecode_function_with(&f, Some(&ev.obarray)).expect("compiles");
    let original = ev
        .obarray
        .symbol_function_id(intern("autoload-do-load"))
        .expect("bound");
    ev.eval_str("(fset 'autoload-do-load (lambda (&rest _) 'redefined))")
        .expect("fset");
    match leaf.call(ctx, &[Value::make_int(1)]) {
        NativeRun::Ok(bits) => assert_eq!(
            crate::emacs_core::print::print_value(&Value::from_bits(bits)),
            "redefined"
        ),
        other => panic!("redefined autoload-do-load: {other:?}"),
    }
    ev.obarray
        .set_symbol_function_id(intern("autoload-do-load"), original);
    match leaf.call(ctx, &[Value::make_int(1)]) {
        NativeRun::Ok(bits) => assert_eq!(
            crate::emacs_core::print::print_value(&Value::from_bits(bits)),
            "1"
        ),
        other => panic!("restored autoload-do-load: {other:?}"),
    }
}

#[test]
fn type_of_call_sites_answer_as_the_builtin_on_the_fast_path() {
    use crate::emacs_core::eval::Context;
    use crate::emacs_core::intern::SymId;
    crate::emacs_core::jit::compile::force_profit_gate_for_test(false);
    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context as *mut u8;
    let pool_src = "(list 5 (expt 2 70) 1.5 \"s\" (cons 1 2) 'sym nil t :kw \
                    (record 'foo 1) (record (record 'eieio--class 'my-class) 2) \
                    (vector 1) (make-char-table 'x) (make-bool-vector 2 t) \
                    (make-hash-table) (symbol-function 'car) (make-marker) \
                    (current-buffer) (lambda (x) x))";
    for name in ["type-of", "cl-type-of"] {
        let mut f = ByteCodeFunction::new(LambdaParams {
            required: vec![SymId(1)],
            optional: Vec::new(),
            rest: None,
        });
        f.lexical = true;
        f.ops = vec![Op::Constant(0), Op::StackRef(1), Op::Call(1), Op::Return];
        f.constants = vec![Value::symbol(name)].into();
        f.max_stack = 8;
        // The site is the GC-free predicate intrinsic, not a general subr
        // speculation (which counts as a fast completion too).
        let binding = ev
            .obarray
            .symbol_function_id(crate::emacs_core::intern::intern(name))
            .expect("bound");
        assert_eq!(
            super::subr_spec_kind(binding, crate::emacs_core::intern::intern(name), 1),
            Some(if name == "type-of" {
                super::SpecCalleeKind::PredTypeOf
            } else {
                super::SpecCalleeKind::PredClTypeOf
            }),
            "{name}"
        );
        let leaf = compile_bytecode_function_with(&f, Some(&ev.obarray)).expect("compiles");
        let pool = ev.eval_str(pool_src).expect("pool");
        let items = crate::emacs_core::value::list_to_vec(&pool).expect("list");
        #[cfg(debug_assertions)]
        let fast0 = SUBR_SPEC_FAST_COUNT.load(Ordering::Relaxed);
        for item in &items {
            let want = if name == "type-of" {
                crate::emacs_core::builtins::types::builtin_type_of(&[*item])
            } else {
                crate::emacs_core::builtins::types::builtin_cl_type_of(&[*item])
            }
            .expect("builtin answers");
            match leaf.call(ctx, &[*item]) {
                NativeRun::Ok(bits) => assert_eq!(
                    crate::emacs_core::print::print_value(&Value::from_bits(bits)),
                    crate::emacs_core::print::print_value(&want),
                    "({name} {})",
                    crate::emacs_core::print::print_value(item)
                ),
                other => panic!("({name} ...) must run natively: {other:?}"),
            }
        }
        // The two records are answered inline, before the intrinsic shim.
        #[cfg(debug_assertions)]
        let fast = SUBR_SPEC_FAST_COUNT.load(Ordering::Relaxed) - fast0;
        #[cfg(debug_assertions)]
        assert!(
            fast >= items.len() as u64 - 2,
            "{name}: every other call took the intrinsic ({fast} of {})",
            items.len()
        );
        // A redefinition is honoured: the site re-validates and takes the
        // generic call.
        ev.eval_str(&format!("(fset '{name} (lambda (_) 'redefined))"))
            .expect("fset");
        match leaf.call(ctx, &[Value::make_int(1)]) {
            NativeRun::Ok(bits) => assert_eq!(
                crate::emacs_core::print::print_value(&Value::from_bits(bits)),
                "redefined",
                "{name} redefined"
            ),
            other => panic!("redefined {name}: {other:?}"),
        }
    }
}

#[test]
fn an_unrelated_redefinition_keeps_an_inlining_leaf_compiled() {
    use crate::emacs_core::eval::Context;
    use crate::emacs_core::intern::SymId;
    // F = (C a) inlines C = (* x x). Redefining an UNRELATED D moves the
    // function epoch but must not recompile F: its leaf is the same object
    // across the next call. Redefining C does recompile F, which then
    // computes with the new C.
    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context;
    let mk_sym = |name: &str| {
        let s = Value::symbol(name);
        let crate::emacs_core::value::ValueKind::Symbol(id) = s.kind() else {
            panic!("symbol");
        };
        (s, id)
    };
    let mk_fn = |ops: Vec<Op>, consts: Vec<Value>| {
        let mut bf = ByteCodeFunction::new(LambdaParams {
            required: vec![SymId(1)],
            optional: Vec::new(),
            rest: None,
        });
        bf.lexical = true;
        bf.ops = ops;
        bf.constants = consts.into();
        bf.max_stack = 16;
        bf
    };
    let (c_sym, c_id) = mk_sym("jit-keep-c");
    let (_d_sym, d_id) = mk_sym("jit-keep-d");
    ev.obarray.set_symbol_function_id(
        c_id,
        Value::make_bytecode(mk_fn(vec![Op::Dup, Op::Mul, Op::Return], vec![])),
    );
    let f = mk_fn(
        vec![Op::Constant(0), Op::StackRef(1), Op::Call(1), Op::Return],
        vec![c_sym],
    );
    let f_val = Value::make_bytecode(f.clone());
    let run = |ctx: *mut Context| {
        crate::emacs_core::jit::try_run_compiled(ctx, &f, f_val, &[Value::make_int(4)])
    };
    assert!(matches!(run(ctx), Ok(Some(b)) if b == Value::make_int(16).bits()));
    let f_id = f.jit_runtime().compiled_id_or_assign();
    let before =
        crate::emacs_core::jit::cache::compiled_leaf_ptr_for_test(f_id).expect("F is compiled");
    ev.obarray.set_symbol_function_id(
        d_id,
        Value::make_bytecode(mk_fn(vec![Op::Sub1, Op::Return], vec![])),
    );
    assert!(matches!(run(ctx), Ok(Some(b)) if b == Value::make_int(16).bits()));
    assert_eq!(
        crate::emacs_core::jit::cache::compiled_leaf_ptr_for_test(f_id),
        Some(before),
        "an unrelated fset must not recompile an inlining leaf"
    );
    ev.obarray.set_symbol_function_id(
        c_id,
        Value::make_bytecode(mk_fn(vec![Op::Add1, Op::Return], vec![])),
    );
    assert!(
        matches!(run(ctx), Ok(Some(b)) if b == Value::make_int(5).bits()),
        "F recompiled against the new C: (1+ 4)"
    );
}

#[test]
fn refsetting_the_same_function_and_repeated_redefinitions_stop_recompiling() {
    use crate::emacs_core::eval::Context;
    use crate::emacs_core::intern::SymId;
    // F = (C a) inlines C. Storing C's own value again redefines nothing, so F
    // stays compiled. Redefining C for real evicts F each time, but after
    // two such evictions C counts as unstable: F recompiles CALLING C, and
    // later redefinitions no longer touch F -- while every answer follows
    // the current C.
    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context;
    let c_sym = Value::symbol("jit-unstable-c");
    let crate::emacs_core::value::ValueKind::Symbol(c_id) = c_sym.kind() else {
        panic!("symbol");
    };
    let mk_fn = |ops: Vec<Op>, consts: Vec<Value>| {
        let mut bf = ByteCodeFunction::new(LambdaParams {
            required: vec![SymId(1)],
            optional: Vec::new(),
            rest: None,
        });
        bf.lexical = true;
        bf.ops = ops;
        bf.constants = consts.into();
        bf.max_stack = 16;
        bf
    };
    let square = Value::make_bytecode(mk_fn(vec![Op::Dup, Op::Mul, Op::Return], vec![]));
    let add1 = Value::make_bytecode(mk_fn(vec![Op::Add1, Op::Return], vec![]));
    ev.obarray.set_symbol_function_id(c_id, square);
    let f = mk_fn(
        vec![Op::Constant(0), Op::StackRef(1), Op::Call(1), Op::Return],
        vec![c_sym],
    );
    let f_val = Value::make_bytecode(f.clone());
    // `None` when F ran interpreted: once C is only called, a call-only
    // body may stay below the JIT's profit gate.
    let run = |ctx: *mut Context| match crate::emacs_core::jit::try_run_compiled(
        ctx,
        &f,
        f_val,
        &[Value::make_int(4)],
    ) {
        Ok(bits) => bits,
        Err(flow) => panic!("F signalled: {flow:?}"),
    };
    assert_eq!(run(ctx), Some(Value::make_int(16).bits()));
    let f_id = f.jit_runtime().compiled_id_or_assign();
    let leaf = crate::emacs_core::jit::cache::compiled_leaf_ptr_for_test(f_id);
    ev.obarray.set_symbol_function_id(c_id, square);
    assert_eq!(run(ctx), Some(Value::make_int(16).bits()));
    assert_eq!(
        crate::emacs_core::jit::cache::compiled_leaf_ptr_for_test(f_id),
        leaf,
        "an fset to the same function redefines nothing"
    );
    for (round, (def, want)) in [(add1, 5), (square, 16), (add1, 5), (square, 16)]
        .into_iter()
        .enumerate()
    {
        ev.obarray.set_symbol_function_id(c_id, def);
        if let Some(bits) = run(ctx) {
            assert_eq!(bits, Value::make_int(want).bits(), "round {round}");
        }
    }
    assert!(crate::emacs_core::jit::cache::inline_callee_is_unstable(
        c_id
    ));
    assert_eq!(
        crate::emacs_core::jit::cache::inline_dependent_count_for_test(c_id),
        0,
        "an unstable callee is called, not inlined"
    );
}
