//! X12 (P2.0 §7): the `DeoptCells` layout, the reason codes a cold block
//! stores, and the read-and-reset rule that keeps a stale `reason` or
//! `chain` from one deopt out of the next.

use super::*;
use crate::emacs_core::jit::compile::{
    DeoptCells, DeoptResume, NativeRun, force_deopt_for_test, lower_nullary_leaf,
};
use crate::emacs_core::jit::stats;
use crate::emacs_core::value::LambdaParams;

/// Every cause with each payload a reason code can carry.
fn every_cause() -> Vec<DeoptCause> {
    vec![
        DeoptCause::ArithOperands(NumericFeedback::FixnumOnly),
        DeoptCause::ArithOperands(NumericFeedback::Float),
        DeoptCause::ArithOperands(NumericFeedback::Other),
        DeoptCause::ArithOverflow,
        DeoptCause::InlinedCall,
        DeoptCause::InlineEpochMoved,
        DeoptCause::TypeError,
        DeoptCause::OsrEntry,
        DeoptCause::Rerun,
        DeoptCause::Unattributed,
        DeoptCause::EntryGuard(0),
        DeoptCause::EntryGuard(7),
        DeoptCause::EntryGuard(u8::MAX),
        DeoptCause::Unreached,
        DeoptCause::InlineIdentity,
        DeoptCause::InlineAttention,
        DeoptCause::DepthLimit,
        DeoptCause::ColdFlagged,
    ]
}

/// The golden layout: five i64 cells in declaration order, the first three
/// where they always were (AOT reaches them through per-cell addresses).
#[test]
fn deopt_cells_layout_is_pinned() {
    assert_eq!(std::mem::size_of::<DeoptCells>(), 40);
    assert_eq!(std::mem::align_of::<DeoptCells>(), 8);
    assert_eq!(std::mem::offset_of!(DeoptCells, pc), 0);
    assert_eq!(std::mem::offset_of!(DeoptCells, depth), 8);
    assert_eq!(std::mem::offset_of!(DeoptCells, handlers), 16);
    assert_eq!(std::mem::offset_of!(DeoptCells, reason), 24);
    assert_eq!(std::mem::offset_of!(DeoptCells, chain), 32);
    let cells = DeoptCells::new();
    assert_eq!(
        (
            cells.pc.get(),
            cells.depth.get(),
            cells.handlers.get(),
            cells.reason.get(),
            cells.chain.get()
        ),
        (0, 0, 0, 0, -1)
    );
    assert_eq!(DeoptCells::NO_REASON, 0);
    assert_eq!(DeoptCells::SINGLE_FRAME, -1);
}

/// Every cause round-trips through its code; codes are distinct and never
/// the unset value; words no cause produces decode to nothing.
#[test]
fn reason_codes_round_trip_and_reject_garbage() {
    let causes = every_cause();
    let mut codes: Vec<i64> = causes.iter().map(|c| c.reason_code()).collect();
    for (&cause, &code) in causes.iter().zip(&codes) {
        assert_ne!(code, DeoptCells::NO_REASON, "{cause:?}");
        assert_eq!(DeoptCause::from_reason_code(code), Some(cause), "{code:#x}");
    }
    codes.sort_unstable();
    codes.dedup();
    assert_eq!(codes.len(), causes.len(), "two causes share a code");
    for garbage in [
        DeoptCells::NO_REASON,
        -1,
        15,
        0xff,
        // A payload on a kind that carries none.
        DeoptCause::ArithOverflow.reason_code() | 1 << 8,
        DeoptCause::ColdFlagged.reason_code() | 3 << 8,
        // A NumericFeedback byte that does not exist.
        DeoptCause::ArithOperands(NumericFeedback::Float).reason_code() | 3 << 8,
        1 << 16,
        i64::MIN,
    ] {
        assert_eq!(DeoptCause::from_reason_code(garbage), None, "{garbage:#x}");
    }
}

/// `max` of a float and a fixnum: a JIT leaf that deopts precisely at pc 2.
fn max_leaf() -> crate::emacs_core::jit::compile::CompiledLeaf {
    lower_nullary_leaf(
        &[Op::Constant(0), Op::Constant(1), Op::Max, Op::Return],
        &[Value::make_float(1.5), Value::make_int(7)],
    )
    .expect("compiles")
}

fn deopt(leaf: &crate::emacs_core::jit::compile::CompiledLeaf, ev: &mut Context) -> DeoptResume {
    match leaf.call(ev as *mut Context as *mut u8, &[]) {
        NativeRun::DeoptAt(resume) => *resume,
        other => panic!("expected a precise deopt, got {other:?}"),
    }
}

/// X12: in one leaf, a chain deopt followed by a single-frame deopt. The
/// first reads the chain and cause its cold block stored (simulated here:
/// no producer exists yet); the second, whose block stores neither, reads
/// them unset rather than inheriting the first's.
#[test]
fn a_chain_deopt_does_not_leak_into_the_next_single_frame_deopt() {
    force_deopt_for_test(false);
    let mut ev = Context::new();
    let leaf = max_leaf();

    leaf.deopt_meta.chain.set(2);
    leaf.deopt_meta
        .reason
        .set(DeoptCause::InlineIdentity.reason_code());
    let first = deopt(&leaf, &mut ev);
    assert_eq!(first.pc, 2);
    assert_eq!(first.chain, Some(2));
    assert_eq!(first.cause, Some(DeoptCause::InlineIdentity));
    assert_eq!(
        (leaf.deopt_meta.reason.get(), leaf.deopt_meta.chain.get()),
        (DeoptCells::NO_REASON, DeoptCells::SINGLE_FRAME),
        "the read resets both cells"
    );

    let second = deopt(&leaf, &mut ev);
    assert_eq!(second.pc, 2);
    assert_eq!(
        second.chain, None,
        "a stale chain made a single frame a chain"
    );
    assert_eq!(second.cause, None);
    assert_eq!(second.stack, first.stack);
}

/// A plain JIT deopt stores nothing: it reads unset and the hook classifies
/// it from the op, exactly as before the cells existed.
#[test]
fn an_ordinary_deopt_reads_the_cells_unset() {
    force_deopt_for_test(false);
    let mut ev = Context::new();
    let leaf = max_leaf();
    let resume = deopt(&leaf, &mut ev);
    assert_eq!((resume.cause, resume.chain), (None, None));
}

fn census() -> [u64; stats::DEOPT_CAUSES] {
    stats::compile_stats_snapshot().deopt_causes
}

/// The hook takes the stored cause over its own classification, and a
/// cause no policy knows yet changes nothing (T0.1 is layout only).
#[test]
fn note_deopt_prefers_the_stored_cause() {
    force_deopt_for_test(false);
    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context;
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: Vec::new(),
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = vec![Op::Constant(0), Op::Constant(1), Op::Max, Op::Return];
    f.constants = vec![Value::make_float(1.5), Value::make_int(7)].into();
    f.max_stack = 4;
    f.seal_hand_assembled_ops();
    let leaf = max_leaf();
    let resume = deopt(&leaf, &mut ev);
    let stack = resume.stack.clone();
    let precise = |cause| DeoptEvent::Precise {
        pc: resume.pc,
        stack: &stack,
        cause,
    };
    let unreached = DeoptCause::Unreached.census_index();
    let float = DeoptCause::ArithOperands(NumericFeedback::Float).census_index();

    let before = census();
    let verdict = note_deopt(
        ctx,
        &f,
        &leaf,
        LeafOrigin::Entry,
        precise(Some(DeoptCause::Unreached)),
    );
    let after = census();
    assert_eq!(verdict, ReoptVerdict::Kept);
    assert_eq!(after[unreached] - before[unreached], 1);
    assert_eq!(
        after[float], before[float],
        "the stored cause was reclassified"
    );
    assert_eq!(
        f.jit_runtime().numeric_feedback(2),
        NumericFeedback::FixnumOnly,
        "an unknown cause widened feedback"
    );

    // No stored cause: the op classifies it, as before.
    let before = census();
    let _ = note_deopt(ctx, &f, &leaf, LeafOrigin::Entry, precise(None));
    let after = census();
    assert_eq!(after[float] - before[float], 1);
    assert_eq!(after[unreached], before[unreached]);
}
