use super::*;
#[test]
fn clock_conversion_rejects_uncertain_zero_and_overflowing_observations() {
    let calibration = Calibration {
        monotonic: 100,
        local: 300,
        uncertainty_ns: 50,
    };
    assert_eq!(calibration.convert(350), Some(150));
    assert_eq!(calibration.convert(100), None);
    assert_eq!(calibration.convert(0), None);
    assert_eq!(
        Calibration {
            uncertainty_ns: 1_000_001,
            ..calibration
        }
        .convert(350),
        None
    );
    assert_eq!(
        Calibration {
            monotonic: u64::MAX,
            local: 1,
            uncertainty_ns: 0
        }
        .convert(2),
        None
    );
}
