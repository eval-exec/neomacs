//! Final-slice size policy, with compiler-only invocation-owned settings.
use super::frontend_tests::Settings;
use super::*;
use crate::emacs_core::jit::compile::force_opt_max_ops_for_test;

#[test]
fn opt_profit_final_size_disabled_cap_allows_selected_expansion() {
    let _settings = Settings::enter();
    force_opt_max_ops_for_test(Some(0));
    for front in [FrontChoice::Selected, FrontChoice::SelectedAfterMir] {
        assert!(final_size_admitted(front, 4096));
    }
    assert!(osr_admitted(&[Op::Add1, Op::Goto(0)], &[], 4096));
}

#[test]
fn opt_profit_final_size_rejects_selected_expansion_inclusively() {
    let _settings = Settings::enter();
    force_opt_max_ops_for_test(Some(64));
    for front in [FrontChoice::Selected, FrontChoice::SelectedAfterMir] {
        assert!(final_size_admitted(front, 48), "original source fits");
        assert!(final_size_admitted(front, 64), "bound is inclusive");
        assert!(
            !final_size_admitted(front, 65),
            "expanded slice exceeds cap"
        );
        assert!(!final_size_admitted(front, 4096));
    }
    assert!(osr_admitted(&[Op::Add1, Op::Goto(0)], &[], 64));
    assert!(!osr_admitted(&[Op::Add1, Op::Goto(0)], &[], 65));
}

#[test]
fn opt_profit_final_size_keeps_unselected_legacy_and_current_unchanged() {
    let _settings = Settings::enter();
    force_opt_max_ops_for_test(Some(1));
    assert!(final_size_admitted(FrontChoice::Legacy, 4096));
    // PROFIT=off and OPT=legacy/off select Current before this seam.
    assert!(final_size_admitted(FrontChoice::Current, 4096));
}
