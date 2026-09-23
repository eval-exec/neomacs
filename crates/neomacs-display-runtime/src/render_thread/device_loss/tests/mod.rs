use super::{CONSECUTIVE_SURFACE_LOST_THRESHOLD, DeviceLossDetector};
use std::sync::atomic::Ordering;

#[test]
fn take_drains_the_latch_once() {
    let mut detector = DeviceLossDetector::new();
    assert!(!detector.take());

    detector.mark_lost_now();
    assert!(detector.take());
    assert!(!detector.take());
}

#[test]
fn shared_flag_feeds_take() {
    let mut detector = DeviceLossDetector::new();
    let flag = detector.shared_flag();

    flag.store(true, Ordering::SeqCst);
    assert!(detector.take());
    assert!(!detector.take());
}

#[test]
fn streak_escalates_exactly_at_the_threshold() {
    let mut detector = DeviceLossDetector::new();
    for _ in 0..CONSECUTIVE_SURFACE_LOST_THRESHOLD - 1 {
        assert!(!detector.record_surface_lost());
    }
    assert!(!detector.take(), "no latch below the threshold");

    assert!(
        detector.record_surface_lost(),
        "the Nth consecutive Lost escalates"
    );
    assert!(detector.take());

    // The streak restarted after escalation.
    assert!(!detector.record_surface_lost());
    assert!(!detector.take());
}

#[test]
fn successful_acquire_resets_the_streak() {
    let mut detector = DeviceLossDetector::new();
    for _ in 0..CONSECUTIVE_SURFACE_LOST_THRESHOLD - 1 {
        assert!(!detector.record_surface_lost());
    }
    detector.record_surface_acquired();

    for _ in 0..CONSECUTIVE_SURFACE_LOST_THRESHOLD - 1 {
        assert!(!detector.record_surface_lost());
    }
    assert!(!detector.take(), "acquired reset must restart the streak");
    assert!(detector.record_surface_lost());
    assert!(detector.take());
}

#[test]
fn taking_a_callback_loss_resets_the_streak() {
    let mut detector = DeviceLossDetector::new();
    for _ in 0..CONSECUTIVE_SURFACE_LOST_THRESHOLD - 1 {
        assert!(!detector.record_surface_lost());
    }
    detector.mark_lost_now();
    assert!(detector.take());
    // The rebuild starts from a clean streak.
    assert!(!detector.record_surface_lost());
}
