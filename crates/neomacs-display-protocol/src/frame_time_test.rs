use super::*;

fn t0() -> EventTime {
    observe_platform_now()
}

const MS: fn(u64) -> Duration = Duration::from_millis;

// =======================================================================
// EventTime
// =======================================================================

#[test]
fn saturating_since_measures_forward_spans() {
    let start = t0();
    let later = start.plus(MS(250));
    assert_eq!(later.saturating_since(start), MS(250));
}

#[test]
fn saturating_since_clamps_a_reversed_pair_to_zero() {
    // Two adapters mint observations independently; nothing orders them, so a
    // reversed pair must not panic.
    let start = t0();
    let later = start.plus(MS(250));
    assert_eq!(start.saturating_since(later), Duration::ZERO);
}

#[test]
fn plus_and_checked_plus_agree_when_there_is_no_overflow() {
    let start = t0();
    assert_eq!(start.checked_plus(MS(40)), Some(start.plus(MS(40))));
}

#[test]
fn checked_plus_reports_overflow_instead_of_panicking() {
    assert_eq!(t0().checked_plus(Duration::MAX), None);
}

#[test]
fn event_times_order_by_when_they_happened() {
    let start = t0();
    let later = start.plus(MS(1));
    assert!(later > start);
    assert_eq!(start.min(later), start);
}

#[test]
fn an_observed_instant_round_trips() {
    let start = t0();
    assert_eq!(
        EventTime::from_observed_instant(start.into_instant()),
        start
    );
}

// =======================================================================
// FrameSample
// =======================================================================

#[test]
fn presentation_time_is_exactly_one_interval_after_frame_time() {
    let frame = t0();
    let sample = FrameSample::new(frame, MS(16));
    assert_eq!(sample.frame_time(), frame);
    assert_eq!(sample.interval(), MS(16));
    assert_eq!(sample.presentation_time(), frame.plus(MS(16)));
    assert_eq!(
        sample.presentation_time().saturating_since(frame),
        sample.interval()
    );
}

#[test]
fn since_and_since_at_presentation_differ_by_exactly_the_interval() {
    let observed = t0();
    let sample = FrameSample::new(observed.plus(MS(100)), MS(16));
    assert_eq!(sample.since(observed), MS(100));
    assert_eq!(sample.since_at_presentation(observed), MS(116));
    assert_eq!(
        sample.since_at_presentation(observed) - sample.since(observed),
        sample.interval()
    );
}

#[test]
fn ageing_an_observation_from_the_future_saturates_rather_than_panicking() {
    // An input event stamped after the loop chose to draw is entirely possible.
    let frame = t0();
    let sample = FrameSample::new(frame, MS(16));
    let observed_later = frame.plus(MS(5));
    assert_eq!(sample.since(observed_later), Duration::ZERO);
    // ...but it is already in the past by presentation time.
    assert_eq!(sample.since_at_presentation(observed_later), MS(11));
}

#[test]
fn a_zero_interval_sample_dates_presentation_to_the_frame_itself() {
    let frame = t0();
    let sample = FrameSample::new(frame, Duration::ZERO);
    assert_eq!(sample.presentation_time(), frame);
    let observed = frame.saturating_since(frame);
    assert_eq!(observed, Duration::ZERO);
}

#[test]
fn one_sample_ages_every_observation_against_the_same_instant() {
    // The point of the type: two things sampled in one frame cannot disagree
    // about what time it is, which two Instant::now() calls could.
    let base = t0();
    let a = base;
    let b = base.plus(MS(10));
    let sample = FrameSample::new(base.plus(MS(100)), MS(16));
    assert_eq!(sample.since(a) - sample.since(b), MS(10));
}

#[test]
fn samples_are_equal_only_when_both_components_match() {
    let frame = t0();
    assert_eq!(
        FrameSample::new(frame, MS(16)),
        FrameSample::new(frame, MS(16))
    );
    assert_ne!(
        FrameSample::new(frame, MS(16)),
        FrameSample::new(frame, MS(8))
    );
    assert_ne!(
        FrameSample::new(frame, MS(16)),
        FrameSample::new(frame.plus(MS(1)), MS(16))
    );
}
