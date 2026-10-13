use super::*;
use crate::emacs_core::bytecode::{ByteCodeFunction, Op};
use crate::emacs_core::eval::Context;
use crate::emacs_core::intern::intern;
use crate::emacs_core::jit::NumericFeedback;
use crate::emacs_core::jit::inline::{force_inline_for_test, fuse_calls_v2};
use crate::emacs_core::value::{LambdaParams, Value};

fn value(index: usize) -> ClifValue {
    use cranelift_codegen::entity::EntityRef;
    ClifValue::new(index)
}

fn nested() -> FusedBody {
    force_inline_for_test(Some(true));
    let mut callee = ByteCodeFunction::new(LambdaParams {
        required: vec![intern("named-chain-entry-arg")],
        optional: Vec::new(),
        rest: None,
    });
    callee.lexical = true;
    callee.ops = vec![Op::StackRef(0), Op::Return];
    callee.max_stack = crate::emacs_core::bytecode::StackDepth::for_test(8);
    callee.seal_hand_assembled_ops_for_test();
    callee.jit_runtime().set_hot_for_test();
    let ops = [Op::Constant(0), Op::StackRef(1), Op::Call(1), Op::Return];
    let mut fused = fuse_calls_v2(
        &ops,
        &[Value::make_bytecode(callee)],
        None,
        1,
        &[NumericFeedback::FixnumOnly; 4],
    )
    .unwrap();
    force_inline_for_test(None);
    let mut inner = fused.regions[0].clone();
    fused.regions[0].start = 2;
    fused.regions[0].end = 5;
    fused.regions[0].call_site_pc = 3;
    fused.regions[0].frame_base = 2;
    inner.start = 3;
    inner.end = 4;
    inner.call_site_pc = 7;
    inner.frame_base = 4;
    inner.parent = Some(0);
    fused.regions.push(inner);
    fused.region_of[3] = Some(1);
    fused.v2.as_mut().unwrap().callee_pc_of_fused[3] = 0;
    fused
}

#[test]
fn named_chain_entry_guard_resumes_parent_call_without_unborn_child() {
    let _ctx = Context::new();
    let fused = nested();
    let state = RegionFrameState {
        region: 0,
        function: RelocIdx(4),
        link: Link::Bcall { nargs: 1 },
        bt: BtState::Virtual,
        binds: 0,
        handlers: 0,
        pre_call: vec![
            (value(0), SlotRep::Tagged),
            (value(1), SlotRep::Tagged),
            (value(2), SlotRep::RawFixnum),
        ],
    };
    let live = [value(0), value(1), value(9), value(10), value(11)];
    let reps = [SlotRep::Tagged; 5];
    let plan = chain_framestate_at(
        &fused,
        3,
        &live,
        &reps,
        PhysicalFrameState::default(),
        &[state],
        Some((0, 7)),
    )
    .unwrap();
    assert_eq!(plan.chain.frames.len(), 2);
    assert_eq!(plan.chain.frames[0].pc, 3);
    assert_eq!(plan.chain.frames[1].pc, 7);
    assert_eq!(plan.chain.frames[1].bt, BtState::Virtual);
    assert_eq!(plan.chain.frames[1].stack, SpillRange { start: 3, len: 3 });
    assert_eq!(plan.spill[2], (value(2), SlotRep::RawFixnum));
    assert_eq!(
        plan.spill[3..],
        live[2..]
            .iter()
            .copied()
            .map(|v| (v, SlotRep::Tagged))
            .collect::<Vec<_>>()
    );
}

#[test]
fn named_chain_entry_guard_preserves_materialized_parent_state() {
    let _ctx = Context::new();
    let fused = nested();
    let state = RegionFrameState {
        region: 0,
        function: RelocIdx(4),
        link: Link::Bcall { nargs: 1 },
        bt: BtState::Materialized { spec_offset: 2 },
        binds: 0,
        handlers: 0,
        pre_call: vec![(value(0), SlotRep::Tagged); 3],
    };
    let live = [value(0); 5];
    let plan = chain_framestate_at(
        &fused,
        3,
        &live,
        &[SlotRep::Tagged; 5],
        PhysicalFrameState {
            binds: 2,
            handlers: 0,
        },
        &[state],
        Some((0, 7)),
    )
    .unwrap();
    assert_eq!(plan.chain.frames.len(), 2);
    assert_eq!(plan.chain.frames[0].binds, 2);
    assert_eq!(
        plan.chain.frames[1].bt,
        BtState::Materialized { spec_offset: 2 }
    );
}
