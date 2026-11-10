//! Lists48 OSR aset on the runtime-startup image.
//!
//! The profile guard publishes the preset and restores the surrounding stack.
//! Advice installation and the compared words stay in this test.

use neovm_eval::test_support::authority::{
    enter_lists48_osr_profile_for_test, lists48_osr_aset_advice_obs_for_test, opt_max_ops_for_test,
    opt_passes_flags_for_test, opt_profit_name_for_test, opt_require_osr_for_test,
};

#[test]
fn opt_profit_lists48_osr_aset_ignores_collecting_advice_and_preserves_retained_alias() {
    let _profile = enter_lists48_osr_profile_for_test();
    let mut ctx = crate::test_utils::runtime_startup_context();
    ctx.eval_str(
        r#"(progn
          (require 'nadvice)
          (defvar opt-profit--aset-advice-calls 0)
          (advice-add 'aset :around
            (lambda (&rest _)
              (setq opt-profit--aset-advice-calls
                    (1+ opt-profit--aset-advice-calls))
              (garbage-collect)
              'overridden)))"#,
    )
    .expect("collecting advice installed on the genuine function cell");
    assert_eq!(opt_max_ops_for_test(), 48);
    assert_eq!(opt_profit_name_for_test(), "PrimitiveLoops");
    assert!(opt_require_osr_for_test());
    assert_eq!(
        opt_passes_flags_for_test(),
        (true, true, true, false, true, false, false, false)
    );

    let (
        op_len,
        osr_ok,
        osr_tier,
        osr_vector,
        osr_pair,
        normal_ok,
        normal_tier,
        normal_vector,
        normal_pair,
        advice_native,
        root_top,
        genuine,
        advice_genuine,
    ) = lists48_osr_aset_advice_obs_for_test(&mut ctx);
    assert!(op_len <= 48);
    assert!(osr_ok);
    assert_eq!(osr_tier, "opt");
    assert!(osr_vector);
    assert!(osr_pair);
    assert!(normal_ok);
    assert_eq!(normal_tier, "opt");
    assert!(normal_vector);
    assert!(normal_pair);
    assert_eq!(
        advice_native,
        Some(0),
        "GNU Baset never enters collecting advice"
    );
    assert_eq!(root_top, 0);
    assert_eq!(
        genuine, "overridden",
        "a genuine call proves that collecting advice remains installed"
    );
    assert_eq!(advice_genuine, Some(1));
}
