//! Sampling a [`MotionSpec`] at a point in time.
//!
//! A `MotionSpec` describes how a value travels; it carries no position, no
//! velocity and no clock. `Motion` binds one to an origin and an interval, and
//! answers where the value is at a given frame.
//!
//! # Why this is a sample and not a step
//!
//! The obvious implementation integrates: keep the current value, advance it by
//! `dt` each frame. That makes the animation a function of *how many frames were
//! drawn*, which is not a property anyone wants. A dropped frame, a slow render,
//! a monitor at a different refresh rate, or a debugger breakpoint all change
//! where the motion ends up. Worse, the value becomes state the compositor has
//! to keep coherent across coalescing, interruption and retargeting.
//!
//! Sampling has none of that: the same `Motion` sampled at the same instant
//! always gives the same answer, so two frames drawn at the same timestamp are
//! identical, and a frame that was never drawn costs nothing. It also makes the
//! finish condition a fact about time rather than an accumulated epsilon.

use neomacs_display_protocol::frame_time::{EventTime, FrameSample};
use neomacs_display_protocol::motion_spec::{MotionSpec, UnitInterval};

/// Where a motion is at one instant.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::render_thread) struct MotionSample {
    /// How far along the motion is, `0.0` at the origin and `1.0` at rest.
    ///
    /// A spring may overshoot, so this is not clamped to the unit interval —
    /// clamping it would silently delete the overshoot that makes a spring
    /// look like a spring.
    pub(in crate::render_thread) progress: f32,
    /// How much of the destination content to show, always within `[0, 1]`.
    ///
    /// Separate from `progress` because a crossfade must not overshoot even
    /// when the geometry does: blending 120% of the destination is not a
    /// picture of anything.
    pub(in crate::render_thread) content_mix: UnitInterval,
    /// Whether the motion has reached its destination and can be dropped.
    pub(in crate::render_thread) finished: bool,
}

impl MotionSample {
    /// The sample of a motion that is over.
    pub(in crate::render_thread) fn settled() -> Self {
        Self {
            progress: 1.0,
            content_mix: UnitInterval::ONE,
            finished: true,
        }
    }
}

/// A [`MotionSpec`] bound to the moment it started.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::render_thread) struct Motion {
    spec: MotionSpec,
    origin: EventTime,
}

/// How close to rest a spring must be, in both value and rate, to be finished.
///
/// A spring approaches its target asymptotically and never arrives, so without
/// a threshold it would request redraws forever. Both conditions are needed:
/// the value passes through the target on every overshoot while still moving
/// fast, so value alone would stop the motion mid-bounce.
const SPRING_REST_EPSILON: f32 = 0.001;

/// How long a deceleration is allowed to run before it is called finished.
///
/// `Deceleration` has no target to converge on — it is a velocity decay — so
/// its end has to come from the decay itself falling below what a pixel can
/// show. This is that point for the fastest flick the input layer produces.
const DECELERATION_REST_EPSILON: f32 = 0.001;

impl Motion {
    /// A motion starting now.
    ///
    /// Returns `None` for [`MotionSpec::Instant`]: there is nothing to sample,
    /// and a caller that holds `None` installs the destination directly rather
    /// than allocating a snapshot to animate between. Making that a type-level
    /// distinction is what keeps disabled motion actually free.
    pub(in crate::render_thread) fn start(spec: MotionSpec, origin: EventTime) -> Option<Self> {
        (!spec.is_instant()).then_some(Self { spec, origin })
    }

    /// Where the motion is at `frame`.
    pub(in crate::render_thread) fn sample(self, frame: FrameSample) -> MotionSample {
        let seconds = frame.since_at_presentation(self.origin).as_secs_f32();
        match self.spec {
            // `start` rejects it, so a `Motion` can never hold it. Answering
            // "already there" is still the only correct answer if one appears.
            MotionSpec::Instant => MotionSample::settled(),
            MotionSpec::Tween(tween) => {
                let duration = tween.duration.as_secs_f32();
                let linear = (seconds / duration).clamp(0.0, 1.0);
                let progress = tween.easing.apply(linear);
                MotionSample {
                    progress,
                    content_mix: UnitInterval::clamp(progress),
                    finished: linear >= 1.0,
                }
            }
            MotionSpec::Spring(spring) => {
                let omega = spring.omega.get();
                let zeta = spring.damping.get();
                let progress = spring_progress(omega, zeta, seconds);
                let rate = spring_rate(omega, zeta, seconds);
                MotionSample {
                    progress,
                    content_mix: UnitInterval::clamp(progress),
                    finished: (progress - 1.0).abs() < SPRING_REST_EPSILON
                        && rate.abs() < SPRING_REST_EPSILON,
                }
            }
            MotionSpec::Deceleration(deceleration) => {
                // Displacement of a first-order decay, normalized so that the
                // total distance travelled is 1: x(t) = 1 - e^(-friction * t).
                let friction = deceleration.friction.get();
                let remaining = (-friction * seconds).exp();
                MotionSample {
                    progress: 1.0 - remaining,
                    content_mix: UnitInterval::clamp(1.0 - remaining),
                    finished: remaining < DECELERATION_REST_EPSILON,
                }
            }
            // `MotionSpec` is `#[non_exhaustive]`, so a variant added later
            // reaches this sampler before anyone teaches it the curve. Settling
            // immediately is the one safe answer: it shows the destination,
            // which is always correct, where guessing a curve from the wrong
            // variant would animate to somewhere nobody asked for.
            _ => MotionSample::settled(),
        }
    }
}

/// A second-order spring's normalized displacement at `t`, from rest to target.
///
/// Sampled analytically rather than integrated, which is what lets the same
/// motion be evaluated at any instant without replaying the frames between.
fn spring_progress(omega: f32, zeta: f32, t: f32) -> f32 {
    if t <= 0.0 {
        return 0.0;
    }
    if zeta < 1.0 {
        // Under-damped: oscillates toward the target, and the overshoot is the
        // point — it is what makes the motion read as physical.
        let damped = omega * (1.0 - zeta * zeta).sqrt();
        let envelope = (-zeta * omega * t).exp();
        1.0 - envelope * ((damped * t).cos() + (zeta * omega / damped) * (damped * t).sin())
    } else if (zeta - 1.0).abs() < f32::EPSILON {
        // Critically damped: the fastest approach with no overshoot at all.
        1.0 - (1.0 + omega * t) * (-omega * t).exp()
    } else {
        // Over-damped: two real roots, no oscillation, slower than critical.
        let root = omega * (zeta * zeta - 1.0).sqrt();
        let fast = -zeta * omega + root;
        let slow = -zeta * omega - root;
        1.0 - (slow * (fast * t).exp() - fast * (slow * t).exp()) / (slow - fast)
    }
}

/// The spring's rate of change at `t`, for deciding when it has come to rest.
fn spring_rate(omega: f32, zeta: f32, t: f32) -> f32 {
    // Differentiating each closed form above would duplicate three cases that
    // must stay consistent with `spring_progress`; a central difference cannot
    // drift away from it, and rest is a threshold question where the exact
    // derivative buys nothing.
    const H: f32 = 1e-4;
    (spring_progress(omega, zeta, t + H) - spring_progress(omega, zeta, (t - H).max(0.0)))
        / (H + (t - (t - H).max(0.0)))
}

#[cfg(test)]
#[path = "motion_test.rs"]
mod tests;
