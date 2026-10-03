//! Source-native array proof and bounds-elimination parity.
//! Actual source Arefs run through Tier0/reference/native baseline before any
//! lift/elimination assertion. GNU vector/record/index/type/bounds/error-after-
//! store semantics are frozen67; mutation uses real Aset and bytecode Call.
//! Context, roots, plans, proofs and override settings belong to this mutator.

use crate::emacs_core::jit::compile::opt_census::SelectedTier;

use super::compile_pipeline_tests::function;
use super::*;
use crate::emacs_core::error::Flow;
use crate::emacs_core::eval::{
    push_scratch_gc_roots, restore_scratch_gc_roots, save_scratch_gc_roots,
};
use crate::emacs_core::jit::opt::passes::{array_reads, range};
use crate::emacs_core::jit::opt::{
    build, eval, ir, mem,
    types::{Range, TypeSet},
};
use crate::emacs_core::jit::tier2::CompileTier;
use crate::emacs_core::print::print_value;
use array_reads::{ArrayReadProofs, BoundsWitness};

struct Settings;
impl Settings {
    fn enter() -> Self {
        force_opt_for_test(Some(OptMode::Opt), Some(OptAdmit::ALL));
        force_opt_passes_for_test(Some(OptPasses {
            range: true,
            ..OptPasses::default()
        }));
        force_tier2_for_test(Some(Tier2Knob {
            on: true,
            window: 1000,
            loop_credit: 64,
        }));
        crate::emacs_core::jit::feedback::force_feedback_mode_for_test(Some(
            crate::emacs_core::jit::feedback::FeedbackMode::Off,
        ));
        crate::emacs_core::jit::inline::force_inline_for_test(Some(false));
        force_deopt_for_test(false);
        Self
    }
}
impl Drop for Settings {
    fn drop(&mut self) {
        force_opt_for_test(None, None);
        force_opt_passes_for_test(None);
        force_tier2_for_test(None);
        crate::emacs_core::jit::feedback::force_feedback_mode_for_test(None);
        crate::emacs_core::jit::inline::force_inline_for_test(None);
        force_deopt_for_test(false);
    }
}
struct Roots(usize);
impl Roots {
    fn new(v: &[Value]) -> Self {
        let n = save_scratch_gc_roots();
        push_scratch_gc_roots(v);
        Self(n)
    }
}
impl Drop for Roots {
    fn drop(&mut self) {
        restore_scratch_gc_roots(self.0);
    }
}

fn plan(f: &ByteCodeFunction) -> ir::Func {
    plan_prefix(f, 0)
}
fn plan_prefix(f: &ByteCodeFunction, prefix: usize) -> ir::Func {
    let arity = f.params.required.len();
    let cfg = analyze_cfg(
        f.executable_ops(),
        &f.constants,
        f.executable_gnu_byte_offset_map(),
        arity,
    )
    .unwrap();
    let constants = f
        .constants
        .iter()
        .copied()
        .map(ir::ValueBits::from_value)
        .collect::<Vec<_>>();
    build::build(build::BuildInput {
        ops: f.executable_ops(),
        constants: &constants,
        cfg: &cfg,
        params: ir::ParamShape {
            required: arity,
            ..Default::default()
        },
        dynamic_prefix: prefix,
        fused: None,
        osr: None,
    })
    .unwrap()
}
fn lower(f: &ByteCodeFunction, ir: &ir::Func) -> CompiledLeaf {
    ir.verify().unwrap();
    let leaf = lower_opt_ir_for_test(
        f.executable_ops(),
        &f.constants,
        f.params.required.len(),
        f.executable_gnu_byte_offset_map(),
        ir,
    )
    .expect("actual verified array macro lowers natively");
    assert_eq!(leaf.selected_tier(), SelectedTier::Opt);
    leaf
}
fn tier0(ctx: &mut Context, f: &ByteCodeFunction, args: &[Value]) -> Result<Value, Flow> {
    let mut vm = Vm::from_context(ctx);
    vm.force_interpreter_only_for_test();
    vm.execute(f, args.to_vec())
}
fn native(ctx: &mut Context, f: &ByteCodeFunction, leaf: &CompiledLeaf, args: &[Value]) -> Value {
    let NativeRun::Ok(bits) =
        leaf.call_consts(ctx as *mut Context as *mut u8, f.constants.as_ptr(), args)
    else {
        panic!("successful source array case must finish NativeRun::Ok")
    };
    Value::from_bits(bits)
}
fn parity(
    ctx: &mut Context,
    f: &ByteCodeFunction,
    ir: &ir::Func,
    leaf: &CompiledLeaf,
    args: &[Value],
) {
    let expected = tier0(ctx, f, args).unwrap();
    let _root = Roots::new(&[expected]);
    let run = eval::evaluate(
        ir,
        ctx,
        eval::Inputs {
            args,
            prefix: &f.constants[..ir.dynamic_prefix],
            ..Default::default()
        },
    )
    .unwrap();
    let eval::Outcome::Returned(value) = run.outcome else {
        panic!("reference array success")
    };
    assert_eq!(print_value(&value.to_value()), print_value(&expected));
    assert_eq!(
        print_value(&native(ctx, f, leaf, args)),
        print_value(&expected)
    );
    assert_eq!(ctx.jit_root_stack_top, 0);
}
fn observe_t1(ctx: &mut Context, f: &ByteCodeFunction, args: &[Value]) {
    let leaf = compile_bytecode_function_requested(
        f,
        Some(&ctx.obarray),
        CompileRequest {
            tier: CompileTier::T1,
            regalloc: lowering::RegallocPolicy::Auto,
            bypass_profit_gate: true,
            origin: crate::emacs_core::jit::stats::CompileOrigin::Direct,
        },
    )
    .unwrap();
    assert_eq!(leaf.selected_tier(), SelectedTier::Baseline);
    let expected = tier0(ctx, f, args).unwrap();
    let _root = Roots::new(&[expected]);
    assert_eq!(
        print_value(&native(ctx, f, &leaf, args)),
        print_value(&expected)
    );
    assert!(
        f.jit_runtime().array_sites().is_some(),
        "actual emitted T1 supplied dynamic kind evidence"
    );
}
fn candidate(
    f: &ByteCodeFunction,
    before: &ir::Func,
) -> (
    ir::Func,
    ArrayReadProofs,
    array_reads::ArrayLiftStats,
    range::RangeStats,
) {
    // Taking a snapshot captures the masks recorded by the native T1; workers
    // consume copied scalars, never reclassify arbitrary runtime Arg contents.
    let _scope = super::snapshot::publish_numeric_feedback_with_arrays(f);
    let hints = super::array_snapshot::admission(
        f.executable_ops().len(),
        &f.constants,
        before.dynamic_prefix,
    );
    let mut after = before.clone();
    let mut proofs = ArrayReadProofs::default();
    let lift = array_reads::lift(&mut after, &mut proofs, &hints).unwrap();
    let stats = range::run_with_array_proofs(&mut after, &mut proofs).unwrap();
    array_reads::verify_reads(&after, &proofs).unwrap();
    // ROOT adds this compiler-owned field and atomic sidecar remapping first.
    after.array_reads = proofs.clone();
    after.verify().unwrap();
    (after, proofs, lift, stats)
}
fn observations(before: &ir::Func, after: &ir::Func) {
    assert_eq!(after.frames, before.frames);
    assert_eq!(after.entry_stacks, before.entry_stacks);
    assert_eq!(after.source_states.len(), before.source_states.len());
    for (a, b) in after.source_states.iter().zip(&before.source_states) {
        match (a, b) {
            (Some(a), Some(b)) => {
                assert_eq!(a.pre, b.pre);
                assert_eq!(a.post, b.post);
                assert_eq!(a.frame, b.frame);
                assert_eq!(a.block, b.block);
            }
            (None, None) => {}
            _ => panic!("source state retained"),
        }
    }
}
fn signal_text(flow: &Flow) -> (String, String) {
    let signal = flow.as_signal().unwrap();
    (
        signal.symbol_name().to_owned(),
        print_value(&Value::list(signal.data.clone())),
    )
}
fn resumed(ctx: &mut Context, f: &ByteCodeFunction, exit: &DeoptResume) -> Result<Value, Flow> {
    let mut vm = Vm::from_context(ctx);
    vm.force_interpreter_only_for_test();
    vm.run_resumed_frame(
        f,
        Value::NIL,
        exit.pc,
        &exit.stack,
        exit.handlers,
        &exit.binds,
        exit.spec_base,
        exit.cond_base,
    )
}

#[test]
fn opt_range_native_actual_vector_record_arefs_elide_current_bounds_and_keep_errors() {
    let _settings = Settings::enter();
    let mut ctx = Context::new();
    let vector = Value::vector(vec![Value::fixnum(13), Value::fixnum(23)]);
    let record = Value::make_record(vec![Value::symbol("record-tag"), Value::fixnum(29)]);
    let _roots = Roots::new(&[vector, record]);
    // Exact source operations with one retained SSA array owner:
    // (list (aref ARRAY 1) (aref ARRAY 0)). No known length is fabricated.
    for array in [vector, record] {
        let f = function(
            vec![
                Op::Constant(0),
                Op::Dup,
                Op::Constant(1),
                Op::Aref,
                Op::StackRef(1),
                Op::Constant(2),
                Op::Aref,
                Op::List(2),
                Op::Return,
            ],
            vec![array, Value::fixnum(1), Value::fixnum(0)],
            0,
        );
        let before = plan(&f);
        let baseline = lower(&f, &before);
        parity(&mut ctx, &f, &before, &baseline, &[]); // native baseline FIRST
        let (after, proofs, lift, stats) = candidate(&f, &before);
        let optimized = lower(&f, &after);
        parity(&mut ctx, &f, &after, &optimized, &[]);
        observations(&before, &after);
        assert_eq!(lift.reads_lifted, 2);
        assert_eq!(stats.bounds_checks_elided, 1);
        assert_eq!(
            proofs
                .reads
                .values()
                .filter(|p| matches!(p.witness, BoundsWitness::Checked))
                .count(),
            1
        );
        for (&id, p) in &proofs.reads {
            assert_eq!(after.values[p.length.index()].rep, ir::Rep::RawInt);
            assert_eq!(
                after.values[p.length.index()].ty,
                TypeSet::fixnum_range(Range {
                    lo: 0,
                    hi: Range::FULL.hi
                })
            );
            let read = &after.insts[id.index()];
            assert_eq!(read.op, ir::Opcode::Opaque(Op::Aref)); // original result/id/pc preserved
            assert_eq!(read.result, before.insts[id.index()].result);
            assert_eq!(read.frame, before.insts[id.index()].frame);
            assert_eq!(read.pc, before.insts[id.index()].pc);
        }
    }
    // Frozen index-wrong-before-array / array-wrong-before-bounds / negative /
    // past-end / error-after-visible-store. Speculation exits are full preop
    // frames; resume derives signal/data from Tier0, not a new error helper.
    let payload = Value::list(vec![Value::symbol("heap-companion")]);
    let wrong = Value::symbol("wrong");
    let text = Value::string("x");
    let _roots = Roots::new(&[payload, wrong, text]);
    let f = function(
        vec![
            Op::StackRef(0),
            Op::VarSet(0),
            Op::StackRef(2),
            Op::StackRef(2),
            Op::Aref,
            Op::Return,
        ],
        vec![Value::symbol("t34-o335-side")],
        3,
    );
    ctx.obarray.set_symbol_value("t34-o335-side", Value::NIL);
    let before = plan(&f);
    let baseline = lower(&f, &before);
    parity(
        &mut ctx,
        &f,
        &before,
        &baseline,
        &[vector, Value::fixnum(0), payload],
    );
    observe_t1(&mut ctx, &f, &[vector, Value::fixnum(0), payload]);
    let (after, _, lift, _) = candidate(&f, &before);
    let optimized = lower(&f, &after);
    parity(
        &mut ctx,
        &f,
        &after,
        &optimized,
        &[vector, Value::fixnum(0), payload],
    );
    for (array, index) in [
        (wrong, wrong),
        (wrong, Value::fixnum(-1)),
        (vector, wrong),
        (vector, Value::fixnum(-1)),
        (vector, Value::fixnum(2)),
        (text, Value::fixnum(0)),
    ] {
        let args = [array, index, payload];
        ctx.obarray.set_symbol_value("t34-o335-side", Value::NIL);
        let expected = tier0(&mut ctx, &f, &args);
        let expected_side = ctx.obarray.symbol_value("t34-o335-side").copied().unwrap();
        ctx.obarray.set_symbol_value("t34-o335-side", Value::NIL);
        // Original native errors are checked independently, including existing
        // baseline signal protocol; frame omissions are kept in frozen67 report.
        match baseline.call(&mut ctx as *mut Context as *mut u8, &args) {
            NativeRun::Signal => {
                let flow = super::shims::take_pending_flow().unwrap();
                assert_eq!(
                    signal_text(&flow),
                    signal_text(expected.as_ref().unwrap_err())
                );
            }
            NativeRun::Ok(bits) => assert_eq!(
                print_value(&Value::from_bits(bits)),
                print_value(expected.as_ref().unwrap())
            ),
            other => panic!("baseline Aref outcome must be its ordinary success/signal: {other:?}"),
        }
        ctx.obarray.set_symbol_value("t34-o335-side", Value::NIL);
        let NativeRun::DeoptAt(exit) = optimized.call(&mut ctx as *mut Context as *mut u8, &args)
        else {
            panic!("wrong shape/index/bounds leaves through original Aref snapshot")
        };
        assert_eq!(exit.pc, 4);
        assert_eq!(exit.stack, vec![array, index, payload, array, index]);
        assert_eq!(
            ctx.obarray.symbol_value("t34-o335-side").copied(),
            Some(expected_side)
        );
        let actual = resumed(&mut ctx, &f, &exit);
        match (actual, expected) {
            (Ok(a), Ok(b)) => assert_eq!(print_value(&a), print_value(&b)),
            (Err(a), Err(b)) => assert_eq!(signal_text(&a), signal_text(&b)),
            _ => panic!("GNU signal/success preserved"),
        }
    }
    observations(&before, &after);
    assert_eq!(lift.reads_lifted, 1);
}

#[derive(Clone, Copy)]
enum Mutation {
    None,
    Aset,
    Call,
    Join,
}
struct Program {
    ops: Vec<Op>,
    depth: usize,
}
impl Program {
    fn new() -> Self {
        Self {
            ops: vec![],
            depth: 3,
        }
    }
    fn op(&mut self, op: Op) -> usize {
        let pc = self.ops.len();
        match op {
            Op::Constant(_) | Op::StackRef(_) | Op::Nil => self.depth += 1,
            Op::Aref | Op::List(2) | Op::Pop | Op::GotoIfNil(_) => self.depth -= 1,
            Op::Aset | Op::Call(2) => self.depth -= 2,
            Op::Goto(_) | Op::Return => {}
            _ => panic!("explicit stack effect"),
        }
        self.ops.push(op);
        pc
    }
    fn copy(&mut self, bottom: usize) {
        self.op(Op::StackRef((self.depth - 1 - bottom) as u16));
    }
    fn read(&mut self, index: u16) {
        self.copy(0);
        self.op(Op::Constant(index));
        self.op(Op::Aref);
    }
    fn set(&mut self, call: bool) {
        if call {
            self.op(Op::Constant(2));
            self.copy(0);
            self.copy(1);
            self.op(Op::Call(2));
        } else {
            self.copy(0);
            self.op(Op::Constant(1));
            self.copy(1);
            self.op(Op::Aset);
        }
        self.op(Op::Pop);
    }
    fn finish(mut self, kind: Mutation, callback: Value) -> ByteCodeFunction {
        self.read(0);
        match kind {
            Mutation::None => {}
            Mutation::Aset => self.set(false),
            Mutation::Call => self.set(true),
            Mutation::Join => {
                self.copy(2);
                let branch = self.op(Op::GotoIfNil(0));
                self.set(false);
                let go = self.op(Op::Goto(0));
                let skip = self.ops.len();
                self.op(Op::Nil);
                self.op(Op::Pop);
                let join = self.ops.len();
                self.ops[branch] = Op::GotoIfNil(skip as u32);
                self.ops[go] = Op::Goto(join as u32);
            }
        }
        self.read(1);
        self.op(Op::List(2));
        self.op(Op::Return);
        function(
            self.ops,
            vec![Value::fixnum(1), Value::fixnum(0), callback],
            3,
        )
    }
}

#[test]
fn opt_range_native_array_alias_call_and_join_keep_current_owner_checks() {
    let _settings = Settings::enter();
    let mut ctx = Context::new();
    // Real bytecode callback mutates alias slot0; no invented resizing API.
    let callback = Value::make_bytecode(function(
        vec![
            Op::StackRef(1),
            Op::Constant(0),
            Op::StackRef(2),
            Op::Aset,
            Op::Return,
        ],
        vec![Value::fixnum(0)],
        2,
    ));
    let new = Value::list(vec![Value::symbol("new-child")]);
    let _roots = Roots::new(&[callback, new]);
    for kind in [
        Mutation::None,
        Mutation::Aset,
        Mutation::Call,
        Mutation::Join,
    ] {
        let f = Program::new().finish(kind, callback);
        let before = plan(&f);
        let baseline = lower(&f, &before);
        for selector in [Value::NIL, Value::T] {
            let array = Value::vector(vec![Value::fixnum(13), Value::fixnum(23)]);
            let _root = Roots::new(&[array]);
            let args = [array, new, selector];
            let expected = print_value(&tier0(&mut ctx, &f, &args).unwrap());
            for mode in 0..2 {
                // reference and native need fresh mutated objects
                let array = Value::vector(vec![Value::fixnum(13), Value::fixnum(23)]);
                let _root = Roots::new(&[array]);
                let args = [array, new, selector];
                let actual = if mode == 0 {
                    let run = eval::evaluate(
                        &before,
                        &mut ctx,
                        eval::Inputs {
                            args: &args,
                            ..Default::default()
                        },
                    )
                    .unwrap();
                    let eval::Outcome::Returned(v) = run.outcome else {
                        panic!("baseline reference success")
                    };
                    v.to_value()
                } else {
                    native(&mut ctx, &f, &baseline, &args)
                };
                assert_eq!(print_value(&actual), expected);
            }
        }
        let observed = Value::vector(vec![Value::fixnum(13), Value::fixnum(23)]);
        let _root = Roots::new(&[observed]);
        observe_t1(&mut ctx, &f, &[observed, new, Value::T]);
        let (after, proofs, lift, stats) = candidate(&f, &before);
        let optimized = lower(&f, &after);
        for selector in [Value::NIL, Value::T] {
            let array = Value::vector(vec![Value::fixnum(13), Value::fixnum(23)]);
            let _root = Roots::new(&[array]);
            let expected = print_value(&tier0(&mut ctx, &f, &[array, new, selector]).unwrap());
            let array = Value::vector(vec![Value::fixnum(13), Value::fixnum(23)]);
            let _root = Roots::new(&[array]);
            let args = [array, new, selector];
            let run = eval::evaluate(
                &after,
                &mut ctx,
                eval::Inputs {
                    args: &args,
                    ..Default::default()
                },
            )
            .unwrap();
            let eval::Outcome::Returned(value) = run.outcome else {
                panic!("candidate reference array success")
            };
            assert_eq!(print_value(&value.to_value()), expected);
            let array = Value::vector(vec![Value::fixnum(13), Value::fixnum(23)]);
            let _root = Roots::new(&[array]);
            assert_eq!(
                print_value(&native(&mut ctx, &f, &optimized, &[array, new, selector])),
                expected
            );
        }
        observations(&before, &after);
        assert_eq!(lift.reads_lifted, 2);
        assert_eq!(
            stats.bounds_checks_elided,
            if matches!(kind, Mutation::None) { 1 } else { 0 }
        );
        if !matches!(kind, Mutation::None) {
            assert!(
                proofs
                    .reads
                    .values()
                    .all(|p| matches!(p.witness, BoundsWitness::Checked))
            );
        }
        // Alias writes/calls stay actual operations, not 'length immutable'.
        for op in [Op::Aset, Op::Call(2)] {
            assert_eq!(
                before
                    .insts
                    .iter()
                    .filter(|i| i.op == ir::Opcode::Opaque(op.clone()))
                    .count(),
                after
                    .insts
                    .iter()
                    .filter(|i| i.op == ir::Opcode::Opaque(op.clone()))
                    .count()
            );
        }
    }
}

#[test]
fn opt_range_native_unknown_string_and_patched_arrays_decline_speculation() {
    let _settings = Settings::enter();
    let mut ctx = Context::new();
    let vector = Value::vector(vec![Value::fixnum(13)]);
    let string = Value::string("x");
    let _roots = Roots::new(&[vector, string]);
    let unknown = function(
        vec![Op::StackRef(0), Op::Constant(0), Op::Aref, Op::Return],
        vec![Value::fixnum(0)],
        1,
    );
    let text = function(
        vec![Op::Constant(0), Op::Constant(1), Op::Aref, Op::Return],
        vec![string, Value::fixnum(0)],
        0,
    );
    let patched = function(
        vec![Op::Constant(0), Op::Constant(1), Op::Aref, Op::Return],
        vec![vector, Value::fixnum(0)],
        0,
    );
    for (f, prefix, args) in [
        (&unknown, 0, vec![vector]),
        (&unknown, 0, vec![string]),
        (&text, 0, vec![]),
        (&patched, 1, vec![]),
    ] {
        let before = plan_prefix(f, prefix);
        let baseline = lower(f, &before);
        parity(&mut ctx, f, &before, &baseline, &args); // original native path FIRST
        let (after, proofs, lift, stats) = candidate(f, &before);
        let optimized = lower(f, &after);
        parity(&mut ctx, f, &after, &optimized, &args);
        assert_eq!(lift.reads_lifted, 0);
        assert_eq!(stats.bounds_checks_elided, 0);
        assert!(proofs.reads.is_empty());
        observations(&before, &after);
        assert_eq!(
            after.insts.len(),
            before.insts.len(),
            "no unknown/string shape deopt guards appended"
        );
    }
}

// ROOT BEFORE REGISTRATION: Func.array_reads, raw-length+resultful-Bounds
// verifier/native support and original-Opaque proof-aware shared Aref wrapper.
// Replace no production test expectation: all semantic answers derive Tier0.
// Table metadata never changes the reference Opaque Aref result.

#[test]
fn opt_range_native_array_singleton_shape_guard_preserves_exact_word_identity() {
    let _settings = Settings::enter();
    let mut ctx = Context::new();
    let expected_array = Value::vector(vec![Value::fixnum(13)]);
    let distinct_array = Value::vector(vec![Value::fixnum(23)]);
    let _roots = Roots::new(&[expected_array, distinct_array]);
    // Pool slot1 keeps the exact identity witness owned/rooted by the source;
    // the original Aref itself still reads its ordinary runtime argument.
    let source = function(
        vec![Op::StackRef(0), Op::Constant(0), Op::Aref, Op::Return],
        vec![Value::fixnum(0), expected_array],
        1,
    );
    let before = plan(&source);
    let baseline = lower(&source, &before);
    for array in [expected_array, distinct_array] {
        parity(&mut ctx, &source, &before, &baseline, &[array]);
    }
    observe_t1(&mut ctx, &source, &[expected_array]);
    let (mut after, proofs, lift, _) = candidate(&source, &before);
    assert_eq!(lift.reads_lifted, 1);
    let proof = proofs.reads.values().next().unwrap().clone();
    let ir::ValueDef::Inst(shape) = after.values[proof.guarded_base.index()].def else {
        panic!("actual lifted base is a real executed shape guard")
    };
    let exact_type = TypeSet::VECTOR.with_singleton(ir::ValueBits::from_value(expected_array));
    assert_eq!(
        exact_type.singleton(),
        Some(ir::ValueBits::from_value(expected_array))
    );
    after.insts[shape.index()].op = ir::Opcode::CheckType(exact_type);
    after.values[proof.guarded_base.index()].ty = exact_type;
    after.array_reads = proofs.clone();
    after
        .verify()
        .expect("ordinary valid SSA and complete current-owner proof");
    array_reads::verify_reads(&after, &proofs).expect("independent final array capability");
    observations(&before, &after);
    let matching = eval::evaluate(
        &after,
        &mut ctx,
        eval::Inputs {
            args: &[expected_array],
            ..Default::default()
        },
    )
    .unwrap();
    let eval::Outcome::Returned(matching_value) = matching.outcome else {
        panic!("exact singleton identity passes reference CheckType")
    };
    let expected = tier0(&mut ctx, &source, &[expected_array]).unwrap();
    let _root = Roots::new(&[expected]);
    assert_eq!(
        print_value(&matching_value.to_value()),
        print_value(&expected)
    );
    let rejected = eval::evaluate(
        &after,
        &mut ctx,
        eval::Inputs {
            args: &[distinct_array],
            ..Default::default()
        },
    )
    .unwrap();
    let eval::Outcome::Deopt(reference_exit) = rejected.outcome else {
        panic!("different vector must fail exact singleton reference guard")
    };
    assert_eq!(reference_exit.pc, proof.pc);
    // The original StackRef retains its argument below the two Aref operands.
    // Preserve that complete GNU residual stack in both cold exits.
    assert_eq!(
        reference_exit.stack,
        vec![
            ir::ValueBits::from_value(distinct_array),
            ir::ValueBits::from_value(distinct_array),
            ir::ValueBits::from_value(Value::fixnum(0))
        ]
    );
    // Conservative refusal of singleton shape targets is a permitted native
    // policy. Only that explicit policy refusal is accepted; an unrelated
    // missing native opcode/layout/frame failure cannot make this test pass.
    let selected = match lower_opt_ir_for_test(
        source.executable_ops(),
        &source.constants,
        1,
        source.executable_gnu_byte_offset_map(),
        &after,
    ) {
        Ok(leaf) => leaf,
        Err(CompileError::UnsupportedOp("opt-array:shape-singleton")) => return,
        Err(error) => panic!("unexpected native admission failure: {error:?}"),
    };
    assert_eq!(selected.selected_tier(), SelectedTier::Opt);
    assert_eq!(
        print_value(&native(&mut ctx, &source, &selected, &[expected_array])),
        print_value(&expected)
    );
    let NativeRun::DeoptAt(native_exit) = selected.call_consts(
        &mut ctx as *mut Context as *mut u8,
        source.constants.as_ptr(),
        &[distinct_array],
    ) else {
        panic!("native shape guard accepted a different vector under exact singleton TypeSet")
    };
    assert_eq!(native_exit.pc as u32, reference_exit.pc);
    assert_eq!(
        native_exit
            .stack
            .iter()
            .copied()
            .map(ir::ValueBits::from_value)
            .collect::<Vec<_>>(),
        reference_exit.stack
    );
    assert_eq!(
        print_value(&resumed(&mut ctx, &source, &native_exit).unwrap()),
        print_value(&tier0(&mut ctx, &source, &[distinct_array]).unwrap())
    );
    assert_eq!(ctx.jit_root_stack_top, 0);
}
