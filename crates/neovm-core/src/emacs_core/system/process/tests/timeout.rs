//! GNU lib/dtotimespec.c saturation, tested without entering a wait.

use super::*;
use crate::emacs_core::eval::Context;

#[test]
fn process_wait_deadline_handles_poll_finite_and_overflow_without_waiting() {
    let now = Instant::now();
    let poll = ProcessWaitDeadline::from(Duration::ZERO);
    assert_eq!(poll, ProcessWaitDeadline::Poll);
    assert_eq!(poll.remaining(now), Some(Duration::ZERO));
    assert!(poll.is_expired(now));
    let forever = ProcessWaitDeadline::from(Duration::MAX);
    assert_eq!(forever, ProcessWaitDeadline::Forever);
    assert_eq!(forever.remaining(now), None);
    assert!(!forever.is_expired(now));
    let future = now.checked_add(Duration::from_secs(1)).unwrap();
    let finite = ProcessWaitDeadline::Until(future);
    assert_eq!(finite.remaining(now), Some(Duration::from_secs(1)));
    assert!(!finite.is_expired(now));
    assert!(finite.is_expired(future));
}

#[test]
fn accept_process_output_timeout_saturates_positive_float_overflow() {
    let _context = Context::new();
    for seconds in [f64::INFINITY, f64::MAX, 1.0e300] {
        let args = [Value::NIL, Value::make_float(seconds)];
        let timeout = accept_process_output_positive_timeout(&args).unwrap();
        assert_eq!(timeout, Duration::new(i64::MAX as u64, 999_999_999));
    }
}

#[test]
fn accept_process_output_timeout_preserves_poll_and_milliseconds() {
    let _context = Context::new();
    for seconds in [f64::NAN, f64::NEG_INFINITY, -1.0, 0.0] {
        assert_eq!(
            accept_process_output_positive_timeout(&[Value::NIL, Value::make_float(seconds)]),
            None,
        );
    }
    assert_eq!(accept_process_output_positive_timeout(&[]), None);
    assert_eq!(
        accept_process_output_positive_timeout(
            &[Value::NIL, Value::fixnum(2), Value::fixnum(500),]
        ),
        Some(Duration::new(2, 500_000_000)),
    );
}
