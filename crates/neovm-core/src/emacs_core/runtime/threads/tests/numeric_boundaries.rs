use super::super::{make_sleep_blocker, sleep_duration_from_blocker};
use std::time::Duration;

#[test]
fn sleep_blocker_saturates_gnu_timeout_without_duration_panic() {
    assert_eq!(
        sleep_duration_from_blocker(make_sleep_blocker(2.5)),
        Some(Duration::new(2, 500_000_000))
    );
    assert_eq!(
        sleep_duration_from_blocker(make_sleep_blocker(1e300)),
        Some(Duration::new(i64::MAX as u64, 999_999_999))
    );
    assert_eq!(
        sleep_duration_from_blocker(make_sleep_blocker(f64::INFINITY)),
        Some(Duration::new(i64::MAX as u64, 999_999_999))
    );
    assert_eq!(
        sleep_duration_from_blocker(make_sleep_blocker(f64::NAN)),
        None
    );
}
