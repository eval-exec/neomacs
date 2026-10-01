use super::*;
use crate::emacs_core::bytecode::ByteCodeFunction;
use crate::emacs_core::bytecode::opcode::Op;
use crate::emacs_core::eval::Context;
use crate::emacs_core::jit::NumericFeedback;
use crate::emacs_core::jit::compile::lowering::FlonumKind;
use crate::emacs_core::jit::inline::fuse_calls_v2;
use crate::emacs_core::value::{LambdaParams, Value};

fn value(i: u32) -> ClifValue {
    use cranelift_codegen::entity::EntityRef;
    ClifValue::new(i as usize)
}

fn body() -> FusedBody {
    let mut callee = ByteCodeFunction::new(LambdaParams {
        required: vec![crate::emacs_core::intern::intern("chain-plan-arg")],
        optional: vec![],
        rest: None,
    });
    callee.lexical = true;
    callee.ops = vec![Op::StackRef(0), Op::Return];
    callee.max_stack = 8;
    callee.seal_hand_assembled_ops_for_test();
    callee.jit_runtime().set_hot_for_test();
    let ops = vec![Op::Constant(0), Op::StackRef(1), Op::Call(1), Op::Return];
    fuse_calls_v2(
        &ops,
        &[Value::make_bytecode(callee)],
        None,
        1,
        &vec![NumericFeedback::FixnumOnly; ops.len()],
    )
    .unwrap()
}

fn nested_body() -> FusedBody {
    let mut body = body();
    let mut inner = body.regions[0].clone();
    body.regions[0].start = 2;
    body.regions[0].end = 5;
    body.regions[0].call_site_pc = 3;
    body.regions[0].frame_base = 2;
    inner.start = 3;
    inner.end = 4;
    inner.call_site_pc = 7;
    inner.frame_base = 4;
    inner.parent = Some(0);
    body.regions.push(inner);
    body.region_of[3] = Some(1);
    body.v2.as_mut().unwrap().callee_pc_of_fused[3] = 12;
    body
}

fn snapshots() -> Vec<RegionFrameState> {
    vec![
        RegionFrameState {
            region: 0,
            function: RelocIdx(6),
            link: Link::Bcall { nargs: 1 },
            bt: BtState::Virtual,
            binds: 0,
            handlers: 0,
            pre_call: vec![
                (value(0), SlotRep::RawFixnum),
                (value(1), SlotRep::Tagged),
                (value(2), SlotRep::RawFixnum),
            ],
        },
        RegionFrameState {
            region: 1,
            function: RelocIdx(7),
            link: Link::Bcall { nargs: 1 },
            bt: BtState::Virtual,
            binds: 0,
            handlers: 0,
            pre_call: vec![
                (value(0), SlotRep::RawFixnum),
                (value(1), SlotRep::Tagged),
                (value(3), SlotRep::RawFixnum),
                (value(4), SlotRep::Tagged),
                (value(5), SlotRep::RawFixnum),
            ],
        },
    ]
}

/// Original args stay in the physical snapshot; parameter mutation is
/// represented only by the nested caller's snapshot. Raw and flonum
/// aliases survive flattening, so the cold emitter can reconstruct once.
#[test]
fn chain_framestate_plans_nested_frames_and_shared_representations() {
    let _ev = Context::new();
    let body = nested_body();
    let states = snapshots();
    let live = [
        value(0),
        value(1),
        value(3),
        value(4),
        value(5),
        value(9),
        value(9),
    ];
    let float = SlotRep::Flonum {
        f64: value(10),
        kind: FlonumKind::Float,
    };
    let reps = [
        SlotRep::RawFixnum,
        SlotRep::Tagged,
        SlotRep::RawFixnum,
        SlotRep::Tagged,
        SlotRep::RawFixnum,
        float,
        float,
    ];
    let plan = chain_framestate(
        &body,
        3,
        &live,
        &reps,
        PhysicalFrameState {
            binds: 2,
            handlers: 0,
        },
        &states,
    )
    .unwrap();
    assert_eq!(plan.chain.frames.len(), 3);
    assert_eq!(
        plan.chain.frames.iter().map(|f| f.pc).collect::<Vec<_>>(),
        vec![3, 7, 12]
    );
    assert_eq!(
        plan.chain
            .frames
            .iter()
            .map(|f| f.stack)
            .collect::<Vec<_>>(),
        vec![
            SpillRange { start: 0, len: 3 },
            SpillRange { start: 3, len: 3 },
            SpillRange { start: 6, len: 3 }
        ]
    );
    assert_eq!(plan.chain.frames[0].binds, 2);
    assert_eq!(
        plan.chain.frames[1].kind,
        VFrameKind::Bytecode {
            func: RelocIdx(6),
            link: Link::Bcall { nargs: 1 }
        }
    );
    assert_eq!(
        plan.spill[2],
        (value(2), SlotRep::RawFixnum),
        "original physical call arg"
    );
    assert_eq!(
        plan.spill[3],
        (value(3), SlotRep::RawFixnum),
        "mutated outer callee param"
    );
    assert_eq!(
        plan.spill[7], plan.spill[8],
        "shared flonum retains one identity"
    );
}

/// Invalid ancestry, dropped handlers and a virtual frame with bindings
/// are rejected before any code generator can spill a misleading chain.
#[test]
fn chain_framestate_refuses_invalid_or_observing_frame_states() {
    let _ev = Context::new();
    let mut body = nested_body();
    let mut states = snapshots();
    let live = [value(0); 6];
    let reps = [SlotRep::Tagged; 6];
    assert!(
        chain_framestate(
            &body,
            3,
            &live,
            &reps,
            PhysicalFrameState {
                binds: 0,
                handlers: 1
            },
            &states
        )
        .is_err()
    );
    states[0].binds = 1;
    assert!(
        chain_framestate(
            &body,
            3,
            &live,
            &reps,
            PhysicalFrameState::default(),
            &states
        )
        .is_err()
    );
    states[0].binds = 0;
    states[1].bt = BtState::Materialized { spec_offset: 2 };
    assert!(
        chain_framestate(
            &body,
            3,
            &live,
            &reps,
            PhysicalFrameState::default(),
            &states
        )
        .is_err()
    );
    states[1].bt = BtState::Virtual;
    body.regions[0].parent = Some(1);
    assert!(
        chain_framestate(
            &body,
            3,
            &live,
            &reps,
            PhysicalFrameState::default(),
            &states
        )
        .is_err()
    );
}

/// Outside a region, the same helper produces the ordinary single-frame
/// format, with the inverse pc map rather than a fused instruction index.
#[test]
fn chain_framestate_maps_a_physical_only_site() {
    let _ev = Context::new();
    let body = body();
    let plan = chain_framestate(
        &body,
        0,
        &[value(0)],
        &[SlotRep::Tagged],
        PhysicalFrameState::default(),
        &[],
    )
    .unwrap();
    assert_eq!(plan.chain.frames.len(), 1);
    assert_eq!(plan.chain.frames[0].kind, VFrameKind::PhysicalBytecode);
    assert_eq!(plan.chain.frames[0].pc, 0);
    assert_eq!(plan.chain.frames[0].bt, BtState::Physical);
    assert_eq!(plan.spill, vec![(value(0), SlotRep::Tagged)]);
}
