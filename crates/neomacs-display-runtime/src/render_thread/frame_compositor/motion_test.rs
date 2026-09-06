use super::*;
use neomacs_display_protocol::motion_spec::{
    AngularFrequency, DampingRatio, DecelerationSpec, MotionDuration, SpringSpec, TweenSpec,
};
use neomacs_display_protocol::scroll_animation::TransitionEasing;
use std::time::Duration;

/// A fixed origin, so every sample below is a function of the offset alone.
fn origin() -> EventTime {
    neomacs_display_protocol::frame_time::observe_platform_now()
}

/// The frame drawn `millis` after `origin`, presented immediately.
///
/// A zero interval keeps these tests about the motion rather than about the
/// presentation prediction, which `frame_time` covers on its own.
fn frame_at(origin: EventTime, millis: u64) -> FrameSample {
    FrameSample::new(origin.plus(Duration::from_millis(millis)), Duration::ZERO)
}

fn tween(millis: u64, easing: TransitionEasing) -> MotionSpec {
    MotionSpec::Tween(TweenSpec {
        duration: MotionDuration::new(Duration::from_millis(millis)).expect("a positive duration"),
        easing,
        bezier: None,
    })
}

// =======================================================================
// Instant is not a motion
// =======================================================================

#[test]
fn an_instant_spec_yields_no_motion_at_all() {
    // The caller must install the destination directly; a `Motion` that
    // existed here would ask for a snapshot to animate between, which is
    // exactly the cost a reduced-motion setting is supposed to avoid.
    assert!(Motion::start(MotionSpec::Instant, origin()).is_none());
}

// =======================================================================
// Tween
// =======================================================================

#[test]
fn a_linear_tween_is_where_the_clock_says_it_is() {
    let origin = origin();
    let motion = Motion::start(tween(100, TransitionEasing::Linear), origin).expect("a motion");
    assert!((motion.sample(frame_at(origin, 0)).progress - 0.0).abs() < 1e-5);
    assert!((motion.sample(frame_at(origin, 25)).progress - 0.25).abs() < 1e-5);
    assert!((motion.sample(frame_at(origin, 50)).progress - 0.5).abs() < 1e-5);
}

#[test]
fn a_tween_finishes_exactly_at_its_duration_and_stays_finished() {
    let origin = origin();
    let motion = Motion::start(tween(100, TransitionEasing::Linear), origin).expect("a motion");
    assert!(!motion.sample(frame_at(origin, 99)).finished);

    let done = motion.sample(frame_at(origin, 100));
    assert!(done.finished);
    assert!((done.progress - 1.0).abs() < 1e-5);

    // A frame drawn long after the end must not run past the destination: a
    // late frame is common, and there is nowhere past 1.0 to be.
    let late = motion.sample(frame_at(origin, 10_000));
    assert!(late.finished);
    assert!((late.progress - 1.0).abs() < 1e-5);
}

#[test]
fn sampling_the_same_instant_twice_gives_the_same_answer() {
    // The property that separates sampling from integrating. A compositor that
    // draws two frames for one timestamp — a resize repaint, a forced redraw —
    // must produce identical pixels, and a frame that was skipped must cost
    // the motion nothing.
    let origin = origin();
    let motion = Motion::start(tween(100, TransitionEasing::EaseOutCubic), origin).expect("motion");
    assert_eq!(
        motion.sample(frame_at(origin, 37)),
        motion.sample(frame_at(origin, 37))
    );
}

#[test]
fn skipping_frames_does_not_change_where_the_motion_ends_up() {
    let origin = origin();
    let motion = Motion::start(tween(100, TransitionEasing::EaseOutQuad), origin).expect("motion");
    let sampled_every_frame: Vec<_> = (0..=60)
        .map(|ms| motion.sample(frame_at(origin, ms)))
        .collect();
    let jumped = motion.sample(frame_at(origin, 60));
    assert_eq!(
        *sampled_every_frame.last().expect("a last sample"),
        jumped,
        "the value is a function of the clock, not of how many frames were drawn"
    );
}

#[test]
fn easing_shapes_the_progress_without_moving_the_endpoints() {
    let origin = origin();
    for easing in [
        TransitionEasing::Linear,
        TransitionEasing::EaseOutQuad,
        TransitionEasing::EaseOutCubic,
        TransitionEasing::EaseInOutCubic,
    ] {
        let motion = Motion::start(tween(100, easing), origin).expect("a motion");
        assert!(
            motion.sample(frame_at(origin, 0)).progress.abs() < 1e-5,
            "{easing:?} must start at the source"
        );
        assert!(
            (motion.sample(frame_at(origin, 100)).progress - 1.0).abs() < 1e-5,
            "{easing:?} must arrive at the destination"
        );
    }
}

// =======================================================================
// Spring
// =======================================================================

fn spring(omega: f32, damping: f32) -> MotionSpec {
    MotionSpec::Spring(SpringSpec {
        omega: AngularFrequency::new(omega).expect("a positive frequency"),
        damping: DampingRatio::new(damping).expect("a positive damping ratio"),
    })
}

#[test]
fn a_critically_damped_spring_approaches_without_overshooting() {
    let origin = origin();
    let motion = Motion::start(spring(20.0, 1.0), origin).expect("a motion");
    for ms in 0..500 {
        let progress = motion.sample(frame_at(origin, ms)).progress;
        assert!(
            progress <= 1.0 + 1e-4,
            "critical damping is the fastest approach with no overshoot, got {progress} at {ms}ms"
        );
    }
}

#[test]
fn an_under_damped_spring_overshoots_and_the_sample_keeps_the_overshoot() {
    // Clamping progress into the unit interval here would delete exactly what
    // makes a spring look like a spring.
    let origin = origin();
    let motion = Motion::start(spring(30.0, 0.3), origin).expect("a motion");
    let peak = (0..500)
        .map(|ms| motion.sample(frame_at(origin, ms)).progress)
        .fold(f32::MIN, f32::max);
    assert!(
        peak > 1.05,
        "expected a visible overshoot, peaked at {peak}"
    );
}

#[test]
fn the_content_mix_never_overshoots_even_when_the_geometry_does() {
    // Blending 120% of the destination is not a picture of anything.
    let origin = origin();
    let motion = Motion::start(spring(30.0, 0.3), origin).expect("a motion");
    for ms in 0..500 {
        let mix = motion.sample(frame_at(origin, ms)).content_mix.get();
        assert!((0.0..=1.0).contains(&mix), "content mix left [0,1]: {mix}");
    }
}

#[test]
fn a_spring_reports_finished_only_once_it_has_stopped_moving() {
    let origin = origin();
    let motion = Motion::start(spring(30.0, 0.3), origin).expect("a motion");
    // An under-damped spring passes through its target on every bounce. If
    // rest were decided by value alone, the motion would end mid-swing with
    // the pane still visibly travelling.
    let first_crossing = (0..2000)
        .find(|ms| (motion.sample(frame_at(origin, *ms)).progress - 1.0).abs() < 1e-3)
        .expect("the spring crosses its target");
    assert!(
        !motion.sample(frame_at(origin, first_crossing)).finished,
        "still moving fast at the first crossing"
    );
    assert!(
        motion.sample(frame_at(origin, 2000)).finished,
        "and settled once it has come to rest"
    );
}

#[test]
fn an_over_damped_spring_is_slower_than_a_critical_one_and_never_overshoots() {
    let origin = origin();
    let critical = Motion::start(spring(20.0, 1.0), origin).expect("a motion");
    let over = Motion::start(spring(20.0, 2.5), origin).expect("a motion");
    let at = |m: Motion, ms| m.sample(frame_at(origin, ms)).progress;
    assert!(
        at(over, 100) < at(critical, 100),
        "over-damping trades speed for certainty"
    );
    for ms in 0..500 {
        assert!(at(over, ms) <= 1.0 + 1e-4);
    }
}

// =======================================================================
// Deceleration
// =======================================================================

#[test]
fn a_deceleration_covers_ground_quickly_then_creeps_and_finally_stops() {
    let origin = origin();
    let motion = Motion::start(
        MotionSpec::Deceleration(DecelerationSpec {
            friction: AngularFrequency::new(6.0).expect("a positive friction"),
        }),
        origin,
    )
    .expect("a motion");
    let early = motion.sample(frame_at(origin, 100)).progress;
    let late = motion.sample(frame_at(origin, 200)).progress;
    assert!(early > 0.4, "most of a flick's distance lands early");
    assert!(late > early, "and it keeps moving");
    assert!(
        motion.sample(frame_at(origin, 5_000)).finished,
        "a decay with no target still has to stop asking for redraws"
    );
}
