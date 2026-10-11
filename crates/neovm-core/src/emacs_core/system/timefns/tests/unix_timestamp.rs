use super::*;
use std::time::Duration;

#[test]
fn signed_system_time_round_trip_is_normalized() {
    for offset in [
        Duration::from_secs(1),
        Duration::from_millis(500),
        Duration::new(1, 1),
    ] {
        let system_time = UNIX_EPOCH.checked_sub(offset).unwrap();
        let timestamp = UnixTimestamp::try_from(system_time).unwrap();
        assert_eq!(SystemTime::try_from(timestamp).unwrap(), system_time);
        assert!(timestamp.nanoseconds() < 1_000_000_000);
    }
    let half = UnixTimestamp::try_from(UNIX_EPOCH - Duration::from_millis(500)).unwrap();
    assert_eq!((half.seconds(), half.nanoseconds()), (-1, 500_000_000));
}
