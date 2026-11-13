use super::*;

static_assertions::assert_impl_all!(WaitDeadline: Send, Sync);

#[test]
fn duration_deadline_uses_the_supplied_fine_clock_for_expiration() {
    let deadline = WaitDeadline::for_duration(Duration::from_secs(60));
    let WaitDeadline::Until { instant, .. } = deadline else {
        panic!("representable finite deadline");
    };
    assert!(!deadline.expired(instant - Duration::from_nanos(1)));
    assert!(deadline.expired(instant));
    assert!(deadline.expired(instant + Duration::from_nanos(1)));
}

#[test]
fn duration_deadline_never_shortens_the_fine_remainder() {
    let deadline = WaitDeadline::for_duration(Duration::from_millis(50));
    let WaitDeadline::Until { instant, .. } = deadline else {
        panic!("representable finite deadline");
    };
    // Simulate a loop's fine sample predating a later clock read. A second
    // clock source must not shorten the timeout computed from that sample.
    let remaining = Duration::from_millis(100);
    assert_eq!(deadline.remaining(instant - remaining), Some(remaining));
    assert_eq!(deadline.remaining(instant), Some(Duration::ZERO));
}

#[test]
fn poll_deadline_preserves_clock_free_policy() {
    let now = Instant::now();
    let deadline = process_output_wait_deadline(ProcessOutputWaitTiming::Poll);
    assert_eq!(deadline, WaitDeadline::Poll);
    assert!(!deadline.expired(now));
    assert_eq!(deadline.remaining(now), Some(Duration::ZERO));
    assert_eq!(deadline.timer_deadline(), None);
}
