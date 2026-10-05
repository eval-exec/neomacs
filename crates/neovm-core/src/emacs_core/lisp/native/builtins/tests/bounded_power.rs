use super::{BoundedPower, Integer, PowerError};

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
