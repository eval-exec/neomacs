use super::*;
use crate::emacs_core::bytecode::ByteCodeFunction;
use crate::emacs_core::intern::intern;
use crate::emacs_core::jit::compile::{self, Inline2Mode};
use crate::emacs_core::jit::inline::{self, force_inline_for_test};
use crate::emacs_core::value::LambdaParams;

struct Knobs;
impl Knobs {
    fn enter() -> Self {
        force_inline_for_test(Some(true));
        compile::force_inline2_for_test(Some(Inline2Mode::All));
        compile::force_deopt_for_test(false);
        Self
    }
}
impl Drop for Knobs {
    fn drop(&mut self) {
        force_inline_for_test(None);
        compile::force_inline2_for_test(None);
        compile::force_deopt_for_test(false);
    }
}

fn step() -> Value {
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: vec![intern("entry-cache-step")],
        optional: vec![],
        rest: None,
    });
    f.lexical = true;
    f.ops = vec![Op::StackRef(0), Op::Sub1, Op::Return];
    f.max_stack = 8;
    f.seal_hand_assembled_ops_for_test();
    f.jit_runtime().set_hot_for_test();
    Value::make_bytecode(f)
}

fn fuse(ops: Vec<Op>, constants: Vec<Value>) -> FusedBody {
    inline::fuse_calls_v2(
        &ops,
        &constants,
        None,
        1,
        &vec![NumericFeedback::FixnumOnly; ops.len()],
    )
    .expect("admitted static call")
}

fn constant_loop(effect: Vec<Op>) -> FusedBody {
    constant_loop_with_prefix(vec![], effect)
}

fn constant_loop_with_prefix(mut prefix: Vec<Op>, effect: Vec<Op>) -> FusedBody {
    let header = prefix.len();
    let mut ops = vec![
        Op::StackRef(0),
        Op::Constant(0),
        Op::Gtr,
        Op::GotoIfNil(0),
        Op::Constant(1),
        Op::StackRef(1),
        Op::Call(1),
        Op::StackSet(1),
    ];
    ops.extend(effect);
    ops.push(Op::Goto(header as u32));
    let end = header + ops.len();
    ops[3] = Op::GotoIfNil(end as u32);
    ops.push(Op::Return);
    prefix.extend(ops);
    fuse(
        prefix,
        vec![
            Value::make_int(0),
            step(),
            Value::symbol("entry-cache-effect"),
        ],
    )
}

fn candidates(body: &FusedBody, osr: Option<usize>) -> Vec<bool> {
    let _feedback = compile::publish_numeric_feedback_vec(body.feedback.clone());
    let cfg =
        compile::analyze_cfg(&body.ops, &body.constants, body.offset_map.as_deref(), 1).unwrap();
    candidate_regions(body, &cfg, 1, osr, 0)
}

fn physical(body: &FusedBody, osr: Option<usize>) -> Vec<bool> {
    let _feedback = compile::publish_numeric_feedback_vec(body.feedback.clone());
    let cfg =
        compile::analyze_cfg(&body.ops, &body.constants, body.offset_map.as_deref(), 1).unwrap();
    physical_admission_regions(body, &cfg, 1, osr, 0)
}

#[test]
fn inline_entry_cache_rejects_context_opaque_and_allocation_effects_in_the_loop() {
    let _knobs = Knobs::enter();
    for effect in [
        vec![Op::Constant(0), Op::VarSet(2)],
        vec![Op::Constant(2), Op::Call(0), Op::Pop],
        vec![Op::Nil, Op::Nil, Op::Cons, Op::Pop],
    ] {
        assert_eq!(candidates(&constant_loop(effect), None), vec![false]);
    }
}

#[test]
fn inline_entry_cache_rejects_float_and_generic_arithmetic_lowerings() {
    let _knobs = Knobs::enter();
    let mut body = constant_loop(vec![]);
    assert_eq!(candidates(&body, None), vec![true]);
    let inner = body
        .ops
        .iter()
        .position(|op| matches!(op, Op::Sub1))
        .unwrap();
    for feedback in [NumericFeedback::Float, NumericFeedback::Other] {
        body.feedback[inner] = feedback;
        assert_eq!(candidates(&body, None), vec![false]);
    }
}

#[test]
fn inline_entry_cache_requires_independent_constant_identity_proof() {
    let _knobs = Knobs::enter();
    let mut body = constant_loop(vec![]);
    assert_eq!(candidates(&body, None), vec![true]);
    assert_eq!(
        candidates(&body, Some(0)),
        vec![true],
        "OSR recomputes the constant in the loop"
    );
    let producer = body
        .caller_of_fused
        .iter()
        .enumerate()
        .find(|(pc, original)| **original == 4 && body.region_of[*pc].is_none())
        .map(|(pc, _)| pc)
        .unwrap();
    body.ops[producer] = Op::StackRef(0);
    assert_eq!(
        candidates(&body, None),
        vec![false],
        "selected region metadata alone is not a proof"
    );
}

#[test]
fn inline_entry_cache_allows_preloop_closure_maker_but_retains_fresh_identity() {
    let _knobs = Knobs::enter();
    let body = fuse(
        vec![
            Op::Constant(0),
            Op::Constant(1),
            Op::Call(1),
            Op::StackRef(1),
            Op::Constant(2),
            Op::Gtr,
            Op::GotoIfNil(12),
            Op::StackRef(0),
            Op::StackRef(2),
            Op::Call(1),
            Op::StackSet(2),
            Op::Goto(3),
            Op::Pop,
            Op::Return,
        ],
        vec![Value::symbol("make-closure"), step(), Value::make_int(0)],
    );
    assert!(matches!(
        body.v2.as_ref().unwrap().region_kind[0],
        RegionKind::Closure { .. }
    ));
    assert_eq!(candidates(&body, None), vec![true]);
    compile::force_deopt_for_test(true);
    assert_eq!(
        candidates(&body, None),
        vec![false],
        "forced guards retain their original emission"
    );
}

#[test]
fn inline_entry_physical_admission_rejects_effectful_preludes_but_keeps_loop_flags() {
    let _knobs = Knobs::enter();
    for prefix in [
        vec![Op::Constant(0), Op::VarSet(2)],
        vec![Op::Constant(2), Op::Call(0), Op::Pop],
        vec![Op::Nil, Op::Nil, Op::Cons, Op::Pop],
    ] {
        let body = constant_loop_with_prefix(prefix, vec![]);
        assert_eq!(physical(&body, None), vec![false]);
        assert_eq!(candidates(&body, None), vec![true]);
    }
    let body = constant_loop(vec![]);
    assert_eq!(physical(&body, None), vec![true]);
    let mut body = body;
    let inner = body
        .ops
        .iter()
        .position(|op| matches!(op, Op::Sub1))
        .unwrap();
    for feedback in [NumericFeedback::Float, NumericFeedback::Other] {
        body.feedback[inner] = feedback;
        assert_eq!(physical(&body, None), vec![false]);
    }
}

#[test]
fn inline_entry_physical_admission_uses_the_actual_osr_entry_reachability() {
    let _knobs = Knobs::enter();
    let body = constant_loop_with_prefix(vec![Op::Constant(0), Op::VarSet(2)], vec![]);
    let header = body
        .caller_of_fused
        .iter()
        .position(|&original| original == 2)
        .unwrap();
    assert_eq!(physical(&body, None), vec![false]);
    assert_eq!(physical(&body, Some(header)), vec![true]);
    assert_eq!(physical(&body, Some(body.regions[0].start)), vec![false]);
}

#[test]
fn inline_entry_physical_admission_does_not_trust_snapshot_held_callees() {
    let _knobs = Knobs::enter();
    let body = fuse(
        vec![
            Op::Constant(1),
            Op::StackRef(1),
            Op::Constant(0),
            Op::Gtr,
            Op::GotoIfNil(10),
            Op::StackRef(0),
            Op::StackRef(2),
            Op::Call(1),
            Op::StackSet(2),
            Op::Goto(1),
            Op::Pop,
            Op::Return,
        ],
        vec![Value::make_int(0), step()],
    );
    let header = body
        .caller_of_fused
        .iter()
        .position(|&original| original == 1)
        .unwrap();
    assert_eq!(physical(&body, None), vec![true]);
    assert_eq!(physical(&body, Some(header)), vec![false]);
}

#[test]
fn inline_entry_physical_admission_refuses_nonconstant_nested_and_forced_regions() {
    let _knobs = Knobs::enter();
    let forward = fuse(
        vec![Op::Constant(0), Op::StackRef(1), Op::Call(1), Op::Return],
        vec![step()],
    );
    assert_eq!(physical(&forward, None), vec![false]);
    let mut body = constant_loop(vec![]);
    assert_eq!(physical(&body, None), vec![true]);
    body.regions[0].parent = Some(0);
    assert_eq!(physical(&body, None), vec![false]);
    body.regions[0].parent = None;
    body.v2.as_mut().unwrap().region_kind[0] = RegionKind::Closure {
        prefix: 0,
        const_base: 0,
    };
    assert_eq!(physical(&body, None), vec![false]);
    body.v2.as_mut().unwrap().region_kind[0] = RegionKind::Constant;
    compile::force_deopt_for_test(true);
    assert_eq!(physical(&body, None), vec![false]);
}
