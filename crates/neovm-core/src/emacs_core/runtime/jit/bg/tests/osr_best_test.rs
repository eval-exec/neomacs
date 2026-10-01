//! T2.3 before opt IR exists: hot OSR uses Full baseline lowering with
//! recorded source-call feedback directly, without a T1 countdown. B10's
//! pending/latch integration is covered by osr_pending_test.rs.

use super::*;
use crate::emacs_core::bytecode::Op;
use crate::emacs_core::eval::Context;
use crate::emacs_core::jit::cache::{self, OsrProbe};
use crate::emacs_core::jit::compile::compile_pipeline_tests::function;
use crate::emacs_core::jit::compile::lowering::RegallocChoice;
use crate::emacs_core::jit::compile::{
    SpecSlotKind, Tier2Knob, force_deopt_for_test, force_spec_sources_for_test,
    force_tier2_for_test,
};
use crate::emacs_core::jit::feedback::{FeedbackMode, force_feedback_mode_for_test};
use crate::emacs_core::jit::stats::{ObserveOverride, force_observe_for_test};
use crate::emacs_core::jit::tier2::{DISARMED, T2Origin};
use crate::emacs_core::value::Value;

#[test]
fn jit_bg_osr_uses_full_feedback_backend_without_a_countdown() {
    force_mode_for_test(Some(BgMode::Sync));
    force_deopt_for_test(false);
    force_feedback_mode_for_test(Some(FeedbackMode::Use));
    force_spec_sources_for_test(Some(true));
    force_observe_for_test(ObserveOverride {
        entry_count: true,
        ..Default::default()
    });
    let mut served = Vec::new();
    for on in [false, true] {
        let mut knob = Tier2Knob::from_env(|_| None);
        knob.on = on;
        knob.window = 1;
        force_tier2_for_test(Some(knob));
        let mut ctx = Context::new();
        let callback =
            Value::make_bytecode(function(vec![Op::StackRef(0), Op::Return], Vec::new(), 1));
        // (lambda (f n) (let ((i 0)) (while (< i n) (funcall f i)
        //                         (setq i (1+ i))) i))
        let f = function(
            vec![
                Op::Constant(0),
                Op::StackRef(0),
                Op::StackRef(2),
                Op::Lss,
                Op::GotoIfNil(13),
                Op::StackRef(2),
                Op::StackRef(1),
                Op::Call(1),
                Op::Pop,
                Op::StackRef(0),
                Op::Add1,
                Op::StackSet(1),
                Op::Goto(1),
                Op::Return,
            ],
            vec![Value::make_int(0)],
            2,
        );
        f.jit_runtime()
            .call_sites_for(f.executable_ops(), &f.constants)
            .site_at(7)
            .expect("callback feedback")
            .observe(callback);
        let before = cache::OSR_TRANSFER_COUNT.load(std::sync::atomic::Ordering::Relaxed);
        let result = cache::try_run_osr_probe(
            &mut ctx,
            &f,
            1,
            &[callback, Value::make_int(1024), Value::make_int(0)],
            &[],
        );
        assert!(
            matches!(result, OsrProbe::Ran(crate::emacs_core::jit::compile::NativeRun::Ok(bits)) if bits == Value::make_int(1024).bits())
        );
        let transfers =
            cache::OSR_TRANSFER_COUNT.load(std::sync::atomic::Ordering::Relaxed) - before;
        assert_eq!(transfers, 1);
        let ptr = cache::osr_leaf_ptr_for_test(&f, 1).expect("installed OSR leaf");
        // SAFETY: this mutator's OSR cache keeps the leaf alive until clear;
        // no eviction or clear occurs while this test inspects it.
        let leaf = unsafe { &*ptr };
        assert_eq!(leaf.regalloc, RegallocChoice::Full);
        assert_eq!(leaf.obs.t2.origin, T2Origin::Unprofiled);
        assert_eq!(leaf.obs.t2.budget.get(), DISARMED);
        assert_eq!(
            leaf.spec_slot_kinds
                .iter()
                .filter(|kind| **kind == SpecSlotKind::Source)
                .count(),
            1,
            "OSR consumed recorded source feedback directly"
        );
        let polls = leaf.obs.t2.polls.get();
        assert!(polls > 0, "OSR still uses the quit poll");
        served.push((transfers, polls));
    }
    assert_eq!(
        served[0], served[1],
        "the knob preserves OSR transfers and quit-poll cadence"
    );
    force_spec_sources_for_test(None);
    force_feedback_mode_for_test(None);
    force_tier2_for_test(None);
    force_mode_for_test(None);
}
