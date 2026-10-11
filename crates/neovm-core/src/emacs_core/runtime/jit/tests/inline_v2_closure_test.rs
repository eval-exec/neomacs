//! In-unit closure provenance and its chain-emission contract. These front
//! tests exercise CFG joins and mutated function slots without running native
//! code; native capture/deopt parity is tested by the runtime lane.

use super::*;
use crate::emacs_core::jit::compile::{self, Inline2Mode};
use crate::emacs_core::value::LambdaParams;

fn lexical(required: usize, ops: Vec<Op>, constants: Vec<Value>) -> ByteCodeFunction {
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: (0..required)
            .map(|i| crate::emacs_core::intern::intern(&format!("v2-closure-arg-{i}")))
            .collect(),
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = ops;
    f.constants = constants.into();
    f.max_stack = crate::emacs_core::bytecode::StackDepth::for_test(16);
    f.seal_hand_assembled_ops_for_test();
    f.jit_runtime().set_hot_for_test();
    f
}

fn template() -> Value {
    Value::make_bytecode(lexical(
        1,
        vec![Op::StackRef(0), Op::Constant(0), Op::Add, Op::Return],
        vec![Value::symbol("V0")],
    ))
}

fn caller(proto: Value, tail: impl IntoIterator<Item = Op>) -> ByteCodeFunction {
    let mut ops = vec![
        Op::Constant(0),
        Op::Constant(1),
        Op::Constant(2),
        Op::Call(2),
    ];
    ops.extend(tail);
    lexical(
        1,
        ops,
        vec![Value::symbol("make-closure"), proto, Value::make_int(7)],
    )
}

fn fuse(f: &ByteCodeFunction, mode: Inline2Mode) -> Option<FusedBody> {
    force_inline_for_test(Some(true));
    compile::force_inline2_for_test(Some(mode));
    let fused = fuse_calls_v2(
        f.executable_ops(),
        &f.constants,
        None,
        f.params
            .stack_shape()
            .expect("fixture stack parameters")
            .required(),
        &vec![NumericFeedback::FixnumOnly; f.executable_ops().len()],
    );
    compile::force_inline2_for_test(None);
    force_inline_for_test(None);
    fused
}

#[test]
fn inline_v2_in_unit_closure_retains_object_and_dynamic_prefix_contract() {
    let proto = template();
    let f = caller(proto, [Op::StackRef(1), Op::Call(1), Op::Return]);
    let fused = fuse(&f, Inline2Mode::Closure).expect("in-unit closure admitted");
    assert_eq!(fused.regions.len(), 1);
    let region = &fused.regions[0];
    assert_eq!(region.call_site_pc, 5);
    assert_eq!(region.frame_base, 2, "closure object is at fused slot 1");
    assert_eq!(region.callee_bits, proto.bits() as u64);
    assert_eq!(
        fused.v2.as_ref().unwrap().region_kind.as_ref(),
        &[RegionKind::Closure {
            prefix: 1,
            const_base: f.constants.len(),
        }]
    );
    assert_eq!(
        fused.ops[3],
        Op::Call(2),
        "closure creation still allocates"
    );
    assert_eq!(
        fused.ops[region.start + 1],
        Op::Constant(f.constants.len() as u16),
        "emission recognizes the rebased prefix Constant"
    );
    assert_eq!(fused.constants[1], proto, "prototype remains rooted");
}

#[test]
fn inline_v2_closure_provenance_survives_agreeing_cfg_join() {
    let f = caller(
        template(),
        [
            Op::StackRef(1),
            Op::GotoIfNil(8),
            Op::Nil,
            Op::Pop,
            Op::StackRef(1),
            Op::Call(1),
            Op::Return,
        ],
    );
    let fused = fuse(&f, Inline2Mode::All).expect("both edges preserve the closure");
    assert_eq!(fused.regions[0].call_site_pc, 9);
    assert!(matches!(
        fused.v2.as_ref().unwrap().region_kind[0],
        RegionKind::Closure { prefix: 1, .. }
    ));
}

#[test]
fn inline_v2_closure_provenance_is_lost_when_cfg_edges_disagree() {
    let f = caller(
        template(),
        [
            Op::StackRef(1),
            Op::GotoIfNil(8),
            Op::StackRef(1),
            Op::StackSet(1),
            Op::StackRef(1),
            Op::Call(1),
            Op::Return,
        ],
    );
    assert!(fuse(&f, Inline2Mode::Closure).is_none());
}

#[test]
fn inline_v2_closure_producer_is_scoped_to_closure_and_all_modes() {
    let f = caller(template(), [Op::StackRef(1), Op::Call(1), Op::Return]);
    for mode in [Inline2Mode::Off, Inline2Mode::Named, Inline2Mode::Hof] {
        assert!(fuse(&f, mode).is_none(), "{mode:?}");
    }
    for mode in [Inline2Mode::Closure, Inline2Mode::All] {
        assert!(fuse(&f, mode).is_some(), "{mode:?}");
    }
}

#[test]
fn inline_v2_closure_uses_the_widest_known_source_prefix() {
    let proto = Value::make_bytecode(lexical(
        1,
        vec![Op::Constant(1), Op::Return],
        vec![Value::symbol("V0"), Value::symbol("V1")],
    ));
    proto
        .get_bytecode_data()
        .unwrap()
        .jit_runtime()
        .note_patched_prefix(2);
    let f = caller(proto, [Op::StackRef(1), Op::Call(1), Op::Return]);
    let fused = fuse(&f, Inline2Mode::Closure).unwrap();
    assert_eq!(
        fused.v2.as_ref().unwrap().region_kind[0],
        RegionKind::Closure {
            prefix: 2,
            const_base: f.constants.len(),
        }
    );
}

#[test]
fn inline_v2_closure_front_admits_only_guarded_cons_stores() {
    for store in [Op::Setcar, Op::Setcdr] {
        let proto = Value::make_bytecode(lexical(
            1,
            vec![Op::Constant(0), Op::StackRef(1), store, Op::Return],
            vec![Value::symbol("V0")],
        ));
        let f = caller(proto, [Op::StackRef(1), Op::Call(1), Op::Return]);
        assert!(fuse(&f, Inline2Mode::Closure).is_some());
    }
    let proto = Value::make_bytecode(lexical(
        1,
        vec![Op::Constant(0), Op::StackRef(1), Op::Cons, Op::Return],
        vec![Value::symbol("V0")],
    ));
    let f = caller(proto, [Op::StackRef(1), Op::Call(1), Op::Return]);
    assert!(
        fuse(&f, Inline2Mode::Closure).is_none(),
        "allocation stays out"
    );
}
