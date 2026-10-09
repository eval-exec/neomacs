use super::*;

#[test]
fn unrepresentable_wait_deadline_keeps_the_indefinite_wait_state() {
    let deadline = WaitDeadline::for_duration_with_timer_deadline(
        Duration::new(i64::MAX as u64, 999_999_999),
        GnuTimerTimestamp::now(),
    );
    assert!(matches!(deadline, WaitDeadline::Forever));
    assert_eq!(deadline.remaining(Instant::now()), None);
}
