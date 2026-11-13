use super::{BoundedPower, Integer, PowerError};
use malachite::base::num::logic::traits::SignificantBits;

#[test]
fn minimum_width_is_a_lower_bound_for_every_admitted_power() {
    for base in [i64::MIN, -7, -2, -1, 0, 1, 2, 7, i64::MAX] {
        for exponent in 0..=64 {
            let power = BoundedPower::try_from((Integer::from(base), exponent)).unwrap();
            let minimum = power.minimum_bits();
            let result = Integer::from(power);
            assert!(minimum <= result.significant_bits());
        }
    }
    let zero = BoundedPower::try_from((Integer::from(0), u64::MAX)).unwrap();
    assert_eq!(zero.minimum_bits(), 0);
}

#[test]
fn checked_power_proves_gnu_limb_limit_without_allocating_result() {
    assert!(BoundedPower::try_from((Integer::from(7), i32::MAX as u64 - 5)).is_ok());
    assert_eq!(
        BoundedPower::try_from((Integer::from(7), i32::MAX as u64 - 4)).unwrap_err(),
        PowerError::Overflow
    );
    assert!(BoundedPower::try_from((Integer::from(7), 1 << 34)).is_err());
    let power = BoundedPower::try_from((Integer::from(-7), 3)).unwrap();
    assert_eq!(Integer::from(power), Integer::from(-343));
    let zero = BoundedPower::try_from((Integer::from(0), u64::MAX)).unwrap();
    assert_eq!(Integer::from(zero), Integer::from(0));
}
