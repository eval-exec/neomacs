//! A fresh runtime context takes the process's `NEOVM_TIER_I`, so a whole
//! suite run under the knob (the soak) exercises Tier-I everywhere. Re-homed
//! from eval's tier_i/tests/knobs_test.rs; the context-free knob-parser tests
//! stay in eval with that file. API drift: the private Tier-I mode/counter
//! reads go through the signed test-support fences.

use crate::emacs_core::eval::{TierIEvent, TierIMode, parse_tier_i_knob};
use neovm_eval::test_support::authority::{tier_i_mode_for_test, tier_i_stats_for_test};

#[test]
fn a_fresh_context_takes_the_process_knob() {
    crate::test_utils::init_test_tracing();
    let expected = parse_tier_i_knob(std::env::var("NEOVM_TIER_I").ok().as_deref());
    let mut eval = crate::test_utils::runtime_startup_context();
    assert_eq!(tier_i_mode_for_test(&eval), expected);
    eval.eval_str("(progn (defun ti-knob-probe (x) (list x)) (ti-knob-probe 1) (ti-knob-probe 2))")
        .expect("evaluates");
    if expected == TierIMode::On || expected == TierIMode::Verify {
        assert!(
            tier_i_stats_for_test(&eval).count(TierIEvent::Run) > 0,
            "{}",
            tier_i_stats_for_test(&eval).report()
        );
    }
}
