//! Independent recipe verification fixtures.
//!
//! Every fixture verifies and executes its actual original source through
//! Tier-0/reference before inspecting a selected recipe or poisoning metadata.
//! These are new API tests, first isolated by the lead; no synthetic acceptance
//! or unsupported implementation is supplied to manufacture a regression.
//!
//! Threading: source constants, scratch roots, Context and candidate metadata
//! belong to one test invocation/mutator. No TLS/cache/shared state is added.

use crate::emacs_core::bytecode::{ByteCodeFunction, Op, Vm};
use crate::emacs_core::eval::{
    Context, push_scratch_gc_roots, restore_scratch_gc_roots, save_scratch_gc_roots,
};
use crate::emacs_core::jit::opt::{
    build::{BuildInput, build},
    eval::{Inputs, Outcome, evaluate},
    ir::*,
    mem::{AliasClass, Effects},
    types::TypeSet,
};
use crate::emacs_core::value::{LambdaParams, Value as LispValue};

use crate::emacs_core::jit::opt::sink_recipes::{
    self, RecipeFields, RecipeOrigin, RecipePoint, RecipeVersionId, SinkOp, SinkVerifyReason,
    VersionCause,
};

struct Roots(usize);
impl Roots {
    fn new(values: &[LispValue]) -> Self {
        let roots = Self(save_scratch_gc_roots());
        push_scratch_gc_roots(values);
        roots
    }
    fn add(&self, values: &[LispValue]) {
        push_scratch_gc_roots(values);
    }
}
impl Drop for Roots {
    fn drop(&mut self) {
        restore_scratch_gc_roots(self.0);
    }
}

fn source(ops: Vec<Op>, constants: Vec<LispValue>, arity: usize) -> ByteCodeFunction {
    let expected = ops.clone();
    let mut source = ByteCodeFunction::new(LambdaParams {
        required: (0..arity)
            .map(|_| crate::emacs_core::intern::intern("opt-sink-recipe-arg"))
            .collect(),
        optional: Vec::new(),
        rest: None,
    });
    source.lexical = true;
    source.max_stack = crate::emacs_core::bytecode::StackDepth::for_test(32);
    source.ops = ops;
    source.constants = constants.into();
    source.seal_hand_assembled_ops_for_test();
    assert_eq!(
        source.executable_ops(),
        expected.as_slice(),
        "sealing must preserve fixture operations"
    );
    source
}

fn frozen_loop_add() -> ByteCodeFunction {
    // Exact GNU31.1 body, descriptor and pool from the immutable loop pack:
    // t34-o36-loop-add(seed, step, n), not an invented oracle program.
    let mut source = ByteCodeFunction::new(LambdaParams {
        required: (0..3)
            .map(|_| crate::emacs_core::intern::intern("opt-sink-loop-arg"))
            .collect(),
        optional: Vec::new(),
        rest: None,
    });
    source.lexical = true;
    source.arglist = LispValue::fixnum(771);
    source.max_stack = crate::emacs_core::bytecode::StackDepth::for_test(8);
    source.constants = vec![LispValue::fixnum(0)].into();
    source.gnu_bytecode_bytes = Some(crate::tagged::header::LispByteVec::copy_from_slice(&[
        2, 137, 192, 137, 4, 87, 131, 21, 0, 2, 5, 92, 137, 178, 4, 178, 2, 84, 130, 3, 0, 2, 2,
        66, 135,
    ]));
    source
        .restore_gnu_decode_policy()
        .expect("exact frozen GNU bytecode decode");
    assert!(!source.executable_ops().is_empty());
    source
}

fn plan(source: &ByteCodeFunction) -> Func {
    let params = ParamShape {
        required: source
            .params
            .stack_shape()
            .expect("fixture stack parameters")
            .required(),
        ..ParamShape::default()
    };
    let cfg = crate::emacs_core::jit::compile::analyze_cfg(
        source.executable_ops(),
        &source.constants,
        source.executable_gnu_byte_offset_map(),
        params.native_arity(),
    )
    .expect("actual source CFG must be valid before selection");
    let constants = source
        .constants
        .iter()
        .copied()
        .map(ValueBits::from_value)
        .collect::<Vec<_>>();
    let func = build(BuildInput {
        ops: source.executable_ops(),
        constants: &constants,
        cfg: &cfg,
        params,
        dynamic_prefix: 0,
        fused: None,
        osr: None,
    })
    .expect("actual builder SSA before recipe changes");
    func.verify()
        .expect("ordinary builder input must verify before selection");
    func
}

fn tier0(ctx: &mut Context, source: &ByteCodeFunction, args: &[LispValue]) -> LispValue {
    let _roots = Roots::new(&source.constants);
    let mut vm = Vm::from_context(ctx);
    vm.force_interpreter_only_for_test();
    vm.execute(source, args.to_vec())
        .expect("actual independent Tier-0 baseline")
}
fn reference(ctx: &mut Context, func: &Func, args: &[LispValue]) -> LispValue {
    let run = evaluate(
        func,
        ctx,
        Inputs {
            args,
            ..Inputs::default()
        },
    )
    .expect("actual original-IR reference baseline");
    let Outcome::Returned(value) = run.outcome else {
        panic!("baseline must return before recipe assertions")
    };
    value.to_value()
}
fn opaque_results(func: &Func, op: Op) -> Vec<Value> {
    func.insts
        .iter()
        .filter_map(|inst| {
            matches!(&inst.op, Opcode::Opaque(actual) if actual == &op)
                .then_some(inst.result)
                .flatten()
        })
        .collect()
}
fn arg(func: &Func, wanted: u16) -> Value {
    func.insts
        .iter()
        .find_map(|inst| {
            matches!(inst.op, Opcode::Arg(actual) if actual == wanted)
                .then_some(inst.result)
                .flatten()
        })
        .expect("original Arg identity")
}
fn observation_ids(original: &Func, selected: &Func) {
    assert!(selected.frames.len() >= original.frames.len());
    assert_eq!(
        &selected.frames[..original.frames.len()],
        original.frames.as_slice()
    );
    assert_eq!(
        &selected.entry_stacks[..original.entry_stacks.len()],
        original.entry_stacks.as_slice()
    );
    assert_eq!(selected.source_states.len(), original.source_states.len());
    for (before, after) in original.source_states.iter().zip(&selected.source_states) {
        match (before, after) {
            (Some(before), Some(after)) => {
                assert_eq!(before.frame, after.frame);
                assert_eq!(before.block, after.block);
                assert_eq!(before.pre, after.pre);
                assert_eq!(before.post, after.post);
            }
            (None, None) => {}
            _ => panic!("original source state changed"),
        }
    }
}

fn fresh_float_pair(ctx: &mut Context, roots: &Roots) -> (ByteCodeFunction, Func, [Value; 2]) {
    // This exact eight-op source is already exercised by the GREEN current
    // opt_gvn_reference_fresh_float_and_dynamic_positioned_eq_are_excluded.
    let constants = [LispValue::make_float(1.25), LispValue::make_float(2.0)];
    roots.add(&constants);
    let source = source(
        vec![
            Op::Constant(0),
            Op::Constant(1),
            Op::Mul,
            Op::Constant(0),
            Op::Constant(1),
            Op::Mul,
            Op::Eq,
            Op::Return,
        ],
        constants.to_vec(),
        0,
    );
    let func = plan(&source);
    assert!(
        tier0(ctx, &source, &[]).is_nil(),
        "GNU frozen fresh-Float identity rule"
    );
    assert!(reference(ctx, &func, &[]).is_nil());
    let owners = opaque_results(&func, Op::Mul);
    assert_eq!(owners.len(), 2);
    assert_ne!(owners[0], owners[1]);
    (source, func, [owners[0], owners[1]])
}

fn insert_number_guard(func: &mut Func, before: Inst, input: Value) -> Inst {
    let old = func.insts[before.index()].clone();
    let frame = old.frame.expect("real original Eq preop frame");
    let block = Block(
        func.blocks
            .iter()
            .position(|block| block.insts.contains(&before))
            .unwrap() as u32,
    );
    let inst = Inst(func.insts.len() as u32);
    let result = Value(func.values.len() as u32);
    func.values.push(ValueData {
        def: ValueDef::Inst(inst),
        rep: Rep::Tagged,
        ty: func.values[input.index()]
            .ty
            .meet(TypeSet::FIXNUM.join(TypeSet::FLOAT)),
    });
    func.insts.push(InstData {
        op: Opcode::CheckType(TypeSet::FIXNUM.join(TypeSet::FLOAT)),
        args: vec![input],
        result: Some(result),
        eff: Effects::MAY_DEOPT,
        mem: AliasClass::None,
        frame: Some(frame),
        pc: old.pc,
    });
    let position = func.blocks[block.index()]
        .insts
        .iter()
        .position(|&item| item == before)
        .unwrap();
    func.blocks[block.index()].insts.insert(position, inst);
    func.verify()
        .expect("two real same-frame guard uses are ordinary valid IR before recipes");
    inst
}

fn partial_alias_source() -> ByteCodeFunction {
    // choose,x,y. True carries one fresh Add result twice. False carries two
    // equal-payload fresh Add results. Two join phis alias on only one edge.
    source(
        vec![
            Op::StackRef(2),
            Op::GotoIfNil(7),
            Op::StackRef(1),
            Op::StackRef(1),
            Op::Add,
            Op::Dup,
            Op::Goto(13),
            Op::StackRef(1),
            Op::StackRef(1),
            Op::Add,
            Op::StackRef(2),
            Op::StackRef(2),
            Op::Add,
            Op::Eq,
            Op::Return,
        ],
        Vec::new(),
        3,
    )
}
fn partial_alias_baseline(
    ctx: &mut Context,
    roots: &Roots,
) -> (ByteCodeFunction, Func, Block, [Value; 2]) {
    let source = partial_alias_source();
    let func = plan(&source);
    let x = LispValue::make_float(1.25);
    roots.add(&[x]);
    let y = LispValue::make_float(2.0);
    roots.add(&[y]);
    for (choose, expected) in [
        (LispValue::T, LispValue::T),
        (LispValue::NIL, LispValue::NIL),
    ] {
        let args = [choose, x, y];
        assert_eq!(tier0(ctx, &source, &args), expected);
        assert_eq!(reference(ctx, &func, &args), expected);
    }
    let state = func.source_states[13]
        .as_ref()
        .expect("original Eq join source state");
    let pair = [
        state.pre[state.pre.len() - 2],
        state.pre[state.pre.len() - 1],
    ];
    assert_ne!(pair[0], pair[1]);
    assert!(func.blocks[state.block.index()].params.contains(&pair[0]));
    assert!(func.blocks[state.block.index()].params.contains(&pair[1]));
    assert_eq!(func.blocks[state.block.index()].preds.len(), 2);
    let join = state.block;
    (source, func, join, pair)
}

fn nested_cons_mutation_baseline(
    ctx: &mut Context,
    roots: &Roots,
) -> (ByteCodeFunction, Func, [Value; 2], Inst, FrameId) {
    // old child is compiler-local, not a constant or retained entry argument.
    // After Setcar, the original source frame at pc9 contains only parent.
    let source = source(
        vec![
            Op::Constant(0),
            Op::Constant(1),
            Op::Cons,
            Op::Constant(1),
            Op::Cons,
            Op::Dup,
            Op::Constant(2),
            Op::Setcar,
            Op::Pop,
            Op::Dup,
            Op::Car,
            Op::List(2),
            Op::Return,
        ],
        vec![LispValue::fixnum(11), LispValue::NIL, LispValue::fixnum(23)],
        0,
    );
    roots.add(&source.constants);
    let func = plan(&source);
    let vm_answer = tier0(ctx, &source, &[]);
    roots.add(&[vm_answer]);
    let ref_answer = reference(ctx, &func, &[]);
    roots.add(&[ref_answer]);
    for answer in [vm_answer, ref_answer] {
        let parent = answer.cons_car();
        assert!(parent.is_cons());
        assert_eq!(parent.cons_car(), LispValue::fixnum(23));
        assert_eq!(answer.cons_cdr().cons_car(), LispValue::fixnum(23));
    }
    let cons = opaque_results(&func, Op::Cons);
    assert_eq!(cons.len(), 2);
    let setter = Inst(
        func.insts
            .iter()
            .position(|inst| matches!(inst.op, Opcode::Opaque(Op::Setcar)))
            .unwrap() as u32,
    );
    let later = func.source_states[9].as_ref().unwrap();
    assert_eq!(later.pre.as_ref(), &[cons[1]]);
    let later_frame = later.frame;
    (source, func, [cons[0], cons[1]], setter, later_frame)
}

fn selected(original: &Func) -> Func {
    let mut func = original.clone();
    let mut feedback =
        vec![crate::emacs_core::jit::NumericFeedback::FixnumOnly; func.source_states.len()];
    for inst in &func.insts {
        if matches!(
            inst.op,
            Opcode::Opaque(Op::Add | Op::Sub | Op::Mul | Op::Div)
        ) {
            feedback[inst.pc as usize] = crate::emacs_core::jit::NumericFeedback::Float;
        }
    }
    let stats = crate::emacs_core::jit::opt::passes::sink::run(&mut func, &feedback)
        .expect("real sink selection after valid independent baseline");
    assert!(
        stats.numeric_sources + stats.cons_sources > 0,
        "selected producer exists before verifier poison"
    );
    func.verify()
        .expect("ordinary and recipe verification on positive selected plan");
    observation_ids(original, &func);
    func
}
fn number_fields(func: &Func, version: RecipeVersionId) -> (Value, Value, Value, Value) {
    let RecipeFields::Number(f) = func.sink_recipes.versions[version.0 as usize].fields else {
        panic!("numeric fields")
    };
    (f.payload, f.word, f.ready, f.real_box)
}
fn frame_version(func: &Func, point: RecipePoint, frame: FrameId, owner: Value) -> RecipeVersionId {
    func.sink_recipes.frames[&(point, frame)]
        .versions
        .iter()
        .find(|(v, _)| *v == owner)
        .unwrap()
        .1
}
fn reject(func: &Func, reasons: &[SinkVerifyReason]) {
    let failure = sink_recipes::verify_recipes(func, &func.sink_recipes)
        .err()
        .expect("invalid recipe rejected independently");
    assert!(
        reasons.contains(&failure.reason),
        "unexpected recipe rejection: {failure:?}"
    );
}
fn insert_observer_before(func: &mut Func, before: Inst, owners: [Value; 2]) -> Inst {
    let old = func.insts[before.index()].clone();
    assert_eq!(old.op, Opcode::Opaque(Op::Eq));
    let block = func
        .blocks
        .iter()
        .position(|b| b.insts.contains(&before))
        .unwrap();
    let id = Inst(func.insts.len() as u32);
    let result = Value(func.values.len() as u32);
    let old_result = old.result.unwrap();
    func.values.push(ValueData {
        def: ValueDef::Inst(id),
        ty: func.values[old_result.index()].ty,
        rep: Rep::Tagged,
    });
    func.insts.push(InstData {
        args: owners.into(),
        result: Some(result),
        ..old
    });
    let at = func.blocks[block]
        .insts
        .iter()
        .position(|&i| i == before)
        .unwrap();
    func.blocks[block].insts.insert(at, id);
    func.verify()
        .expect("attached actual Eq observer before selection");
    id
}

#[test]
fn opt_sink_recipe_same_frame_keeps_early_and_late_box_versions() {
    let mut ctx = Context::new();
    let roots = Roots::new(&[]);
    let (source, mut original, owners) = fresh_float_pair(&mut ctx, &roots);
    let eq = Inst(
        original
            .insts
            .iter()
            .position(|i| i.op == Opcode::Opaque(Op::Eq))
            .unwrap() as u32,
    );
    let frame = original.insts[eq.index()].frame.unwrap();
    let literal = original
        .insts
        .iter()
        .find_map(|i| (i.op == Opcode::Const(0)).then_some(i.result).flatten())
        .unwrap();
    let early = insert_number_guard(&mut original, eq, literal);
    let observer = insert_observer_before(&mut original, eq, owners);
    let late = insert_number_guard(&mut original, eq, literal);
    // Actual instruction order is early, Eq observer, late, original Eq.
    let block = original
        .blocks
        .iter()
        .find(|b| b.insts.contains(&eq))
        .unwrap();
    let positions =
        [early, observer, late, eq].map(|i| block.insts.iter().position(|&p| p == i).unwrap());
    assert!(positions.windows(2).all(|p| p[0] < p[1]));
    assert!(tier0(&mut ctx, &source, &[]).is_nil());
    assert!(reference(&mut ctx, &original, &[]).is_nil());
    let func = selected(&original);
    let before = frame_version(&func, RecipePoint::Before(early), frame, owners[0]);
    let after = frame_version(&func, RecipePoint::Before(late), frame, owners[0]);
    assert_ne!(before, after);
    let cap = sink_recipes::verify_recipes(&func, &func.sink_recipes).unwrap();
    assert!(!cap.guaranteed_boxed(before));
    assert!(cap.guaranteed_boxed(after));
    assert_ne!(
        number_fields(&func, before).3,
        number_fields(&func, after).3
    );
    assert_eq!(func.insts[early.index()].frame, Some(frame));
    assert_eq!(func.insts[late.index()].frame, Some(frame));
    let mut future = func.clone();
    future
        .sink_recipes
        .frames
        .get_mut(&(RecipePoint::Before(early), frame))
        .unwrap()
        .versions
        .iter_mut()
        .find(|(v, _)| *v == owners[0])
        .unwrap()
        .1 = after;
    reject(
        &future,
        &[
            SinkVerifyReason::FutureBoxVersion,
            SinkVerifyReason::NonDominatingVersion,
        ],
    );
    let mut stale = func.clone();
    stale
        .sink_recipes
        .frames
        .get_mut(&(RecipePoint::Before(late), frame))
        .unwrap()
        .versions
        .iter_mut()
        .find(|(v, _)| *v == owners[0])
        .unwrap()
        .1 = before;
    reject(&stale, &[SinkVerifyReason::StaleBoxVersion]);
}

#[test]
fn opt_sink_recipe_borrowed_top_requires_false_ready_word_box_lineage() {
    let mut ctx = Context::new();
    let roots = Roots::new(&[]);
    let source = frozen_loop_add();
    let seed = LispValue::cons(LispValue::fixnum(41), LispValue::NIL);
    roots.add(&[seed]);
    let args = [seed, LispValue::fixnum(0), LispValue::fixnum(0)];
    let original = plan(&source);
    let seed_arg = arg(&original, 0);
    let step_arg = arg(&original, 1);
    let vm = tier0(&mut ctx, &source, &args);
    roots.add(&[vm]);
    let eval = reference(&mut ctx, &original, &args);
    roots.add(&[eval]);
    for value in [vm, eval] {
        assert_eq!(value.cons_car().bits(), seed.bits());
        assert_eq!(value.cons_cdr().bits(), seed.bits());
    }
    assert_eq!(original.values[seed_arg.index()].ty, TypeSet::TOP);
    let func = selected(&original);
    let borrow = func
        .sink_recipes
        .owners
        .values()
        .find(|o| matches!(o.origin,RecipeOrigin::Borrow {original,..} if original==seed_arg))
        .unwrap();
    let version = borrow.definition_version;
    let owner = borrow.owner;
    let (payload, word, ready, real_box) = number_fields(&func, version);
    // Readiness/word/box relations are certified by actual BorrowNum projections,
    // not inferred from declarations or by dereferencing the arbitrary seed.
    for (value, field) in [
        (payload, sink_recipes::RecipeField::Payload),
        (word, sink_recipes::RecipeField::Word),
        (ready, sink_recipes::RecipeField::Ready),
        (real_box, sink_recipes::RecipeField::RealBox),
    ] {
        let ValueDef::Inst(inst) = func.values[value.index()].def else {
            panic!("actual projection")
        };
        assert_eq!(
            func.insts[inst.index()].op,
            Opcode::Sink(SinkOp::RecipeField(field))
        );
        assert_eq!(func.insts[inst.index()].args, [owner]);
    }
    assert_eq!(func.values[seed_arg.index()].rep, Rep::Tagged);
    assert_eq!(func.values[seed_arg.index()].ty, TypeSet::TOP);
    let mut box_poison = func.clone();
    let RecipeFields::Number(f) = &mut box_poison.sink_recipes.versions[version.0 as usize].fields
    else {
        unreachable!()
    };
    f.real_box = step_arg;
    reject(&box_poison, &[SinkVerifyReason::InvalidBorrow]);
    let other = func
        .sink_recipes
        .owners
        .values()
        .find(|o| matches!(o.origin,RecipeOrigin::Borrow {original,..} if original==step_arg))
        .expect("actual other Tagged seed borrow");
    let wrong_word = number_fields(&func, other.definition_version).1;
    assert_ne!(word, wrong_word);
    let mut word_poison = func.clone();
    let RecipeFields::Number(f) = &mut word_poison.sink_recipes.versions[version.0 as usize].fields
    else {
        unreachable!()
    };
    f.word = wrong_word;
    reject(&word_poison, &[SinkVerifyReason::InvalidBorrow]);
}

#[test]
fn opt_sink_recipe_phi_rejects_incomplete_tuple() {
    let mut ctx = Context::new();
    let roots = Roots::new(&[]);
    let (_, original, join, owners) = partial_alias_baseline(&mut ctx, &roots);
    let func = selected(&original);
    let mut poison = func.clone();
    let edge = poison
        .sink_recipes
        .edges
        .iter_mut()
        .find(|e| e.target == join && e.owner_param == owners[0])
        .unwrap();
    assert_eq!(edge.field_args.len(), 4);
    edge.field_args = edge.field_args[..3].into();
    reject(&poison, &[SinkVerifyReason::IncompleteTuple]);
}
#[test]
fn opt_sink_recipe_phi_rejects_same_rep_mismatched_tuple() {
    let mut ctx = Context::new();
    let roots = Roots::new(&[]);
    let (_, original, join, owners) = partial_alias_baseline(&mut ctx, &roots);
    let func = selected(&original);
    let (left, right) = func
        .sink_recipes
        .edges
        .iter()
        .filter(|e| e.target == join && e.owner_param == owners[0])
        .find_map(|a| {
            func.sink_recipes
                .edges
                .iter()
                .find(|b| {
                    b.source == a.source
                        && b.edge_index == a.edge_index
                        && b.owner_param == owners[1]
                        && b.incoming_owner != a.incoming_owner
                })
                .map(|b| (a.clone(), b.clone()))
        })
        .unwrap();
    assert_ne!(left.field_args[0], right.field_args[0]);
    assert_eq!(
        func.values[left.field_args[0].index()].rep,
        func.values[right.field_args[0].index()].rep
    );
    let mut poison = func.clone();
    poison
        .sink_recipes
        .edges
        .iter_mut()
        .find(|e| {
            e.source == left.source && e.edge_index == left.edge_index && e.owner_param == owners[0]
        })
        .unwrap()
        .field_args[0] = right.field_args[0];
    reject(&poison, &[SinkVerifyReason::WrongEdgeTuple]);
}
#[test]
fn opt_sink_recipe_equal_payload_distinct_producers_keep_identity() {
    let mut ctx = Context::new();
    let roots = Roots::new(&[]);
    let (_, original, owners) = fresh_float_pair(&mut ctx, &roots);
    let func = selected(&original);
    let eq = Inst(
        original
            .insts
            .iter()
            .position(|i| i.op == Opcode::Opaque(Op::Eq))
            .unwrap() as u32,
    );
    let cap = sink_recipes::verify_recipes(&func, &func.sink_recipes).unwrap();
    assert!(!cap.equivalent_identity(RecipePoint::Before(eq), owners[0], owners[1]));
    let left = &func.sink_recipes.owners[&owners[0]];
    let right = &func.sink_recipes.owners[&owners[1]];
    assert!(matches!(left.origin, RecipeOrigin::NumericSource { .. }));
    assert!(matches!(right.origin, RecipeOrigin::NumericSource { .. }));
    assert_ne!(left.origin, right.origin);
    let mut poison = func.clone();
    poison
        .sink_recipes
        .owners
        .get_mut(&owners[1])
        .unwrap()
        .origin = RecipeOrigin::SameIdentityView {
        inst: match original.values[owners[1].index()].def {
            ValueDef::Inst(i) => i,
            _ => unreachable!(),
        },
        input: owners[0],
    };
    reject(&poison, &[SinkVerifyReason::InvalidSourceOperation]);
}
#[test]
fn opt_sink_recipe_partial_alias_phi_shared_edge_has_one_actual_box() {
    let mut ctx = Context::new();
    let roots = Roots::new(&[]);
    let (_, original, join, owners) = partial_alias_baseline(&mut ctx, &roots);
    let func = selected(&original);
    let point = RecipePoint::SourcePre(13);
    let cap = sink_recipes::verify_recipes(&func, &func.sink_recipes).unwrap();
    assert!(!cap.equivalent_identity(point, owners[0], owners[1]));
    let (left, right) = func
        .sink_recipes
        .edges
        .iter()
        .filter(|e| e.target == join && e.owner_param == owners[0])
        .find_map(|a| {
            func.sink_recipes
                .edges
                .iter()
                .find(|b| {
                    b.source == a.source
                        && b.edge_index == a.edge_index
                        && b.owner_param == owners[1]
                        && b.incoming_owner == a.incoming_owner
                })
                .map(|b| (a.clone(), b.clone()))
        })
        .unwrap();
    assert_eq!(left.field_args[3], right.field_args[3]);
    assert!(cap.guaranteed_boxed(left.incoming_version));
    let unboxed = func.sink_recipes.owners[&left.incoming_owner].definition_version;
    assert_ne!(number_fields(&func, unboxed).3, left.field_args[3]);
    let tuple = number_fields(&func, unboxed);
    let mut poison = func.clone();
    let edge = poison
        .sink_recipes
        .edges
        .iter_mut()
        .find(|e| {
            e.source == right.source
                && e.edge_index == right.edge_index
                && e.owner_param == owners[1]
        })
        .unwrap();
    edge.incoming_version = unboxed;
    edge.field_args = vec![tuple.0, tuple.1, tuple.2, tuple.3].into();
    reject(
        &poison,
        &[
            SinkVerifyReason::LostPartialAlias,
            SinkVerifyReason::WrongEdgeTuple,
        ],
    );
}
fn cons_cuts(func: &Func, child: Value, parent: Value) -> (Inst, Value, Value) {
    let mut parent_mat = None;
    let mut child_box = None;
    let mut parent_box = None;
    for version in &func.sink_recipes.versions {
        if let VersionCause::CacheAfter { materialize, .. } = version.cause {
            let RecipeFields::Cons(f) = version.fields else {
                continue;
            };
            if version.owner == child {
                child_box = Some(f.real_box);
            }
            if version.owner == parent {
                parent_mat = Some(materialize);
                parent_box = Some(f.real_box);
            }
        }
    }
    (parent_mat.unwrap(), child_box.unwrap(), parent_box.unwrap())
}
#[test]
fn opt_sink_recipe_boxed_cons_roots_stop_old_fields_before_and_after_mutation() {
    let mut ctx = Context::new();
    let roots = Roots::new(&[]);
    let (_, original, owners, setter, later_frame) =
        nested_cons_mutation_baseline(&mut ctx, &roots);
    let func = selected(&original);
    let (parent_mat, child_box, parent_box) = cons_cuts(&func, owners[0], owners[1]);
    let before_frame = original.insts[setter.index()].frame.unwrap();
    let cap = sink_recipes::verify_recipes(&func, &func.sink_recipes).unwrap();
    assert_eq!(func.insts[parent_mat.index()].frame, Some(before_frame));
    assert_eq!(
        sink_recipes::roots_at(
            &func,
            &cap,
            RecipePoint::Before(parent_mat),
            before_frame,
            &[]
        )
        .unwrap(),
        vec![child_box]
    );
    assert_eq!(
        sink_recipes::roots_at(&func, &cap, RecipePoint::Before(setter), before_frame, &[])
            .unwrap(),
        vec![parent_box]
    );
    assert_eq!(
        sink_recipes::roots_at(&func, &cap, RecipePoint::SourcePre(9), later_frame, &[]).unwrap(),
        vec![parent_box]
    );
    assert_eq!(func.insts[setter.index()].op, Opcode::Opaque(Op::Setcar));
    assert_eq!(
        func.insts[setter.index()].eff,
        original.insts[setter.index()].eff
    );
    assert_eq!(
        func.insts[setter.index()].frame,
        original.insts[setter.index()].frame
    );
}
#[test]
fn opt_sink_recipe_roots_ignore_dead_historical_cons_owners() {
    let mut ctx = Context::new();
    let roots = Roots::new(&[]);
    let (_, original, owners, setter, frame) = nested_cons_mutation_baseline(&mut ctx, &roots);
    let func = selected(&original);
    let (_, child_box, parent_box) = cons_cuts(&func, owners[0], owners[1]);
    assert!(func.sink_recipes.owners.contains_key(&owners[0]));
    let cap = sink_recipes::verify_recipes(&func, &func.sink_recipes).unwrap();
    let actual =
        sink_recipes::roots_at(&func, &cap, RecipePoint::SourcePre(9), frame, &[]).unwrap();
    assert_eq!(actual, vec![parent_box]);
    assert!(!actual.contains(&child_box));
    assert!(!actual.contains(&owners[0]));
}
#[test]
fn opt_sink_recipe_roots_never_publish_numeric_raw_transport_fields() {
    let mut ctx = Context::new();
    let roots = Roots::new(&[]);
    let source = frozen_loop_add();
    let original = plan(&source);
    let seed = LispValue::cons(LispValue::fixnum(41), LispValue::NIL);
    roots.add(&[seed]);
    let args = [seed, LispValue::fixnum(0), LispValue::fixnum(0)];
    let answer = tier0(&mut ctx, &source, &args);
    roots.add(&[answer]);
    assert_eq!(answer.cons_car().bits(), seed.bits());
    assert_eq!(
        reference(&mut ctx, &original, &args).cons_car().bits(),
        seed.bits()
    );
    let func = selected(&original);
    let (pc, state) = func
        .source_states
        .iter()
        .enumerate()
        .filter_map(|(pc, s)| s.as_ref().map(|s| (pc, s)))
        .find(|(_, s)| {
            s.pre
                .iter()
                .any(|v| func.sink_recipes.owners.contains_key(v))
        })
        .unwrap();
    let point = RecipePoint::SourcePre(pc as u32);
    let owner = *state
        .pre
        .iter()
        .find(|v| func.sink_recipes.owners.contains_key(v))
        .unwrap();
    let cap = sink_recipes::verify_recipes(&func, &func.sink_recipes).unwrap();
    let version = cap.version_at(point, owner).unwrap();
    let (payload, word, ready, real_box) = number_fields(&func, version);
    let actual = sink_recipes::roots_at(&func, &cap, point, state.frame, &[]).unwrap();
    assert!(actual.contains(&real_box));
    for raw in [payload, word, ready] {
        assert!(!actual.contains(&raw));
    }
    for value in actual {
        assert!(func.values[value.index()].rep.is_tagged());
    }
}
#[test]
fn opt_sink_recipe_cache_phi_rejects_stale_boxed_arm_even_with_matching_edge_ssa() {
    let mut ctx = Context::new();
    let roots = Roots::new(&[]);
    let source = source(
        vec![
            Op::StackRef(1),
            Op::StackRef(1),
            Op::Add,
            Op::StackRef(3),
            Op::GotoIfNil(10),
            Op::Dup,
            Op::Dup,
            Op::Eq,
            Op::Pop,
            Op::Goto(10),
            Op::Dup,
            Op::Eq,
            Op::Return,
        ],
        Vec::new(),
        3,
    );
    let original = plan(&source);
    let x = LispValue::make_float(1.25);
    roots.add(&[x]);
    let y = LispValue::make_float(2.0);
    roots.add(&[y]);
    for choice in [LispValue::T, LispValue::NIL] {
        let args = [choice, x, y];
        assert_eq!(tier0(&mut ctx, &source, &args), LispValue::T);
        assert_eq!(reference(&mut ctx, &original, &args), LispValue::T);
    }
    let owner = opaque_results(&original, Op::Add)[0];
    let func = selected(&original);
    let (id, block, param, incoming) = func
        .sink_recipes
        .versions
        .iter()
        .enumerate()
        .find_map(|(id, v)| match &v.cause {
            VersionCause::CachePhi {
                block,
                box_param,
                incoming,
            } if v.owner == owner => Some((
                RecipeVersionId(id as u32),
                *block,
                *box_param,
                incoming.clone(),
            )),
            _ => None,
        })
        .unwrap();
    let cap = sink_recipes::verify_recipes(&func, &func.sink_recipes).unwrap();
    assert_eq!(cap.version(id).unwrap().owner, owner);
    let boxed = incoming
        .iter()
        .find(|e| cap.guaranteed_boxed(e.version))
        .unwrap()
        .clone();
    let old = func.sink_recipes.owners[&owner].definition_version;
    assert_ne!(boxed.version, old);
    let old_box = number_fields(&func, old).3;
    let mut poison = func.clone();
    let VersionCause::CachePhi { incoming, .. } =
        &mut poison.sink_recipes.versions[id.0 as usize].cause
    else {
        unreachable!()
    };
    incoming
        .iter_mut()
        .find(|e| e.source == boxed.source && e.edge_index == boxed.edge_index)
        .unwrap()
        .version = old;
    let ValueDef::Param { index, .. } = poison.values[param.index()].def else {
        unreachable!()
    };
    let edge = edge_mut(
        &mut poison.blocks[boxed.source.index()].term,
        boxed.edge_index as usize,
    )
    .unwrap();
    assert_eq!(edge.target, block);
    edge.args[index as usize] = old_box;
    reject(
        &poison,
        &[
            SinkVerifyReason::WrongCachePhi,
            SinkVerifyReason::StaleBoxVersion,
        ],
    );
}

fn edge_mut(term: &mut Term, index: usize) -> Option<&mut Edge> {
    match term {
        Term::Jump(edge) => (index == 0).then_some(edge),
        Term::Branch {
            if_true, if_false, ..
        } => match index {
            0 => Some(if_true),
            1 => Some(if_false),
            _ => None,
        },
        Term::Switch { cases, default, .. } => {
            if index < cases.len() {
                Some(&mut cases[index].edge)
            } else {
                (index == cases.len()).then_some(default)
            }
        }
        _ => None,
    }
}

#[test]
fn opt_sink_recipe_copied_numeric_view_is_complete_and_point_local() {
    let mut ctx = Context::new();
    let roots = Roots::new(&[]);
    let (source, mut original, owners) = fresh_float_pair(&mut ctx, &roots);
    // Both real operands of these two source multiplications are frozen GNU
    // Float literals. Successful results cannot be bignums; expose precisely
    // that independent source fact before adding a test-only pure view.
    assert_eq!(original.dynamic_prefix, 0);
    assert!(source.constants.iter().all(|value| value.is_float()));
    for owner in owners {
        let ValueDef::Inst(producer) = original.values[owner.index()].def else {
            unreachable!()
        };
        assert_eq!(original.insts[producer.index()].op, Opcode::Opaque(Op::Mul));
        for &arg in &original.insts[producer.index()].args {
            let ValueDef::Inst(literal) = original.values[arg.index()].def else {
                unreachable!()
            };
            let Opcode::Const(pool) = original.insts[literal.index()].op else {
                unreachable!()
            };
            assert!(original.consts[pool as usize].to_value().is_float());
        }
    }
    let successful = TypeSet::FIXNUM.join(TypeSet::FLOAT);
    for owner in owners {
        original.values[owner.index()].ty = successful;
    }
    let ValueDef::Inst(producer) = original.values[owners[0].index()].def else {
        unreachable!()
    };
    let view_inst = Inst(original.insts.len() as u32);
    let view_owner = Value(original.values.len() as u32);
    let ty = original.values[owners[0].index()].ty;
    original.values.push(ValueData {
        def: ValueDef::Inst(view_inst),
        ty,
        rep: Rep::Tagged,
    });
    original.insts.push(InstData {
        op: Opcode::Refine(ty),
        args: vec![owners[0]],
        result: Some(view_owner),
        eff: Effects::PURE,
        mem: AliasClass::None,
        frame: None,
        pc: original.insts[producer.index()].pc,
    });
    let block = original
        .blocks
        .iter_mut()
        .find(|b| b.insts.contains(&producer))
        .unwrap();
    let at = block.insts.iter().position(|&i| i == producer).unwrap();
    block.insts.insert(at + 1, view_inst);
    let eq = original
        .insts
        .iter_mut()
        .find(|i| i.op == Opcode::Opaque(Op::Eq))
        .unwrap();
    assert_eq!(eq.args.as_slice(), owners.as_slice());
    eq.args[0] = view_owner;
    original
        .verify()
        .expect("ordinary pure same-word view is valid before Sink");
    assert!(tier0(&mut ctx, &source, &[]).is_nil());
    assert!(reference(&mut ctx, &original, &[]).is_nil());
    let func = selected(&original);
    let info = &func.sink_recipes.owners[&view_owner];
    assert!(
        matches!(info.origin, RecipeOrigin::SameIdentityView { inst, input }
        if inst == view_inst && input == owners[0])
    );
    let VersionCause::SameIdentity { input } =
        func.sink_recipes.versions[info.definition_version.0 as usize].cause
    else {
        unreachable!()
    };
    let copied = func.sink_recipes.versions[info.definition_version.0 as usize].fields;
    let before = func.sink_recipes.versions[input.0 as usize].fields;
    assert_ne!(
        copied, before,
        "actual selected numeric Refine copies four SSA fields"
    );
    let RecipeFields::Number(fields) = copied else {
        unreachable!()
    };
    let ValueDef::Inst(last) = func.values[fields.real_box.index()].def else {
        unreachable!()
    };
    let cap = sink_recipes::verify_recipes(&func, &func.sink_recipes).unwrap();
    assert_eq!(
        cap.version_at(RecipePoint::Before(view_inst), view_owner),
        None
    );
    assert_eq!(
        cap.version_at(RecipePoint::After(view_inst), view_owner),
        None
    );
    assert_eq!(cap.version_at(RecipePoint::Before(last), view_owner), None);
    assert_eq!(
        cap.version_at(RecipePoint::After(last), view_owner),
        Some(info.definition_version)
    );
    assert!(cap.equivalent_identity(RecipePoint::After(last), view_owner, owners[0]));
    let future = func
        .sink_recipes
        .versions
        .iter()
        .enumerate()
        .find_map(|(i, v)| {
            (v.owner == owners[0]
                && matches!(
                    v.cause,
                    VersionCause::CacheAfter { .. } | VersionCause::AliasCacheAfter { .. }
                ))
            .then_some(RecipeVersionId(i as u32))
        })
        .expect("actual later Eq observes and materializes the copied identity");
    let mut poison = func.clone();
    poison.sink_recipes.versions[info.definition_version.0 as usize].cause =
        VersionCause::SameIdentity { input: future };
    reject(
        &poison,
        &[
            SinkVerifyReason::StaleBoxVersion,
            SinkVerifyReason::FutureBoxVersion,
        ],
    );
}

#[test]
fn opt_sink_recipe_unboxed_cons_view_roots_follow_checked_identity_lineage() {
    let mut ctx = Context::new();
    let roots = Roots::new(&[]);
    let (source, mut original, owners, setter, later_frame) =
        nested_cons_mutation_baseline(&mut ctx, &roots);
    let parent = owners[1];
    let guard = Inst(
        original
            .insts
            .iter()
            .position(|inst| {
                inst.op == Opcode::CheckType(TypeSet::CONS)
                    && inst.args.as_slice() == [parent]
                    && inst.pc == original.insts[setter.index()].pc
            })
            .unwrap() as u32,
    );
    let frame = original.insts[guard.index()].frame.unwrap();
    let pc = original.insts[guard.index()].pc;
    let alias_inst = Inst(original.insts.len() as u32);
    let alias = Value(original.values.len() as u32);
    assert_eq!(original.values[parent.index()].ty, TypeSet::CONS);
    original.values.push(ValueData {
        def: ValueDef::Inst(alias_inst),
        ty: TypeSet::CONS,
        rep: Rep::Tagged,
    });
    original.insts.push(InstData {
        op: Opcode::Refine(TypeSet::CONS),
        args: vec![parent],
        result: Some(alias),
        eff: Effects::PURE,
        mem: AliasClass::None,
        frame: None,
        pc,
    });
    let block = original
        .blocks
        .iter_mut()
        .find(|b| b.insts.contains(&guard))
        .unwrap();
    let at = block.insts.iter().position(|&i| i == guard).unwrap();
    block.insts.insert(at, alias_inst);
    // The alias is genuinely SSA-live at its After cut: the original guard
    // consumes it before the retained setter. GNU frame/source IDs stay exact.
    original.insts[guard.index()].args[0] = alias;
    original
        .verify()
        .expect("ordinary checked Cons identity view before Sink");
    let vm_answer = tier0(&mut ctx, &source, &[]);
    roots.add(&[vm_answer]);
    let ref_answer = reference(&mut ctx, &original, &[]);
    roots.add(&[ref_answer]);
    for answer in [vm_answer, ref_answer] {
        assert_eq!(answer.cons_car().cons_car(), LispValue::fixnum(23));
        assert_eq!(answer.cons_cdr().cons_car(), LispValue::fixnum(23));
    }
    let retained_store = original.insts[setter.index()].clone();
    let func = selected(&original);
    let info = &func.sink_recipes.owners[&alias];
    assert!(
        matches!(info.origin, RecipeOrigin::SameIdentityView { inst, input }
        if inst == alias_inst && input == parent)
    );
    let VersionCause::SameIdentity { input } =
        func.sink_recipes.versions[info.definition_version.0 as usize].cause
    else {
        unreachable!()
    };
    assert_eq!(
        func.sink_recipes.versions[info.definition_version.0 as usize].fields,
        func.sink_recipes.versions[input.0 as usize].fields
    );
    let cap = sink_recipes::verify_recipes(&func, &func.sink_recipes).unwrap();
    let point = RecipePoint::After(alias_inst);
    assert_eq!(cap.version_at(point, alias), Some(info.definition_version));
    assert!(!cap.guaranteed_boxed(info.definition_version));
    assert!(cap.equivalent_identity(point, alias, parent));
    // No physical Cons exists at this cut and all ultimate fields are
    // immediates. Correct checked recursive expansion therefore has no roots.
    assert_eq!(
        sink_recipes::roots_at(&func, &cap, point, frame, &[alias])
            .expect("unboxed copied Cons lineage must expand its exact old fields"),
        Vec::<Value>::new()
    );
    let after_store = RecipePoint::SourcePre(9);
    let current = cap.version_at(after_store, parent).unwrap();
    assert!(cap.guaranteed_boxed(current));
    let RecipeFields::Cons(fields) = cap.version(current).unwrap().fields else {
        unreachable!()
    };
    assert_eq!(
        sink_recipes::roots_at(&func, &cap, after_store, later_frame, &[]).unwrap(),
        vec![fields.real_box],
        "materialized/mutated parent stops old child fields"
    );
    let store = &func.insts[setter.index()];
    assert_eq!(store.op, retained_store.op);
    assert_eq!(store.eff, retained_store.eff);
    assert_eq!(store.frame, retained_store.frame);
    assert_eq!(store.pc, retained_store.pc);
}

fn dominating_partial_baseline(ctx: &mut Context, roots: &Roots) -> (Func, Block, Value, Value) {
    ctx.gc_stress = false;
    ctx.set_gc_threshold(usize::MAX);
    // flag,x. True carries dominating x twice; false keeps dominating x and
    // creates fresh y with equal payload. Actual GC precedes Cons observation.
    let source = source(
        vec![
            Op::Constant(0),
            Op::Constant(1),
            Op::Mul,
            Op::Dup,
            Op::StackRef(2),
            Op::GotoIfNil(7),
            Op::Goto(11),
            Op::Pop,
            Op::Constant(0),
            Op::Constant(1),
            Op::Mul,
            Op::Constant(2),
            Op::Call(0),
            Op::Pop,
            Op::Cons,
            Op::Return,
        ],
        vec![
            LispValue::make_float(2.0),
            LispValue::make_float(1.5),
            LispValue::symbol("garbage-collect"),
        ],
        1,
    );
    roots.add(&source.constants);
    let original = plan(&source);
    original.verify().unwrap();
    for flag in [LispValue::T, LispValue::NIL] {
        let before = ctx.gc_count;
        let value = tier0(ctx, &source, &[flag]);
        roots.add(&[value]);
        assert_eq!(ctx.gc_count - before, 1);
        let car = value.cons_car();
        let cdr = value.cons_cdr();
        assert!(car.is_float() && cdr.is_float());
        assert_eq!(car.bits() == cdr.bits(), !flag.is_nil());
        let before = ctx.gc_count;
        let actual = reference(ctx, &original, &[flag]);
        roots.add(&[actual]);
        assert_eq!(ctx.gc_count - before, 1);
        assert_eq!(actual.cons_car().xfloat().to_bits(), car.xfloat().to_bits());
        assert_eq!(actual.cons_cdr().xfloat().to_bits(), cdr.xfloat().to_bits());
        assert_eq!(
            actual.cons_car().bits() == actual.cons_cdr().bits(),
            car.bits() == cdr.bits()
        );
        assert_eq!(ctx.jit_root_stack_top, 0);
    }
    let state = original.source_states[11].as_ref().unwrap();
    let dominating = state.pre[state.pre.len() - 2];
    let phi = state.pre[state.pre.len() - 1];
    assert_eq!(dominating, opaque_results(&original, Op::Mul)[0]);
    assert!(
        !original.blocks[state.block.index()]
            .params
            .contains(&dominating)
    );
    assert!(original.blocks[state.block.index()].params.contains(&phi));
    (original.clone(), state.block, dominating, phi)
}

fn dominating_shared_tuple(
    func: &Func,
    join: Block,
    dominating: Value,
    phi: Value,
) -> (sink_recipes::RecipeEdge, RecipeVersionId, Value) {
    let edge = func
        .sink_recipes
        .edges
        .iter()
        .find(|edge| {
            edge.target == join && edge.owner_param == phi && edge.incoming_owner == dominating
        })
        .unwrap()
        .clone();
    let (cache, parameter) = func
        .sink_recipes
        .versions
        .iter()
        .enumerate()
        .find_map(|(id, version)| match &version.cause {
            VersionCause::CachePhi {
                block, box_param, ..
            } if *block == join && version.owner == dominating => {
                Some((RecipeVersionId(id as u32), *box_param))
            }
            _ => None,
        })
        .unwrap();
    (edge, cache, parameter)
}

#[test]
fn opt_sink_recipe_dominating_owner_and_partial_phi_share_actual_edge_box() {
    let mut ctx = Context::new();
    let roots = Roots::new(&[]);
    let (original, join, dominating, phi) = dominating_partial_baseline(&mut ctx, &roots);
    let func = selected(&original);
    let (edge, cache, _) = dominating_shared_tuple(&func, join, dominating, phi);
    let cap = sink_recipes::verify_recipes(&func, &func.sink_recipes).unwrap();
    assert!(
        !cap.equivalent_identity(RecipePoint::Entry(join), dominating, phi),
        "partial shared edge does not make false-arm distinct object identical"
    );
    let VersionCause::CachePhi { incoming, .. } =
        &func.sink_recipes.versions[cache.0 as usize].cause
    else {
        unreachable!()
    };
    let arm = incoming
        .iter()
        .find(|arm| arm.source == edge.source && arm.edge_index == edge.edge_index)
        .unwrap();
    assert_eq!(number_fields(&func, arm.version).3, edge.field_args[3]);
    assert!(
        cap.guaranteed_boxed(edge.incoming_version),
        "one actual box is required for the shared dominating/phi edge"
    );
    assert!(cap.guaranteed_boxed(arm.version));
}

#[test]
fn opt_sink_recipe_verifier_rejects_uncached_dominating_partial_alias_edge() {
    let mut ctx = Context::new();
    let roots = Roots::new(&[]);
    let (original, join, dominating, phi) = dominating_partial_baseline(&mut ctx, &roots);
    let func = selected(&original);
    let (edge, cache, cache_parameter) = dominating_shared_tuple(&func, join, dominating, phi);
    // On current production this is the actual accepted bad edge already:
    // both independently coherent tuples carry the same NIL. After producer
    // correction deliberately restore that tuple on both physical transports.
    let old = func.sink_recipes.owners[&dominating].definition_version;
    let tuple = number_fields(&func, old);
    let mut poison = func.clone();
    let forged = poison
        .sink_recipes
        .edges
        .iter_mut()
        .find(|candidate| {
            candidate.owner_param == phi
                && candidate.source == edge.source
                && candidate.edge_index == edge.edge_index
        })
        .unwrap();
    forged.incoming_version = old;
    forged.field_args = vec![tuple.0, tuple.1, tuple.2, tuple.3].into();
    let VersionCause::CachePhi { incoming, .. } =
        &mut poison.sink_recipes.versions[cache.0 as usize].cause
    else {
        unreachable!()
    };
    incoming
        .iter_mut()
        .find(|arm| arm.source == edge.source && arm.edge_index == edge.edge_index)
        .unwrap()
        .version = old;
    let RecipeOrigin::Phi { field_params, .. } = &poison.sink_recipes.owners[&phi].origin else {
        unreachable!()
    };
    let params = field_params.to_vec();
    let fields = [tuple.0, tuple.1, tuple.2, tuple.3];
    let transport = edge_mut(
        &mut poison.blocks[edge.source.index()].term,
        edge.edge_index as usize,
    )
    .unwrap();
    for (param, field) in params.into_iter().zip(fields) {
        let ValueDef::Param { index, .. } = poison.values[param.index()].def else {
            unreachable!()
        };
        transport.args[index as usize] = field;
    }
    let ValueDef::Param { index, .. } = poison.values[cache_parameter.index()].def else {
        unreachable!()
    };
    transport.args[index as usize] = tuple.3;
    let error = sink_recipes::verify_recipes(&poison, &poison.sink_recipes)
        .err()
        .expect("matching NIL tuples do not certify preserved partial identity");
    assert_eq!(error.reason, SinkVerifyReason::LostPartialAlias);
}
