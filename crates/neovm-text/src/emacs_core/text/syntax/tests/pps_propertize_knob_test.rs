//! `NEOVM_PPS_PROPERTIZE`: on by default (GNU's behaviour); `0` restores the
//! scan that never propertizes, for attribution only.

use super::*;

#[test]
fn the_knob_is_on_unless_switched_off() {
    for (value, on) in [
        (None, true),
        (Some("1"), true),
        (Some("on"), true),
        (Some(""), true),
        (Some("0"), false),
        (Some("off"), false),
        (Some(" OFF "), false),
        (Some("no"), false),
        (Some("false"), false),
        (Some("nil"), false),
    ] {
        assert_eq!(parse_pps_propertize_knob(value), on, "{value:?}");
    }
}
