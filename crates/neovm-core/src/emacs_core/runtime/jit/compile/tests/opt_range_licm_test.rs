use super::*;

// Native Range/LICM parity through the existing opt_reps test helpers.
// No fabricated UnsupportedOp stub, no guessed expected arithmetic answers.
// Each source is run by Tier0 and existing checked native lowering BEFORE the
// pass/candidate assertion. GNU grounding is frozen67 arithmetic/phi/cold rows.
// Threading: compiler-owned plans and existing scratch roots per invocation.

use crate::emacs_core::jit::opt::passes::{bools, cfg, fold, gvn, licm, range, reps, reps_lift};

fn draft_o35_settings() -> Settings {
    let settings = Settings::enter();
    force_opt_passes_for_test(Some(OptPasses {
        fold: true,
        bool_rep: true,
        gvn: true,
        range: true,
        licm: true,
        reps: true,
        ..OptPasses::default()
    }));
    settings
}

fn draft_o35_prepared(source: &ByteCodeFunction) -> ir::Func {
    let mut func = plan(source);
    reps_lift::run(&mut func, &[]).unwrap();
    fold::run(&mut func).unwrap();
    cfg::cleanup(&mut func).unwrap();
    bools::run(&mut func).unwrap();
    gvn::run(&mut func).unwrap();
    func.verify().unwrap();
    func
}

fn draft_o35_reference(ctx: &mut Context, func: &ir::Func, args: &[Value]) -> eval::Outcome {
    eval::evaluate(
        func,
        ctx,
        eval::Inputs {
            args,
            ..eval::Inputs::default()
        },
    )
    .unwrap()
    .outcome
}

fn draft_o35_check_ok(
    ctx: &mut Context,
    source: &ByteCodeFunction,
    func: &ir::Func,
    leaf: &CompiledLeaf,
    args: &[Value],
) {
    let _roots = Roots::new(args);
    let expected = tier0(ctx, source, args).unwrap();
    let _expected = Roots::new(&[expected]);
    let eval::Outcome::Returned(reference) = draft_o35_reference(ctx, func, args) else {
        panic!("input-valid reference success precedes optimization evidence")
    };
    assert_eq!(print_value(&reference.to_value()), print_value(&expected));
    assert_eq!(
        print_value(&native_ok(ctx, leaf, args)),
        print_value(&expected)
    );
    assert_eq!(ctx.jit_root_stack_top, 0);
}

fn draft_o35_census_ops(func: &ir::Func, checked: bool) -> usize {
    func.blocks
        .iter()
        .flat_map(|b| &b.insts)
        .filter(|id| {
            matches!(
                func.insts[id.index()].op,
                ir::Opcode::FixAdd { checked: value } | ir::Opcode::FixSub { checked: value }
                    | ir::Opcode::FixMul { checked: value } if value == checked
            )
        })
        .count()
}

fn draft_o35_observations(after: &ir::Func, before: &ir::Func) {
    assert_eq!(after.frames, before.frames);
    assert_eq!(after.source_states.len(), before.source_states.len());
    for (actual, expected) in after.source_states.iter().zip(&before.source_states) {
        match (actual, expected) {
            (Some(a), Some(b)) => {
                assert_eq!(a.block, b.block);
                assert_eq!(a.frame, b.frame);
                assert_eq!(a.pre, b.pre);
                assert_eq!(a.post, b.post);
            }
            (None, None) => {}
            _ => panic!("range preserves original source observation identity"),
        }
    }
}

/// Existing manual typed helper widens conversion declarations to FIXNUM.
/// Preserve actual operand interval declarations for these new fixtures: this
/// is propagation through proved pure Tag/Untag identities, never Arg stamping.
fn draft_o35_explicit_rep(func: &mut ir::Func, rep: ir::Rep) {
    let targets = func
        .insts
        .iter()
        .enumerate()
        .filter_map(|(n, i)| {
            let op = match i.op {
                ir::Opcode::FixAdd { checked: true } | ir::Opcode::Opaque(Op::Add) => {
                    ir::Opcode::FixAdd { checked: true }
                }
                ir::Opcode::FixSub { checked: true } | ir::Opcode::Opaque(Op::Sub) => {
                    ir::Opcode::FixSub { checked: true }
                }
                ir::Opcode::FixMul { checked: true } | ir::Opcode::Opaque(Op::Mul) => {
                    ir::Opcode::FixMul { checked: true }
                }
                _ => return None,
            };
            Some((ir::Inst(n as u32), op))
        })
        .collect::<Vec<_>>();
    for (id, opcode) in targets {
        typed_binary(func, id, opcode, rep);
    }
    // Definitions inserted by typed_binary are in producer order. Iterate a
    // bounded number because independent source blocks may have different IDs.
    for _ in 0..func.values.len() {
        let mut changed = false;
        for inst in &func.insts {
            if matches!(inst.op, ir::Opcode::TagFix | ir::Opcode::UntagFix) {
                let input = func.resolve(inst.args[0]).unwrap();
                let ty = func.values[input.index()].ty;
                let result = inst.result.unwrap();
                changed |= func.values[result.index()].ty != ty;
                func.values[result.index()].ty = ty;
            }
        }
        if !changed {
            break;
        }
    }
    repair_views(func);
    tag_returns(func);
    func.verify().unwrap();
}

#[test]
fn opt_range_native_full_domain_zero_and_unit_proofs() {
    let _settings = draft_o35_settings();
    let mut ctx = Context::new();
    // Frozen fix-min-zero/fix-max-one and exact ordinary arithmetic semantics.
    // These are full-domain proofs, not manually narrowed runtime arguments.
    for rep in [ir::Rep::TaggedFix, ir::Rep::RawInt] {
        for (op, literal) in [(Op::Add, 0), (Op::Sub, 0), (Op::Mul, 0), (Op::Mul, 1)] {
            let source = function(
                vec![Op::StackRef(0), Op::Constant(0), op, Op::Return],
                vec![Value::fixnum(literal)],
                1,
            );
            // Build the original source before manual representation exposure.
            // Existing lift/fold projections already encode their representation;
            // replacing their producer without boundary repair is not a valid fixture.
            let mut before = plan(&source);
            draft_o35_explicit_rep(&mut before, rep);
            let baseline = lower(&source, &before);
            for input in [
                Value::MOST_NEGATIVE_FIXNUM,
                -1,
                0,
                1,
                Value::MOST_POSITIVE_FIXNUM,
            ] {
                draft_o35_check_ok(
                    &mut ctx,
                    &source,
                    &before,
                    &baseline,
                    &[Value::fixnum(input)],
                );
            }
            let mut after = before.clone();
            let stats = range::run(&mut after).unwrap();
            after.verify().unwrap();
            let candidate = lower(&source, &after);
            for input in [
                Value::MOST_NEGATIVE_FIXNUM,
                -1,
                0,
                1,
                Value::MOST_POSITIVE_FIXNUM,
            ] {
                draft_o35_check_ok(
                    &mut ctx,
                    &source,
                    &after,
                    &candidate,
                    &[Value::fixnum(input)],
                );
            }
            assert_eq!(stats.overflow_checks_elided, 1);
            assert_eq!(draft_o35_census_ops(&after, false), 1);
            draft_o35_observations(&after, &before);
            // The native emitter's independent endpoint validation admits the
            // real unchecked opcode. CLIF opcode count is secondary evidence.
            let clif = captured_clif(|| {
                let _ = lower(&source, &after);
            })
            .join("\n");
            assert!(
                !clif.contains("sadd_overflow")
                    && !clif.contains("ssub_overflow")
                    && !clif.contains("smul_overflow")
            );
        }
    }
}

#[test]
fn opt_range_native_real_interval_guards_prove_add_sub_mul_endpoints() {
    let _settings = draft_o35_settings();
    let mut ctx = Context::new();
    let interval =
        TypeSet::fixnum_range(crate::emacs_core::jit::opt::types::Range { lo: -8, hi: 8 });
    for rep in [ir::Rep::TaggedFix, ir::Rep::RawInt] {
        for op in [Op::Add, Op::Sub, Op::Mul] {
            let source = binary(op);
            let mut before = draft_o35_prepared(&source);
            // The interval is checked by actual native guards. No runtime
            // Arg/phi is assigned an interval declaration without a check.
            for inst in &mut before.insts {
                if inst.op == ir::Opcode::CheckType(TypeSet::FIXNUM) {
                    inst.op = ir::Opcode::CheckType(interval);
                    before.values[inst.result.unwrap().index()].ty = interval;
                }
            }
            repair_views(&mut before);
            draft_o35_explicit_rep(&mut before, rep);
            let baseline = lower(&source, &before);
            for a in [-8, 0, 8] {
                for b in [-8, 0, 8] {
                    draft_o35_check_ok(
                        &mut ctx,
                        &source,
                        &before,
                        &baseline,
                        &[Value::fixnum(a), Value::fixnum(b)],
                    );
                }
            }
            // Outside the speculative interval, the checked baseline exits
            // to the original GNU operation and derives its answer in Tier0.
            let outside = [Value::fixnum(9), Value::fixnum(1)];
            let NativeRun::DeoptAt(base_exit) =
                baseline.call(&mut ctx as *mut Context as *mut u8, &outside)
            else {
                panic!("real interval guard is executable")
            };
            assert_eq!(
                resumed(&mut ctx, &source, &base_exit).unwrap(),
                tier0(&mut ctx, &source, &outside).unwrap()
            );
            let mut after = before.clone();
            let stats = range::run(&mut after).unwrap();
            let candidate = lower(&source, &after);
            for a in [-8, 0, 8] {
                for b in [-8, 0, 8] {
                    draft_o35_check_ok(
                        &mut ctx,
                        &source,
                        &after,
                        &candidate,
                        &[Value::fixnum(a), Value::fixnum(b)],
                    );
                }
            }
            let NativeRun::DeoptAt(exit) =
                candidate.call(&mut ctx as *mut Context as *mut u8, &outside)
            else {
                panic!("arithmetic check erasure cannot erase input guard")
            };
            assert_eq!(exit.pc, base_exit.pc);
            assert_eq!(exit.stack, base_exit.stack);
            assert_eq!(stats.overflow_checks_elided, 1);
            assert_eq!(draft_o35_census_ops(&after, false), 1);
        }
    }
}

fn draft_o35_sibling_source() -> ByteCodeFunction {
    function(
        vec![
            Op::StackRef(0),
            Op::Constant(0),
            Op::Lss,
            Op::GotoIfNil(7),
            Op::StackRef(0),
            Op::Add1,
            Op::Return,
            Op::StackRef(0),
            Op::Add1,
            Op::Return,
        ],
        vec![Value::fixnum(Value::MOST_POSITIVE_FIXNUM)],
        1,
    )
}

#[test]
fn opt_range_native_branch_sibling_proof_does_not_leak() {
    let _settings = draft_o35_settings();
    let mut ctx = Context::new();
    let source = draft_o35_sibling_source();
    let before = draft_o35_prepared(&source);
    let original_arg = before
        .insts
        .iter()
        .find(|i| i.op == ir::Opcode::Arg(0))
        .unwrap()
        .result
        .unwrap();
    let baseline = lower(&source, &before);
    draft_o35_check_ok(
        &mut ctx,
        &source,
        &before,
        &baseline,
        &[Value::fixnum(Value::MOST_POSITIVE_FIXNUM - 1)],
    );
    let args = [Value::fixnum(Value::MOST_POSITIVE_FIXNUM)];
    let expected = tier0(&mut ctx, &source, &args).unwrap();
    let _expected = Roots::new(&[expected]);
    let NativeRun::DeoptAt(base_exit) = baseline.call(&mut ctx as *mut Context as *mut u8, &args)
    else {
        panic!("baseline sibling must preserve the actual fixnum promotion")
    };
    assert_eq!(
        print_value(&resumed(&mut ctx, &source, &base_exit).unwrap()),
        print_value(&expected)
    );
    let mut after = before.clone();
    let stats = range::run(&mut after).unwrap();
    reps::run(&mut after).unwrap();
    let candidate = lower(&source, &after);
    draft_o35_check_ok(
        &mut ctx,
        &source,
        &after,
        &candidate,
        &[Value::fixnum(Value::MOST_POSITIVE_FIXNUM - 1)],
    );
    let NativeRun::DeoptAt(exit) = candidate.call(&mut ctx as *mut Context as *mut u8, &args)
    else {
        panic!("true-edge range proof cannot escape into the unsafe sibling")
    };
    assert_eq!(exit.pc, base_exit.pc);
    assert_eq!(exit.stack, base_exit.stack);
    assert_eq!(
        print_value(&resumed(&mut ctx, &source, &exit).unwrap()),
        print_value(&expected)
    );
    assert_eq!(stats.overflow_checks_elided, 1);
    assert_eq!(draft_o35_census_ops(&after, true), 1);
    assert_eq!(
        after.values[original_arg.index()].ty,
        before.values[original_arg.index()].ty
    );
}

fn draft_o35_counter_source() -> ByteCodeFunction {
    // Exact two-phi source-shaped cnt already used by reps_lift tests. Sum is
    // data-dependent and may eventually overflow; only i+1 under i<n is proved.
    function(
        vec![
            Op::Constant(0),
            Op::Constant(1),
            Op::StackRef(0),
            Op::StackRef(3),
            Op::Lss,
            Op::GotoIfNil(14),
            Op::StackRef(1),
            Op::StackRef(1),
            Op::Add,
            Op::StackSet(2),
            Op::StackRef(0),
            Op::Add1,
            Op::StackSet(1),
            Op::Goto(2),
            Op::StackRef(1),
            Op::Return,
        ],
        vec![Value::fixnum(0), Value::fixnum(0)],
        1,
    )
}

#[test]
fn opt_range_native_real_counter_keeps_accumulator_and_poll_cadence() {
    let _settings = draft_o35_settings();
    let mut ctx = Context::new();
    let source = draft_o35_counter_source();
    let before = draft_o35_prepared(&source);
    let header = before.source_states[2].as_ref().unwrap().block;
    let original_phis = before.blocks[header.index()].params.clone();
    assert_eq!(original_phis.len(), 2);
    let baseline = lower(&source, &before);
    for n in [0, 1, 17, 255, 510] {
        reset_bytecode_branch_poll_count();
        let expected = tier0(&mut ctx, &source, &[Value::fixnum(n)]).unwrap();
        assert_eq!(bytecode_branch_poll_count(), (n / 255) as usize);
        reset_bytecode_branch_poll_count();
        assert_eq!(
            native_ok(&mut ctx, &baseline, &[Value::fixnum(n)]),
            expected
        );
        assert_eq!(bytecode_branch_poll_count(), (n / 255) as usize);
    }
    let mut after = before.clone();
    let stats = range::run(&mut after).unwrap();
    let licm_stats = licm::run(&mut after).unwrap();
    let rep_stats = reps::run(&mut after).unwrap();
    let candidate = lower(&source, &after);
    for n in [0, 1, 17, 255, 510] {
        reset_bytecode_branch_poll_count();
        let expected = tier0(&mut ctx, &source, &[Value::fixnum(n)]).unwrap();
        reset_bytecode_branch_poll_count();
        assert_eq!(
            native_ok(&mut ctx, &candidate, &[Value::fixnum(n)]),
            expected
        );
        assert_eq!(bytecode_branch_poll_count(), (n / 255) as usize);
    }
    assert!(stats.overflow_checks_elided >= 1);
    assert_eq!(
        draft_o35_census_ops(&after, true),
        1,
        "sum still checks its data-dependent overflow"
    );
    assert_eq!(after.blocks[header.index()].params, original_phis);
    assert_eq!(
        after
            .insts
            .iter()
            .filter(|i| i.op == ir::Opcode::Poll)
            .count(),
        before
            .insts
            .iter()
            .filter(|i| i.op == ir::Opcode::Poll)
            .count()
    );
    assert_eq!(
        rep_stats.raw_phis, 2,
        "Range-before-Reps retains the actual source web"
    );
    let _ = licm_stats; // Guard/immutable support must not be claimed from this case.
}

#[test]
fn opt_range_native_cold_overflow_after_store_and_gc_keeps_full_frame() {
    let _settings = draft_o35_settings();
    let mut ctx = Context::new();
    let payload = Value::cons(Value::symbol("heap-companion"), Value::NIL);
    let _roots = Roots::new(&[payload]);
    for rep in [ir::Rep::TaggedFix, ir::Rep::RawInt] {
        for (op, input, step) in [
            (Op::Add, Value::MOST_POSITIVE_FIXNUM, 1),
            (Op::Sub, Value::MOST_NEGATIVE_FIXNUM, 1),
            (Op::Mul, Value::MOST_POSITIVE_FIXNUM, 2),
            (Op::Mul, Value::MOST_NEGATIVE_FIXNUM, -1),
        ] {
            let source = after_store_source(op, step, false, true);
            // Build the original source before manual representation exposure.
            // Existing lift/fold projections already encode their representation;
            // replacing their producer without boundary repair is not a valid fixture.
            let mut before = plan(&source);
            draft_o35_explicit_rep(&mut before, rep);
            let baseline = lower(&source, &before);
            let args = [Value::fixnum(input), Value::NIL, payload];
            ctx.obarray
                .set_symbol_value("t34-o33-effects", Value::fixnum(0));
            let expected = tier0(&mut ctx, &source, &args).unwrap();
            let _expected = Roots::new(&[expected]);
            let expected_effect = ctx.obarray.symbol_value_copied("t34-o33-effects").unwrap();
            ctx.obarray
                .set_symbol_value("t34-o33-effects", Value::fixnum(0));
            let NativeRun::DeoptAt(base_exit) =
                baseline.call(&mut ctx as *mut Context as *mut u8, &args)
            else {
                panic!("actual baseline promotion exit occurs after store/GC")
            };
            assert_eq!(
                print_value(&resumed(&mut ctx, &source, &base_exit).unwrap()),
                print_value(&expected)
            );
            let mut after = before.clone();
            let stats = range::run(&mut after).unwrap();
            let candidate = lower(&source, &after);
            ctx.obarray
                .set_symbol_value("t34-o33-effects", Value::fixnum(0));
            let collections = ctx.tagged_heap.gc_collections();
            let eval::Outcome::Deopt(snapshot) = draft_o35_reference(&mut ctx, &after, &args)
            else {
                panic!("reference retains the last overflow guard")
            };
            ctx.obarray
                .set_symbol_value("t34-o33-effects", Value::fixnum(0));
            let NativeRun::DeoptAt(exit) =
                candidate.call(&mut ctx as *mut Context as *mut u8, &args)
            else {
                panic!("later overflow remains checked")
            };
            assert!(ctx.tagged_heap.gc_collections() > collections);
            assert_eq!(exit.pc, base_exit.pc);
            assert_eq!(exit.pc as u32, snapshot.pc);
            assert_eq!(exit.stack, base_exit.stack);
            assert_eq!(
                exit.stack
                    .iter()
                    .copied()
                    .map(ir::ValueBits::from_value)
                    .collect::<Vec<_>>(),
                snapshot.stack
            );
            assert_eq!(
                ctx.obarray.symbol_value_copied("t34-o33-effects"),
                Some(expected_effect)
            );
            assert_eq!(
                print_value(&resumed(&mut ctx, &source, &exit).unwrap()),
                print_value(&expected)
            );
            assert_eq!(
                ctx.obarray.symbol_value_copied("t34-o33-effects"),
                Some(expected_effect),
                "cold replay does not repeat the store"
            );
            assert_eq!(stats.overflow_checks_elided, 1, "only initial +0 is proved");
            assert_eq!(draft_o35_census_ops(&after, true), 1);
            assert_eq!(ctx.jit_root_stack_top, 0);
        }
    }
}

#[test]
fn opt_range_native_force_deopt_replays_remaining_type_guard() {
    let _settings = draft_o35_settings();
    let mut ctx = Context::new();
    let source = function(
        vec![Op::StackRef(0), Op::Constant(0), Op::Add, Op::Return],
        vec![Value::fixnum(0)],
        1,
    );
    let before = draft_o35_prepared(&source);
    let baseline = lower(&source, &before);
    for value in [Value::MOST_NEGATIVE_FIXNUM, 0, Value::MOST_POSITIVE_FIXNUM] {
        draft_o35_check_ok(
            &mut ctx,
            &source,
            &before,
            &baseline,
            &[Value::fixnum(value)],
        );
    }
    let mut after = before.clone();
    let stats = range::run(&mut after).unwrap();
    let candidate = lower(&source, &after);
    for value in [Value::MOST_NEGATIVE_FIXNUM, 0, Value::MOST_POSITIVE_FIXNUM] {
        draft_o35_check_ok(
            &mut ctx,
            &source,
            &after,
            &candidate,
            &[Value::fixnum(value)],
        );
    }
    assert_eq!(stats.overflow_checks_elided, 1);
    force_deopt_for_test(true);
    let forced = lower(&source, &after);
    force_deopt_for_test(false);
    for value in [Value::MOST_NEGATIVE_FIXNUM, 0, Value::MOST_POSITIVE_FIXNUM] {
        let args = [Value::fixnum(value)];
        let NativeRun::DeoptAt(exit) = forced.call(&mut ctx as *mut Context as *mut u8, &args)
        else {
            panic!("forced remaining real type guard must recover")
        };
        assert_eq!(exit.pc, 2);
        let eval::Outcome::Returned(_) = draft_o35_reference(&mut ctx, &after, &args) else {
            panic!("reference successful operation still returns")
        };
        assert_eq!(
            resumed(&mut ctx, &source, &exit).unwrap(),
            tier0(&mut ctx, &source, &args).unwrap()
        );
        assert_eq!(ctx.jit_root_stack_top, 0);
    }
}

// Separate LICM and array fixtures require the exact proposed adapters. Do not
// replace missing APIs with failing stubs to raise RED counts. Their bounded
// contracts are documented in t34-o35-native-integration-prep.md; source drafts
// will use the real released helper signatures rather than guessed functions.

/// Bytecode-local stack bookkeeping for the exact frozen late-store flow.
struct DraftColdLoopProgram {
    ops: Vec<Op>,
    depth: usize,
}
impl DraftColdLoopProgram {
    fn new() -> Self {
        Self {
            ops: Vec::new(),
            depth: 6,
        }
    }
    fn op(&mut self, op: Op) -> usize {
        let pc = self.ops.len();
        match op {
            Op::StackRef(_) | Op::Constant(_) => self.depth += 1,
            Op::GotoIfNil(_) | Op::VarSet(_) | Op::StackSet(_) => self.depth -= 1,
            Op::Lss | Op::Eqlsign | Op::Add => self.depth -= 1,
            Op::List(n) => self.depth = self.depth - n as usize + 1,
            Op::Add1 | Op::Goto(_) | Op::Return => {}
            _ => panic!("bounded cold-loop fixture must declare stack effect"),
        }
        self.ops.push(op);
        pc
    }
    fn copy(&mut self, slot: usize) {
        self.op(Op::StackRef((self.depth - 1 - slot).try_into().unwrap()));
    }
}

fn draft_o35_cold_loop_source() -> (ByteCodeFunction, usize) {
    // Frozen t34-o335-cold-loop(n,seed,step,fail-at,bad,payload), with i/acc
    // locals. Exact visible store precedes choosing the later invalid operand.
    let mut p = DraftColdLoopProgram::new();
    p.op(Op::Constant(0)); // i slot6
    p.copy(1); // acc slot7
    let header = p.ops.len() as u32;
    p.copy(6);
    p.copy(0);
    p.op(Op::Lss);
    let done = p.op(Op::GotoIfNil(0));
    p.op(Op::Constant(1));
    p.copy(6);
    p.copy(7);
    p.copy(5);
    p.op(Op::List(4));
    p.op(Op::VarSet(2));
    p.copy(7); // accumulator operand lives while choosing the step
    p.copy(6);
    p.copy(3);
    p.op(Op::Eqlsign);
    let ordinary = p.op(Op::GotoIfNil(0));
    p.copy(4);
    let selected = p.op(Op::Goto(0));
    let ordinary_pc = p.ops.len() as u32;
    p.ops[ordinary] = Op::GotoIfNil(ordinary_pc);
    p.depth = 9; // original locals plus accumulator at both branch entries
    p.copy(2);
    let add_pc = p.ops.len();
    p.ops[selected] = Op::Goto(add_pc as u32);
    p.op(Op::Add);
    p.op(Op::StackSet(1)); // acc at7
    p.copy(6);
    p.op(Op::Add1);
    p.op(Op::StackSet(2)); // i at6
    p.op(Op::Goto(header));
    let exit = p.ops.len() as u32;
    p.ops[done] = Op::GotoIfNil(exit);
    p.depth = 8;
    p.copy(6);
    p.copy(7);
    p.copy(5);
    p.op(Op::List(3));
    p.op(Op::Return);
    (
        function(
            p.ops,
            vec![
                Value::fixnum(0),
                Value::symbol("loop-stored"),
                Value::symbol("t34-o335-side"),
            ],
            6,
        ),
        add_pc,
    )
}

#[test]
fn opt_licm_native_body_guards_preserve_zero_trip_and_late_effect() {
    let _settings = draft_o35_settings();
    let mut ctx = Context::new();
    let payload = Value::list(vec![Value::symbol("heap-companion")]);
    let _roots = Roots::new(&[payload]);
    let (source, add_pc) = draft_o35_cold_loop_source();
    let before = draft_o35_prepared(&source);
    let baseline = lower(&source, &before);
    for args in [
        vec![
            Value::fixnum(0),
            Value::symbol("wrong"),
            Value::symbol("wrong"),
            Value::fixnum(0),
            Value::symbol("wrong"),
            payload,
        ],
        vec![
            Value::fixnum(3),
            Value::fixnum(0),
            Value::fixnum(1),
            Value::fixnum(-1),
            Value::fixnum(1),
            payload,
        ],
    ] {
        ctx.obarray.set_symbol_value("t34-o335-side", Value::NIL);
        draft_o35_check_ok(&mut ctx, &source, &before, &baseline, &args);
    }
    let args = [
        Value::fixnum(4),
        Value::fixnum(0),
        Value::fixnum(1),
        Value::fixnum(2),
        Value::symbol("wrong"),
        payload,
    ];
    let _args = Roots::new(&args);
    ctx.obarray.set_symbol_value("t34-o335-side", Value::NIL);
    let expected = tier0(&mut ctx, &source, &args).unwrap_err();
    let expected_signal = signal_summary(&expected);
    let expected_side = ctx.obarray.symbol_value_copied("t34-o335-side").unwrap();
    let _side = Roots::new(&[expected_side]);
    ctx.obarray.set_symbol_value("t34-o335-side", Value::NIL);
    let NativeRun::DeoptAt(base_exit) = baseline.call(&mut ctx as *mut Context as *mut u8, &args)
    else {
        panic!("baseline original late type guard must exit after store")
    };
    assert_eq!(base_exit.pc, add_pc);
    assert_eq!(
        signal_summary(&resumed(&mut ctx, &source, &base_exit).unwrap_err()),
        expected_signal
    );
    let mut after = before.clone();
    range::run(&mut after).unwrap();
    licm::run(&mut after).unwrap();
    reps::run(&mut after).unwrap();
    let candidate = lower(&source, &after);
    let zero = [
        Value::fixnum(0),
        Value::symbol("wrong"),
        Value::symbol("wrong"),
        Value::fixnum(0),
        Value::symbol("wrong"),
        payload,
    ];
    ctx.obarray.set_symbol_value("t34-o335-side", Value::NIL);
    draft_o35_check_ok(&mut ctx, &source, &after, &candidate, &zero);
    assert_eq!(
        ctx.obarray.symbol_value_copied("t34-o335-side"),
        Some(Value::NIL),
        "zero trip never executes body guard/store"
    );
    ctx.obarray.set_symbol_value("t34-o335-side", Value::NIL);
    let eval::Outcome::Deopt(snapshot) = draft_o35_reference(&mut ctx, &after, &args) else {
        panic!("reference retains original body-only type guard")
    };
    ctx.obarray.set_symbol_value("t34-o335-side", Value::NIL);
    let NativeRun::DeoptAt(exit) = candidate.call(&mut ctx as *mut Context as *mut u8, &args)
    else {
        panic!("candidate cannot execute the invalid body guard early")
    };
    assert_eq!(exit.pc, base_exit.pc);
    assert_eq!(exit.stack, base_exit.stack);
    assert_eq!(
        exit.stack
            .iter()
            .copied()
            .map(ir::ValueBits::from_value)
            .collect::<Vec<_>>(),
        snapshot.stack
    );
    assert_eq!(
        print_value(&ctx.obarray.symbol_value_copied("t34-o335-side").unwrap()),
        print_value(&expected_side)
    );
    assert_eq!(
        signal_summary(&resumed(&mut ctx, &source, &exit).unwrap_err()),
        expected_signal
    );
    assert_eq!(ctx.jit_root_stack_top, 0);
}
