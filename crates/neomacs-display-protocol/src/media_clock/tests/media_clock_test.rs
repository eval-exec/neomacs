//! MediaClock timeline arithmetic.

use super::MediaClock;
use crate::frame_time::EventTime;
use std::time::Duration;

/// One real anchor; every other time is derived by `Duration` arithmetic,
/// matching the frame scheduler's determinism discipline.
fn anchor() -> EventTime {
    EventTime::from_observed_instant(std::time::Instant::now())
}

#[test]
fn fresh_clock_reads_zero_and_is_paused() {
    let start = anchor();
    let mut clock = MediaClock::new();
    assert!(clock.is_paused());
    assert_eq!(
        clock.document_time(start.plus(Duration::from_secs(10))),
        Duration::ZERO
    );
}

#[test]
fn first_resume_fixes_the_epoch() {
    let start = anchor();
    let mut clock = MediaClock::new();
    clock.resume(start.plus(Duration::from_secs(2)));
    assert!(!clock.is_paused());
    assert_eq!(
        clock.document_time(start.plus(Duration::from_secs(5))),
        Duration::from_secs(3)
    );
}

#[test]
fn time_before_the_epoch_saturates_at_zero() {
    let start = anchor();
    let mut clock = MediaClock::new();
    clock.resume(start.plus(Duration::from_secs(5)));
    assert_eq!(clock.document_time(start), Duration::ZERO);
}

#[test]
fn pause_freezes_and_resume_continues_across_the_gap() {
    let start = anchor();
    let mut clock = MediaClock::new();
    clock.resume(start);

    clock.pause(start.plus(Duration::from_secs(1)));
    assert!(clock.is_paused());
    // A paused clock ignores the world moving on.
    assert_eq!(
        clock.document_time(start.plus(Duration::from_secs(60))),
        Duration::from_secs(1)
    );

    clock.resume(start.plus(Duration::from_secs(10)));
    assert_eq!(
        clock.document_time(start.plus(Duration::from_secs(12))),
        Duration::from_secs(3)
    );
}

#[test]
fn repeated_pause_keeps_the_earlier_freeze_point() {
    let start = anchor();
    let mut clock = MediaClock::new();
    clock.resume(start);
    clock.pause(start.plus(Duration::from_secs(1)));
    clock.pause(start.plus(Duration::from_secs(2)));
    assert_eq!(
        clock.document_time(start.plus(Duration::from_secs(3))),
        Duration::from_secs(1)
    );
}

#[test]
fn repeated_resume_does_not_reanchor() {
    let start = anchor();
    let mut clock = MediaClock::new();
    clock.resume(start);
    clock.resume(start.plus(Duration::from_secs(100)));
    assert_eq!(
        clock.document_time(start.plus(Duration::from_secs(1))),
        Duration::from_secs(1)
    );
}

#[test]
fn default_matches_new() {
    let start = anchor();
    assert_eq!(
        MediaClock::default().document_time(start),
        MediaClock::new().document_time(start)
    );
}
