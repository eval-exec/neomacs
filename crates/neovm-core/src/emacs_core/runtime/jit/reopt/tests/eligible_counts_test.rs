//! Semantic exits must remain in the census and source history while leaving
//! the speculation policy's counts untouched. The hooks below model readback
//! from cold guards; no native producer is needed to expose the inherited bug.

use super::*;
use crate::emacs_core::jit::{cache, compile, stats};
use crate::emacs_core::value::LambdaParams;

const SEMANTIC: [DeoptCause; 4] = [
    DeoptCause::InlineAttention,
    DeoptCause::DepthLimit,
    DeoptCause::ColdFlagged,
    DeoptCause::TypeError,
];

fn function(ops: Vec<Op>, constants: Vec<Value>) -> ByteCodeFunction {
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: Vec::new(),
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = ops;
    f.constants = constants.into();
    f.max_stack = 4;
    f.seal_hand_assembled_ops();
    f
}

fn setup() {
    compile::force_profit_gate_for_test(false);
    compile::force_deopt_for_test(false);
    force_reopt_for_test(Some(ReoptKnobs {
        site_limit: 3,
        ..ReoptKnobs::stress()
    }));
}

fn cached_leaf(f: &ByteCodeFunction) -> &CompiledLeaf {
    let id = cache::compile_and_cache_jit_leaf(f, None).expect("compiled leaf");
    let ptr = cache::compiled_leaf_ptr_for_test(id).expect("cached leaf");
    // No safepoint occurs in these hooks; retirement retains the allocation.
    unsafe { &*ptr }
}

fn readback(
    f: &ByteCodeFunction,
    leaf: &CompiledLeaf,
    pc: usize,
    cause: DeoptCause,
) -> ReoptVerdict {
    leaf.obs.note_deopt_at(pc as u32);
    note_deopt(
        std::ptr::null(),
        f,
        leaf,
        LeafOrigin::Entry,
        DeoptEvent::Precise {
            pc,
            stack: &[],
            cause: Some(cause),
        },
    )
}

#[test]
fn semantic_deopts_do_not_accelerate_overflow_retirement() {
    setup();
    let f = function(
        vec![Op::Constant(0), Op::Add1, Op::Return],
        vec![Value::make_int(1)],
    );
    let leaf = cached_leaf(&f);
    let before = stats::compile_stats_snapshot().deopt_causes;
    for cause in SEMANTIC {
        for _ in 0..3 {
            assert_eq!(readback(&f, leaf, 1, cause), ReoptVerdict::Kept);
        }
    }
    let after = stats::compile_stats_snapshot().deopt_causes;
    for cause in SEMANTIC {
        let bucket = cause.census_index();
        assert_eq!(after[bucket] - before[bucket], 3, "{cause:?}");
    }
    assert_eq!(leaf.obs.deopt_count_at(1), 12, "all exits remain counted");
    assert_eq!(f.jit_runtime().deopt_history(1), 12);
    assert_eq!(leaf.obs.reopt_deopt_count_at(1), 0);
    for count in 1..=3 {
        assert_eq!(
            readback(&f, leaf, 1, DeoptCause::ArithOverflow),
            if count == 3 {
                ReoptVerdict::Invalidated
            } else {
                ReoptVerdict::Kept
            }
        );
        assert_eq!(leaf.obs.reopt_deopt_count_at(1), count);
        assert_eq!(
            f.jit_runtime().numeric_feedback(1),
            if count == 3 {
                NumericFeedback::Other
            } else {
                NumericFeedback::FixnumOnly
            }
        );
    }
    assert_eq!(leaf.obs.deopt_count_at(1), 15);
    assert_eq!(f.jit_runtime().deopt_history(1), 15);
    assert_eq!(f.jit_runtime().reopt_count(), 1);
    force_reopt_for_test(None);
}

#[test]
fn semantic_chain_deopts_do_not_accelerate_inline_guard_retirement() {
    setup();
    let physical = function(vec![Op::Constant(0), Op::Return], vec![Value::make_int(1)]);
    let inner = function(
        vec![Op::Constant(0), Op::Constant(1), Op::Call(1), Op::Return],
        vec![Value::symbol("identity"), Value::make_int(1)],
    );
    let leaf = cached_leaf(&physical);
    let readback = |cause| {
        leaf.obs.note_deopt_at(0);
        note_deopt_chain(
            std::ptr::null(),
            &physical,
            &inner,
            leaf,
            0,
            DeoptEvent::Precise {
                pc: 2,
                stack: &[],
                cause: Some(cause),
            },
        )
    };
    for cause in SEMANTIC {
        for _ in 0..3 {
            assert_eq!(readback(cause), ReoptVerdict::Kept);
        }
    }
    for count in 1..=3 {
        assert_eq!(
            readback(DeoptCause::InlinedCall),
            if count == 3 {
                ReoptVerdict::Invalidated
            } else {
                ReoptVerdict::Kept
            }
        );
        assert_eq!(inner.jit_runtime().call_site_no_inline(2), count == 3);
        assert_eq!(leaf.obs.reopt_deopt_count_at(0), count);
        assert_eq!(leaf.obs.reopt_deopt_count_at(2), 0);
    }
    assert!(leaf.retired.get(), "the physical activation is retired");
    assert!(!physical.jit_runtime().call_site_no_inline(0));
    assert_eq!(inner.jit_runtime().deopt_history(2), 15);
    force_reopt_for_test(None);
}

#[test]
fn semantic_deopts_do_not_skip_first_osr_feedback_reopen() {
    setup();
    let f = function(
        vec![Op::Constant(0), Op::Add1, Op::Return],
        vec![Value::make_int(1)],
    );
    let leaf = cached_leaf(&f);
    for cause in SEMANTIC {
        for _ in 0..3 {
            assert_eq!(readback(&f, leaf, 1, cause), ReoptVerdict::Kept);
        }
    }
    f.jit_runtime().note_numeric_feedback_consumed();
    assert!(!f.jit_runtime().wants_numeric_feedback());
    leaf.obs.note_deopt_at(1);
    assert_eq!(
        note_deopt(
            std::ptr::null(),
            &f,
            leaf,
            LeafOrigin::Osr {
                header_pc: 1,
                snapshot: &[],
            },
            DeoptEvent::Precise {
                pc: 1,
                stack: &[],
                cause: Some(DeoptCause::OsrEntry),
            }
        ),
        ReoptVerdict::Kept,
        "the first eligible refusal is below the policy limit"
    );
    assert!(
        f.jit_runtime().wants_numeric_feedback(),
        "the first eligible refusal reopens the interpreter's profile"
    );
    assert_eq!(leaf.obs.reopt_deopt_count_at(1), 1);
    assert_eq!(f.jit_runtime().reopt_count(), 0);
    force_reopt_for_test(None);
}

#[test]
fn semantic_pc_overflow_does_not_consume_policy_pc_slots() {
    setup();
    let mut ops = Vec::new();
    for _ in 0..12 {
        ops.extend([Op::Nil, Op::Pop]);
    }
    ops.extend([Op::Constant(0), Op::Add1, Op::Return]);
    let f = function(ops, vec![Value::make_int(1)]);
    let leaf = cached_leaf(&f);
    for pc in 0..12 {
        assert_eq!(
            readback(&f, leaf, pc, DeoptCause::InlineAttention),
            ReoptVerdict::Kept
        );
    }
    // The census has eight slots, so its overflow already reaches the limit.
    assert_eq!(leaf.obs.deopt_count_at(25), 4);
    for count in 1..=3 {
        assert_eq!(
            readback(&f, leaf, 25, DeoptCause::Unattributed),
            if count == 3 {
                ReoptVerdict::Invalidated
            } else {
                ReoptVerdict::Kept
            }
        );
        assert_eq!(leaf.obs.reopt_deopt_count_at(25), count);
    }
    assert_eq!(f.jit_runtime().reopt_level(), ReoptLevel::NoInline);
    force_reopt_for_test(None);
}
