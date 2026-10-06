use super::*;
use strum::IntoEnumIterator;

#[test]
fn derived_arithmetic_decode_accepts_each_kind_and_reports_unknown_codes() {
    for kind in ArithGenericKind::iter() {
        assert_eq!(ArithGenericKind::try_from(kind as i64), Ok(kind));
    }
    for raw in [-1, 15, 16, i64::MIN, i64::MAX] {
        let error = ArithGenericKind::try_from(raw).expect_err("unknown arithmetic code");
        assert_eq!(error.number, raw);
        assert_eq!(ArithGenericKind::from_raw(raw), None);
    }
}
