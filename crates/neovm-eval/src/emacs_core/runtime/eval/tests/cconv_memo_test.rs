//! P4.1 Stage 0 pins: the `cconv-make-interpreted-closure` knob parsers.
//!
//! The two CONTEXT-FREE parser tests stay in eval (census v3 boot_kind empty;
//! they never construct a Context). The 29 BOOT-FULL tests live in
//! moved_eval/emacs_core/runtime/eval/tests/cconv_memo_test.rs.

use crate::emacs_core::eval::{CconvMemoMode, parse_cconv_fast_knob, parse_cconv_memo_knob};

#[test]
fn knob_values() {
    crate::test_utils::init_test_tracing();
    assert_eq!(parse_cconv_memo_knob(None), CconvMemoMode::On);
    for off in ["", "0", "off", "OFF", "no", "false"] {
        assert_eq!(
            parse_cconv_memo_knob(Some(off)),
            CconvMemoMode::Off,
            "{off}"
        );
    }
    assert_eq!(parse_cconv_memo_knob(Some("stats")), CconvMemoMode::Stats);
    assert_eq!(parse_cconv_memo_knob(Some(" Stats ")), CconvMemoMode::Stats);
    assert_eq!(parse_cconv_memo_knob(Some("bogus")), CconvMemoMode::Off);
}

#[test]
fn fast_knob_values() {
    crate::test_utils::init_test_tracing();
    assert!(parse_cconv_fast_knob(None));
    assert!(!parse_cconv_fast_knob(Some("off")));
    assert!(!parse_cconv_fast_knob(Some("0")));
    assert!(parse_cconv_fast_knob(Some("on")));
    assert!(parse_cconv_fast_knob(Some(" 1 ")));
    assert_eq!(parse_cconv_memo_knob(Some("on")), CconvMemoMode::On);
    assert_eq!(parse_cconv_memo_knob(Some("1")), CconvMemoMode::On);
    assert_eq!(parse_cconv_memo_knob(Some("verify")), CconvMemoMode::Verify);
}
