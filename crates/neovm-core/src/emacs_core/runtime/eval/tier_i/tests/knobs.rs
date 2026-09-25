//! The `NEOVM_TIER_I` and `NEOVM_TIER_I_THRESHOLD` knobs.

use crate::emacs_core::eval::{
    TierIMode, parse_tier_i_knob, parse_tier_i_lazy_frames, parse_tier_i_threshold,
};

#[test]
fn mode_values() {
    crate::test_utils::init_test_tracing();
    assert_eq!(parse_tier_i_knob(None), TierIMode::Off);
    for off in ["", "0", "off", "OFF", "no", "false", "bogus"] {
        assert_eq!(parse_tier_i_knob(Some(off)), TierIMode::Off, "{off}");
    }
    assert_eq!(parse_tier_i_knob(Some("census")), TierIMode::Census);
    assert_eq!(parse_tier_i_knob(Some(" Analyze ")), TierIMode::Analyze);
    for on in ["1", "on", "yes", "true"] {
        assert_eq!(parse_tier_i_knob(Some(on)), TierIMode::On, "{on}");
    }
    assert_eq!(parse_tier_i_knob(Some("verify")), TierIMode::Verify);
}

#[test]
fn threshold_values() {
    crate::test_utils::init_test_tracing();
    assert_eq!(parse_tier_i_threshold(None), 2);
    assert_eq!(parse_tier_i_threshold(Some("")), 2);
    assert_eq!(parse_tier_i_threshold(Some("0")), 1);
    assert_eq!(parse_tier_i_threshold(Some(" 64 ")), 64);
    assert_eq!(parse_tier_i_threshold(Some("many")), 2);
}

#[test]
fn lazy_frames_values() {
    crate::test_utils::init_test_tracing();
    assert!(parse_tier_i_lazy_frames(None));
    assert!(parse_tier_i_lazy_frames(Some("on")));
    assert!(parse_tier_i_lazy_frames(Some("1")));
    for off in ["0", "off", "OFF", "no", "false"] {
        assert!(!parse_tier_i_lazy_frames(Some(off)), "{off}");
    }
}

/// A fresh runtime context takes the process's `NEOVM_TIER_I`, so a whole
/// suite run under the knob (the soak) exercises Tier-I everywhere.
#[test]
fn a_fresh_context_takes_the_process_knob() {
    crate::test_utils::init_test_tracing();
    let expected = parse_tier_i_knob(std::env::var("NEOVM_TIER_I").ok().as_deref());
    let mut eval = crate::test_utils::runtime_startup_context();
    assert_eq!(eval.tier_i.mode(), expected);
    eval.eval_str("(progn (defun ti-knob-probe (x) (list x)) (ti-knob-probe 1) (ti-knob-probe 2))")
        .expect("evaluates");
    if expected == TierIMode::On || expected == TierIMode::Verify {
        use crate::emacs_core::eval::TierIEvent;
        assert!(
            eval.tier_i.stats().count(TierIEvent::Run) > 0,
            "{}",
            eval.tier_i.stats().report()
        );
    }
}
