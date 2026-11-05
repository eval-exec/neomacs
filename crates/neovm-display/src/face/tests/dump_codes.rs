use super::*;
use strum::IntoEnumIterator;

#[test]
fn all_weight_alias_codes_round_trip_without_changing_layout_domain() {
    for code in FontWeightDumpCode::iter() {
        let raw = u16::from(code);
        assert_eq!(FontWeightDumpCode::try_from(raw), Ok(code));
        let weight = FontWeight::from(code);
        assert_eq!(FontWeightDumpCode::from(weight), code);
        assert_eq!(weight.dump_code(), raw);
        assert_eq!(FontWeight::from_dump_code(raw), weight);
    }
}

#[test]
fn unknown_alias_codes_are_checked_separately_from_legacy_css_weights() {
    for raw in [0, 99, 103, 399, 952, u16::MAX] {
        let error = FontWeightDumpCode::try_from(raw).expect_err("unknown dump alias code");
        assert_eq!(error.number, raw);
        assert_eq!(
            FontWeight::from_dump_code(raw),
            FontWeight::from_css_weight(raw)
        );
    }
}
