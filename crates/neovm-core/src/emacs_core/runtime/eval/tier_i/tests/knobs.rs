//! The `NEOVM_TIER_I` knob.

use crate::emacs_core::eval::{TierIMode, parse_tier_i_knob};

#[test]
fn mode_values() {
    crate::test_utils::init_test_tracing();
    assert_eq!(parse_tier_i_knob(None), TierIMode::Off);
    for off in ["", "0", "off", "OFF", "no", "false", "bogus"] {
        assert_eq!(parse_tier_i_knob(Some(off)), TierIMode::Off, "{off}");
    }
    assert_eq!(parse_tier_i_knob(Some("census")), TierIMode::Census);
    assert_eq!(parse_tier_i_knob(Some(" Census ")), TierIMode::Census);
}
