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
use neomacs_display_protocol::scroll_animation::TransitionEasing;

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
    /// How fast `progress` is changing, per second.
    ///
    /// Carried on the sample rather than recomputed on demand, because an
    /// interrupted motion must start the next one at the speed this one had,
    /// and a second evaluation could land on the other side of a tween's clamp
    /// and report zero for a motion that was still moving.
    pub(in crate::render_thread) rate: f32,
    /// Whether the motion has reached its destination and can be dropped.
    pub(in crate::render_thread) finished: bool,
}

/// How fast a motion's progress is changing, in progress per second.
///
/// Finite by construction: a NaN reaching a `Motion` comes back out as a pane
/// rect made of NaN, which draws nothing and reads to a user as a blank frame
/// rather than as a bug worth reporting.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::render_thread) struct ProgressRate(f32);

impl ProgressRate {
    pub(in crate::render_thread) fn new(value: f32) -> Self {
        if value.is_finite() {
            Self(value)
        } else {
            // A non-finite rate would come back out as a pane rect made of
            // NaN, which draws nothing and reads as a blank frame rather than
            // as a bug worth reporting. Treat it as a standstill.
            Self(0.0)
        }
    }

    pub(in crate::render_thread) const fn get(self) -> f32 {
        self.0
    }
}

/// A [`MotionSpec`] bound to the moment it started and the speed it started at.
///
/// `entry_rate` is a boundary condition fixed once at construction — it is not
/// a velocity advanced per frame. That is what keeps this a sampler: the same
/// `Motion` evaluated at the same instant still gives the same answer, however
/// many frames were drawn, so an interruption changes the curve rather than
/// introducing state that coalescing and retargeting would have to keep
/// coherent.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::render_thread) struct Motion {
    spec: MotionSpec,
    origin: EventTime,
    /// The speed this motion inherited, or `None` when it began at rest.
    ///
    /// `None` is not the same as `Some(0.0)`, and conflating them is a bug this
    /// distinction exists to prevent: a motion starting fresh should follow its
    /// easing curve exactly, while one resuming at a standstill must be bent to
    /// *depart* at zero — which for most curves is a different shape.
    entry_rate: Option<ProgressRate>,
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
        (!spec.is_instant()).then_some(Self {
            spec,
            origin,
            entry_rate: None,
        })
    }

    /// A motion starting now at a speed it inherited from one it interrupted.
    ///
    /// The distinction from [`Self::start`] is the whole of step 11: a layout
    /// change arriving mid-motion must carry the panes on from where they were
    /// last drawn, at the speed they had, rather than restarting them from a
    /// standstill at a place they never reached.
    pub(in crate::render_thread) fn resume(
        spec: MotionSpec,
        origin: EventTime,
        entry_rate: ProgressRate,
    ) -> Option<Self> {
        (!spec.is_instant()).then_some(Self {
            spec,
            origin,
            entry_rate: Some(entry_rate),
        })
    }

    /// Where the motion is at `frame`.
    pub(in crate::render_thread) fn sample(self, frame: FrameSample) -> MotionSample {
        let seconds = frame.since_at_presentation(self.origin).as_secs_f32();
        let progress = self.progress_at(seconds);
        MotionSample {
            progress,
            content_mix: UnitInterval::clamp(progress),
            rate: self.rate_at(seconds),
            finished: self.finished_at(seconds, progress),
        }
    }

    /// How far along the motion is `seconds` after it began.
    ///
    /// May exceed 1.0: a spring overshoots, and an interrupted tween carrying
    /// speed into a shorter distance can too. Clamping here would delete the
    /// overshoot that makes both read as physical.
    fn progress_at(self, seconds: f32) -> f32 {
        match self.spec {
            // `resume` rejects it, so a `Motion` can never hold it. Answering
            // "already there" is still the only correct answer if one appears.
            MotionSpec::Instant => 1.0,
            MotionSpec::Tween(tween) => {
                let duration = tween.duration.as_secs_f32();
                let u = (seconds / duration).clamp(0.0, 1.0);
                // A tween asked to start already moving gets a bump ADDED to
                // its easing rather than a rescaled duration. `u(1-u)^2` and
                // its derivative are both zero at u = 1, so the curve still
                // arrives exactly at the destination exactly at its duration;
                // only its departure changes. Rescaling the duration instead
                // would let an interruption silently change how long the
                // motion takes, and there is no duration at all that gives
                // ease-in-out a non-zero departure.
                let eased = tween.easing.apply(u);
                let Some(entry) = self.entry_rate else {
                    return eased;
                };
                let excess = entry.get() * duration - easing_initial_rate(tween.easing);
                let bump = excess.clamp(-MAX_ENTRY_BUMP, MAX_ENTRY_BUMP);
                eased + bump * u * (1.0 - u) * (1.0 - u)
            }
            MotionSpec::Spring(spring) => spring_progress(
                spring.omega.get(),
                spring.damping.get(),
                self.entry_rate.map_or(0.0, ProgressRate::get),
                seconds,
            ),
            MotionSpec::Deceleration(deceleration) => {
                // Displacement of a first-order decay, normalized so the total
                // distance travelled is 1: x(t) = 1 - e^(-friction * t).
                1.0 - (-deceleration.friction.get() * seconds).exp()
            }
            // `MotionSpec` is `#[non_exhaustive]`, so a variant added later
            // reaches this sampler before anyone teaches it the curve. Settling
            // immediately is the one safe answer: it shows the destination,
            // which is always correct, where guessing a curve from the wrong
            // variant would animate to somewhere nobody asked for.
            _ => 1.0,
        }
    }

    /// How fast progress is changing `seconds` in.
    ///
    /// A central difference over [`Self::progress_at`] rather than a derivative
    /// per variant. Differentiating each closed form by hand would duplicate
    /// four cases that must stay consistent with the curves they differentiate,
    /// and a numeric difference cannot drift away from the function it is taken
    /// over. Rest and hand-off are both threshold questions, where the exact
    /// derivative buys nothing.
    fn rate_at(self, seconds: f32) -> f32 {
        const H: f32 = 1e-4;
        let before = (seconds - H).max(0.0);
        (self.progress_at(seconds + H) - self.progress_at(before)) / (H + seconds - before)
    }

    fn finished_at(self, seconds: f32, progress: f32) -> bool {
        match self.spec {
            MotionSpec::Instant => true,
            MotionSpec::Tween(tween) => seconds >= tween.duration.as_secs_f32(),
            // Both conditions are needed. An under-damped spring passes through
            // its target on every bounce while still moving fast, so value
            // alone would stop the motion mid-swing.
            MotionSpec::Spring(_) => {
                (progress - 1.0).abs() < SPRING_REST_EPSILON
                    && self.rate_at(seconds).abs() < SPRING_REST_EPSILON
            }
            // A decay has no target to converge on, so its end has to come from
            // the decay itself falling below what a pixel can show.
            MotionSpec::Deceleration(deceleration) => {
                (-deceleration.friction.get() * seconds).exp() < DECELERATION_REST_EPSILON
            }
            _ => true,
        }
    }
}

/// How far a tween's departure may be bent to match an inherited speed.
///
/// Unbounded, a fast interruption into a long duration produces a bump that
/// overshoots the destination absurdly before easing back. This is generous
/// enough to look continuous and small enough to stay on screen.
const MAX_ENTRY_BUMP: f32 = 2.0;

/// The initial slope of an easing curve, in progress per unit of normalized
/// time, so the bump can be sized against the departure the curve already has.
fn easing_initial_rate(easing: TransitionEasing) -> f32 {
    const H: f32 = 1e-4;
    easing.apply(H) / H
}

/// A second-order spring's normalized displacement at `t`.
///
/// `v0` is the entry rate: the speed the value already had when this motion
/// took over. A spring is the one curve where an inherited velocity is exact
/// rather than approximated — it is a boundary condition of the same ODE, so
/// the closed forms below simply gain a `v0` term and the result is still the
/// analytic solution, evaluable at any instant without replaying the frames
/// between.
fn spring_progress(omega: f32, zeta: f32, v0: f32, t: f32) -> f32 {
    if t <= 0.0 {
        return 0.0;
    }
    // Solved for the error e = 1 - x, which starts at e(0) = 1 with
    // e'(0) = -v0, so that x(0) = 0 and x'(0) = v0 for every damping regime.
    let error = if zeta < 1.0 {
        // Under-damped: oscillates toward the target, and the overshoot is the
        // point — it is what makes the motion read as physical.
        let damped = omega * (1.0 - zeta * zeta).sqrt();
        let envelope = (-zeta * omega * t).exp();
        envelope * ((damped * t).cos() + (zeta * omega - v0) / damped * (damped * t).sin())
    } else if (zeta - 1.0).abs() < f32::EPSILON {
        // Critically damped: the fastest approach with no overshoot at all.
        (1.0 + (omega - v0) * t) * (-omega * t).exp()
    } else {
        // Over-damped: two real roots, no oscillation, slower than critical.
        let root = omega * (zeta * zeta - 1.0).sqrt();
        let fast = -zeta * omega + root;
        let slow = -zeta * omega - root;
        let a = (slow + v0) / (slow - fast);
        let b = 1.0 - a;
        a * (fast * t).exp() + b * (slow * t).exp()
    };
    1.0 - error
}

#[cfg(test)]
#[path = "motion_test.rs"]
mod tests;
