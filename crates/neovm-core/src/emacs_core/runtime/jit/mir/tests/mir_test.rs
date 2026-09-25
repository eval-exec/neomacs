use super::*;
use crate::emacs_core::bytecode::opcode::Op;

/// A simple arithmetic body builds the expected MIR shape.
#[test]
fn builds_add_body() {
    // (lambda (a b) (+ a b)): StackRef(1) StackRef(1) Add Return.
    let ops = vec![Op::StackRef(1), Op::StackRef(1), Op::Add, Op::Return];
    let mir = build_mir(&ops, &[], 2).expect("builds");
    assert_eq!(mir.arity, 2);
    assert_eq!(mir.blocks.len(), 1, "one block");
    let blk = &mir.blocks[0];
    assert_eq!(blk.params.len(), 2, "two arg params");
    // The Add result is a fixnum-typed Bin op.
    let add = blk
        .insts
        .iter()
        .find(|i| matches!(i.op, MirOp::Bin(BinKind::Add, _, _)))
        .expect("has an Add");
    assert_eq!(add.ty, LispType::Fixnum);
    assert!(matches!(blk.term, MirTerm::Return(_)));
}

/// A branchy body: block count + entry depths mirror analyze_cfg.
#[test]
fn builds_branch_body_matching_cfg() {
    // (lambda (n) (if (< n 2) n (1+ n))):
    //  0 StackRef(0); 1 Constant(0)=2; 2 Lss; 3 GotoIfNil(6);
    //  4 StackRef(0); 5 Return; 6 StackRef(0); 7 Add1; 8 Return
    let ops = vec![
        Op::StackRef(0),
        Op::Constant(0),
        Op::Lss,
        Op::GotoIfNil(6),
        Op::StackRef(0),
        Op::Return,
        Op::StackRef(0),
        Op::Add1,
        Op::Return,
    ];
    let constants = vec![Value::make_int(2)];
    let cfg = analyze_cfg(&ops, &constants, None, 1).unwrap();
    let mir = build_mir(&ops, &constants, 1).expect("builds");
    // One MIR block per bytecode leader, same leaders.
    assert_eq!(mir.blocks.len(), cfg.leaders.len());
    for blk in &mir.blocks {
        assert_eq!(
            blk.params.len(),
            cfg.entry_depth[&blk.bytecode_pc],
            "block params == analyze_cfg entry depth at pc {}",
            blk.bytecode_pc
        );
    }
    // The first block ends in a conditional branch.
    assert!(matches!(mir.blocks[0].term, MirTerm::Branch { .. }));
}

/// A loop with a back-edge builds without trouble (forward and backward
/// edges are just successor edges carrying the live stack).
#[test]
fn builds_loop_with_backedge() {
    // (lambda (n) (while (> n 0) (setq n (1- n))) n):
    //  0 StackRef(0); 1 Constant(0)=0; 2 Gtr; 3 GotoIfNil(8);
    //  4 StackRef(0); 5 Sub1; 6 StackSet(1); 7 Goto(0);
    //  8 StackRef(0); 9 Return
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
    let constants = vec![Value::make_int(0)];
    let mir = build_mir(&ops, &constants, 1).expect("builds");
    // The backedge block ends in a Goto back to block0.
    let has_backedge = mir
        .blocks
        .iter()
        .any(|b| matches!(b.term, MirTerm::Goto { target, .. } if target == MirBlockId(0)));
    assert!(has_backedge, "loop produces a back-edge Goto to block0");
}

/// Opaque ops (a builtin call) preserve stack discipline via simple_effect.
#[test]
fn opaque_op_preserves_stack() {
    // (lambda (s) (length s)): StackRef(0) Length Return.
    let ops = vec![Op::StackRef(0), Op::Length, Op::Return];
    let mir = build_mir(&ops, &[], 1).expect("builds");
    let blk = &mir.blocks[0];
    // Length is opaque (one operand -> one result).
    assert!(
        blk.insts
            .iter()
            .any(|i| matches!(&i.op, MirOp::Opaque { op: Op::Length, .. })),
        "length is modelled opaque"
    );
    assert!(matches!(blk.term, MirTerm::Return(_)));
}

/// Regression: a 0-result side-effecting opaque op (here `VarSet`, which pops
/// its value and pushes nothing) must still be EMITTED into the MIR. It was
/// once emitted inside `for _ in 0..produces`, so `produces == 0` dropped it
/// entirely — `lower_mir_pure` then never saw the Opaque it bails on and the
/// MIR tier silently claimed the body, making a compiled `(setq special val)`
/// a no-op (it returned `val` via the bytecode `dup` but never assigned).
/// The same class covers `VarBind`/`Save*`/`UnwindProtectPop`.
#[test]
fn zero_result_opaque_op_is_not_dropped() {
    // (setq v 7): Constant 7, Dup (the setq return value), VarSet v, Return.
    let ops = vec![Op::Constant(1), Op::Dup, Op::VarSet(0), Op::Return];
    let constants = vec![Value::symbol("v"), Value::make_int(7)];
    let mir = build_mir(&ops, &constants, 0).expect("builds");
    assert!(
        mir.blocks
            .iter()
            .flat_map(|b| b.insts.iter())
            .any(|i| matches!(
                &i.op,
                MirOp::Opaque {
                    op: Op::VarSet(0),
                    ..
                }
            )),
        "a 0-result VarSet must survive into the MIR (else the side effect is dropped)"
    );
}

/// `build_mir`'s own unmodelled-control bail: a `Throw` body passes
/// `analyze_cfg` (which treats Throw as a terminator) but the Phase 4a MIR
/// builder defers it.
#[test]
fn bails_on_unmodelled_control() {
    // tag, value, Throw — analyze_cfg accepts (Throw terminates); MIR bails.
    let ops = vec![Op::Constant(0), Op::Constant(1), Op::Throw];
    let constants = vec![Value::symbol("tag"), Value::make_int(1)];
    assert!(matches!(
        build_mir(&ops, &constants, 0),
        Err(CompileError::UnsupportedOp("mir-unmodelled-control"))
    ));
    // A non-constant-table Switch is caught earlier, by analyze_cfg — also
    // not buildable, which is all Phase 4a needs.
    let sw = vec![Op::Nil, Op::Nil, Op::Switch, Op::Nil, Op::Return];
    assert!(build_mir(&sw, &[], 0).is_err());
}

#[test]
fn mir_inst_pre_stack_captures_inst_less_folds() {
    // [Const a, Const b, StackRef 1, Add, Return]: StackRef 1 copies slot 0 (a)
    // to the top as an INST-LESS fold (no MirInst). The Add inst's pre_stack
    // must still reflect the folded stack ([a, b, a]) — otherwise a precise
    // deopt at Add would spill the wrong operand stack (the Hole-1 silent
    // miscompile). This is the regression guard for framestate non-lossiness.
    let ops = vec![
        Op::Constant(0),
        Op::Constant(1),
        Op::StackRef(1),
        Op::Add,
        Op::Return,
    ];
    let constants = vec![Value::make_int(10), Value::make_int(20)];
    let mir = build_mir(&ops, &constants, 0).expect("builds");
    let add = mir
        .blocks
        .iter()
        .flat_map(|b| &b.insts)
        .find(|i| matches!(i.op, MirOp::Bin(BinKind::Add, _, _)))
        .expect("has an Add inst");
    assert_eq!(add.pc, 3, "Add is at bytecode pc 3");
    assert_eq!(
        add.pre_stack.len(),
        3,
        "pre_stack must include the inst-less StackRef fold: [a, b, a]"
    );
    assert_eq!(
        add.pre_stack[0], add.pre_stack[2],
        "StackRef 1 copied slot 0's value to the top — same SSA value"
    );
}

#[test]
fn inline_composes_substitutions_for_returned_params() {
    // id = (lambda (v) v); caller (lambda (x) (id (id x))). Both calls inline,
    // and each id returns its param, so the result substitutions CHAIN (outer
    // result -> inner result -> arg). A composed substitution must resolve the
    // Return to the ARG; a sequential one reintroduces the deleted inner-call
    // result (a dangling value). Regression for the splice substitution.
    let id_sym = Value::symbol("jit-inline-id");
    let id_ops = vec![Op::StackRef(0), Op::Return];
    let caller_ops = vec![
        Op::Constant(0), // id (outer fn)
        Op::Constant(0), // id (inner fn)
        Op::StackRef(2), // x
        Op::Call(1),     // (id x)
        Op::Call(1),     // (id (id x))
        Op::Return,
    ];
    let constants = vec![id_sym];
    let mut m = build_mir(&caller_ops, &constants, 1).expect("caller builds");
    let n = inline_pure_single_block_callees(
        &mut m,
        &|_, v| (v.bits() == id_sym.bits()).then(|| build_mir(&id_ops, &[], 1).expect("id builds")),
        8,
        &mut Vec::new(),
    );
    assert_eq!(n, 2, "both id calls inline");
    let MirTerm::Return(rv) = m.blocks[0].term else {
        panic!("single Return block");
    };
    assert_eq!(
        rv, m.blocks[0].params[0],
        "(id (id x)) must return the arg x — composed substitution resolves the chain"
    );
}

#[test]
fn cons_scalar_repl_finds_only_non_escaping_conses() {
    // (car (cons a b)) — the cons is consumed only by car -> replaceable.
    let ops = [
        Op::StackRef(1),
        Op::StackRef(1),
        Op::Cons,
        Op::Car,
        Op::Return,
    ];
    let m = build_mir(&ops, &[], 2).expect("builds");
    assert!(
        cons_scalar_repl_targets(&m).iter().any(|r| r.is_some()),
        "(car (cons a b)) cons is scalar-replaceable"
    );
    // A RETURNED cons escapes -> not replaceable.
    let ops2 = [Op::StackRef(1), Op::StackRef(1), Op::Cons, Op::Return];
    let m2 = build_mir(&ops2, &[], 2).expect("builds");
    assert!(
        cons_scalar_repl_targets(&m2).iter().all(|r| r.is_none()),
        "a returned cons escapes"
    );
}

#[test]
fn bails_on_unreachable_block() {
    // index 2 is a block leader (it follows the unconditional Goto), but
    // nothing targets it -> unreachable, so analyze_cfg assigns it no entry
    // depth. build_mir must bail (the body falls back to the baseline), not
    // panic indexing the missing entry depth. (Regression: wiring lower_mir_pure
    // panicked here on a real module-path function with dead code.)
    let ops = vec![
        Op::Constant(0),
        Op::Goto(4),
        Op::Constant(0), // unreachable leader
        Op::Return,
        Op::Return,
    ];
    let constants = vec![Value::make_int(7)];
    assert!(matches!(
        build_mir(&ops, &constants, 0),
        Err(CompileError::UnsupportedOp("mir-unreachable-block"))
    ));
}

/// The GC-root filter must skip exactly the immediates + symbols (what
/// `neovm_jit_gc_push` skips at runtime) and root everything heap or
/// unknown. A wrong `true` here would drop a live heap root under GC =
/// use-after-free, so this pins the whole table.
/// GNU 31.1's byte-compile of `probe-a` (tmp/eb/probe-loop.el):
/// `(let ((i 0) (acc 0)) (while (< i n) (setq acc (+ acc i)) (setq i (1+ i))) (if v acc acc))`.
/// Leaders {0, 2, 6, 14}; the stack at every non-entry leader is [n v i acc];
/// the `+` is at pc 8.
fn probe_a_ops() -> Vec<Op> {
    vec![
        Op::Constant(0), // 0: i = 0
        Op::Dup,         // 1: acc = 0
        Op::StackRef(1), // 2: i          (loop header)
        Op::StackRef(4), // 3: n
        Op::Lss,         // 4
        Op::GotoIfNil(14),
        Op::Dup,         // 6: acc       (body)
        Op::StackRef(2), // 7: i
        Op::Add,         // 8
        Op::StackSet(1), // 9: acc = ...
        Op::StackRef(1), // 10: i
        Op::Add1,        // 11
        Op::StackSet(2), // 12: i = ...
        Op::Goto(2),
        Op::Return, // 14
    ]
}

fn bin_insts(m: &MirFunction) -> Vec<&MirInst> {
    m.blocks
        .iter()
        .flat_map(|b| b.insts.iter())
        .filter(|i| matches!(i.op, MirOp::Bin(..)))
        .collect()
}

/// The Float rule: `+` at a `Float` site is typed `Any` (a boxed float or a
/// fixnum), everything else keeps its fixnum typing, and the feedback-blind
/// `build_mir` still types the same `+` `Fixnum`.
#[test]
fn float_feedback_bin_result_is_typed_any() {
    let ops = probe_a_ops();
    let constants = vec![Value::make_int(0)];
    let fb = |pc: usize| {
        if pc == 8 {
            NumericFeedback::Float
        } else {
            NumericFeedback::FixnumOnly
        }
    };
    let m = build_mir_with_feedback(&ops, &constants, 2, &fb).expect("builds");
    let adds = bin_insts(&m);
    assert_eq!(adds.len(), 1);
    assert_eq!(adds[0].pc, 8);
    assert_eq!(
        adds[0].ty,
        LispType::Any,
        "a Float-site `+` is not a known fixnum"
    );
    assert_eq!(m.value_type(adds[0].result), LispType::Any);
    let add1 = m
        .blocks
        .iter()
        .flat_map(|b| b.insts.iter())
        .find(|i| matches!(i.op, MirOp::Unary(UnaryKind::Add1, _)))
        .expect("the 1+");
    assert_eq!(add1.ty, LispType::Fixnum, "1+ has no float lowering");
    let blind = build_mir(&ops, &constants, 2).expect("builds");
    assert_eq!(bin_insts(&blind)[0].ty, LispType::Fixnum);
}

/// All four float-lowered ops take the rule, and only at their own pc.
#[test]
fn float_rule_covers_add_sub_mul_div_per_site() {
    // (a b): [a b a a] + -> [a b r1]; [a b r1 b] - -> r2; [.. r2 a] * -> r3;
    // [.. r3 b] / -> r4.
    let ops = vec![
        Op::StackRef(1),
        Op::StackRef(1),
        Op::Add, // pc 2
        Op::StackRef(1),
        Op::Sub, // pc 4
        Op::StackRef(2),
        Op::Mul, // pc 6
        Op::StackRef(1),
        Op::Div, // pc 8
        Op::Return,
    ];
    let all = build_mir_with_feedback(&ops, &[], 2, &|_| NumericFeedback::Float).expect("builds");
    let tys: Vec<LispType> = bin_insts(&all).iter().map(|i| i.ty).collect();
    assert_eq!(tys, vec![LispType::Any; 4]);
    for float_pc in [2usize, 4, 6, 8] {
        let m = build_mir_with_feedback(&ops, &[], 2, &|pc| {
            if pc == float_pc {
                NumericFeedback::Float
            } else {
                NumericFeedback::FixnumOnly
            }
        })
        .expect("builds");
        for inst in bin_insts(&m) {
            let want = if inst.pc == float_pc {
                LispType::Any
            } else {
                LispType::Fixnum
            };
            assert_eq!(
                inst.ty, want,
                "Float at pc {float_pc}: inst at pc {}",
                inst.pc
            );
        }
    }
}

/// `% max min` and the unary ops have no float lowering: both operands are
/// guarded and the result range-checked, so they stay `Fixnum` whatever the
/// feedback says.
#[test]
fn rem_max_min_unary_ignore_float_feedback() {
    let ops = vec![
        Op::StackRef(1),
        Op::StackRef(1),
        Op::Rem,
        Op::StackRef(2),
        Op::Max,
        Op::StackRef(2),
        Op::Min,
        Op::Add1,
        Op::Sub1,
        Op::Negate,
        Op::Return,
    ];
    let m = build_mir_with_feedback(&ops, &[], 2, &|_| NumericFeedback::Float).expect("builds");
    for inst in &m.blocks[0].insts {
        if matches!(inst.op, MirOp::Bin(..) | MirOp::Unary(..)) {
            assert_eq!(inst.ty, LispType::Fixnum, "{:?}", inst.op);
        }
    }
}

/// The probe-a loop: `i` and `acc` are `Fixnum` at every non-entry leader
/// (a fixnum constant on the entry edge, a `Bin`/`Unary` result on the back
/// edge); `n` and `v` are the untyped arguments.
#[test]
fn infers_probe_a_loop_params() {
    let ops = probe_a_ops();
    let constants = vec![Value::make_int(0)];
    let m = build_mir(&ops, &constants, 2).expect("builds");
    let ty = infer_value_types(&m);
    assert_eq!(m.blocks.len(), 4);
    for blk in &m.blocks[1..] {
        let got: Vec<LispType> = blk.params.iter().map(|p| ty[p.0 as usize]).collect();
        assert_eq!(
            got,
            vec![
                LispType::Any,
                LispType::Any,
                LispType::Fixnum,
                LispType::Fixnum
            ],
            "leader pc {}",
            blk.bytecode_pc
        );
    }
    assert_eq!(
        m.blocks[0]
            .params
            .iter()
            .map(|p| ty[p.0 as usize])
            .collect::<Vec<_>>(),
        vec![LispType::Any, LispType::Any]
    );
}

/// A diamond that merges a fixnum with nil: the merge param is `Any`.
#[test]
fn merge_of_fixnum_and_nil_is_any() {
    // 0 Constant(0)=0; 1 GotoIfNil(4); 2 Constant(1)=9; 3 Goto(5);
    // 4 Constant(2)=nil; 5 Return
    let ops = vec![
        Op::Constant(0),
        Op::GotoIfNil(4),
        Op::Constant(1),
        Op::Goto(5),
        Op::Constant(2),
        Op::Return,
    ];
    let constants = vec![Value::make_int(0), Value::make_int(9), Value::NIL];
    let m = build_mir(&ops, &constants, 0).expect("builds");
    let ty = infer_value_types(&m);
    let merge = m
        .blocks
        .iter()
        .find(|b| b.bytecode_pc == 5)
        .expect("the merge block");
    assert_eq!(merge.params.len(), 1);
    assert_eq!(ty[merge.params[0].0 as usize], LispType::Any);
}

/// Block 0 as a loop header: its param is the untyped argument and stays
/// `Any` even though the back edge feeds it a `Sub1` result.
#[test]
fn block_zero_loop_header_param_stays_any() {
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
    let constants = vec![Value::make_int(0)];
    let m = build_mir(&ops, &constants, 1).expect("builds");
    let ty = infer_value_types(&m);
    assert_eq!(ty[m.blocks[0].params[0].0 as usize], LispType::Any);
    // The exit block's param is fed only by block 0's param: Any too.
    let exit = m.blocks.iter().find(|b| b.bytecode_pc == 8).expect("exit");
    assert_eq!(ty[exit.params[0].0 as usize], LispType::Any);
}

/// The Float rule end-to-end: with `+` at a Float site, `acc` is `Any` at
/// every leader (so a lowering would keep its guard) while `i` stays
/// `Fixnum`.
#[test]
fn float_feedback_keeps_header_param_any() {
    let ops = probe_a_ops();
    let constants = vec![Value::make_int(0)];
    let fb = |pc: usize| {
        if pc == 8 {
            NumericFeedback::Float
        } else {
            NumericFeedback::FixnumOnly
        }
    };
    let m = build_mir_with_feedback(&ops, &constants, 2, &fb).expect("builds");
    let ty = infer_value_types(&m);
    for blk in &m.blocks[1..] {
        let got: Vec<LispType> = blk.params.iter().map(|p| ty[p.0 as usize]).collect();
        assert_eq!(
            got,
            vec![
                LispType::Any,
                LispType::Any,
                LispType::Fixnum,
                LispType::Any
            ],
            "leader pc {}",
            blk.bytecode_pc
        );
    }
}

/// A loop whose back-edge value is an inlined callee's result: after the
/// inline pass the header param is `Fixnum` — the analysis runs on the
/// inlined function and reads no pc.
#[test]
fn inlined_callee_result_flows_fixnum_into_header() {
    // sq = (lambda (x) (* x x));
    // caller = (lambda (n) (let ((m 1)) (while (> m 0) (setq m (sq m))) m)),
    // whose loop header (pc 1) is a NON-entry block:
    //  0 Constant(0)=1; 1 StackRef(0); 2 Constant(1)=0; 3 Gtr; 4 GotoIfNil(10);
    //  5 Constant(2)=sq; 6 StackRef(1); 7 Call(1); 8 StackSet(1); 9 Goto(1);
    //  10 Return
    let sq_sym = Value::symbol("jit-inline-sq");
    let sq_ops = vec![Op::StackRef(0), Op::StackRef(1), Op::Mul, Op::Return];
    let caller_ops = vec![
        Op::Constant(0),
        Op::StackRef(0),
        Op::Constant(1),
        Op::Gtr,
        Op::GotoIfNil(10),
        Op::Constant(2),
        Op::StackRef(1),
        Op::Call(1),
        Op::StackSet(1),
        Op::Goto(1),
        Op::Return,
    ];
    let constants = vec![Value::make_int(1), Value::make_int(0), sq_sym];
    let mut m = build_mir(&caller_ops, &constants, 1).expect("caller builds");
    let header = m
        .blocks
        .iter()
        .position(|b| b.bytecode_pc == 1)
        .expect("header at pc 1");
    let m_param = m.blocks[header].params[1];
    assert_eq!(
        infer_value_types(&m)[m_param.0 as usize],
        LispType::Any,
        "before inlining the back edge carries an opaque Call result"
    );
    let n = inline_pure_single_block_callees(
        &mut m,
        &|_, v| (v.bits() == sq_sym.bits()).then(|| build_mir(&sq_ops, &[], 1).expect("sq builds")),
        8,
        &mut Vec::new(),
    );
    assert_eq!(n, 1);
    assert_eq!(
        infer_value_types(&m)[m_param.0 as usize],
        LispType::Fixnum,
        "the inlined `*` result reaches the header"
    );
}

#[test]
fn never_needs_gc_root_matches_runtime_skip_set() {
    use LispType::*;
    for ty in [Fixnum, Nil, True, Boolean, Symbol] {
        assert!(
            ty.never_needs_gc_root(),
            "{ty:?} is immediate/obarray-rooted"
        );
    }
    for ty in [Cons, Str, Float, Veclike, Unknown, Any] {
        assert!(
            !ty.never_needs_gc_root(),
            "{ty:?} can be heap (or unknown) and MUST be rooted"
        );
    }
    // The three GC-root codegen buckets must partition the type lattice:
    // provably-immediate (skip), provably-heap (unconditional push), and
    // Unknown/Any (inlined runtime tag test). No type may fall in two
    // buckets, or the codegen would both skip and unconditionally push it.
    for ty in [Fixnum, Nil, True, Boolean, Symbol] {
        assert!(!ty.provably_heap(), "{ty:?} is immediate, not heap");
    }
    for ty in [Cons, Str, Float, Veclike] {
        assert!(ty.provably_heap(), "{ty:?} is always a heap object");
    }
    for ty in [Unknown, Any] {
        assert!(
            !ty.never_needs_gc_root() && !ty.provably_heap(),
            "{ty:?} is ambiguous — runtime tag test, neither skip nor unconditional push"
        );
    }
}
