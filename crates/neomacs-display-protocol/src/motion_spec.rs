//! Validated motion parameters shared by every temporal effect.
//!
//! This module holds only *descriptions* of motion: how long, how stiff, how
//! damped. It holds no clock, no mutable animation state and no sampling. A
//! [`MotionSpec`] says what a motion is; the compositor's private motion kernel
//! samples it against an injected frame tick.
//!
//! Every numeric parameter is a validated newtype whose only constructor
//! rejects the values that would make a sampler produce `NaN` or diverge. That
//! is what lets the sampler be total: once a `SpringSpec` exists, there is no
//! frequency of zero and no non-finite damping to defend against.

use std::fmt;
use std::time::Duration;

use strum::IntoStaticStr;

use crate::scroll_animation::TransitionEasing;

/// Why a motion parameter was rejected.
///
/// Deliberately coarse: these are configuration errors surfaced once, at the
/// Elisp/config boundary, never on a sampling path.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum MotionSpecError {
    /// A duration was zero. Instantaneous motion is [`MotionSpec::Instant`],
    /// which is a distinct variant rather than a degenerate tween.
    ZeroDuration,
    /// A value was `NaN` or infinite.
    NotFinite,
    /// A value that must be strictly positive was zero or negative.
    NotPositive,
    /// A value that must not be negative was negative.
    Negative,
}

impl fmt::Display for MotionSpecError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::ZeroDuration => "motion duration must be non-zero (use MotionSpec::Instant)",
            Self::NotFinite => "motion parameter must be finite",
            Self::NotPositive => "motion parameter must be strictly positive",
            Self::Negative => "motion parameter must not be negative",
        })
    }
}

impl std::error::Error for MotionSpecError {}

/// A strictly positive animation duration.
///
/// Zero is not representable: a motion that takes no time is
/// [`MotionSpec::Instant`], so a sampler never divides by a zero duration.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Serialize)]
#[serde(transparent)]
pub struct MotionDuration(Duration);

impl MotionDuration {
    /// # Errors
    /// [`MotionSpecError::ZeroDuration`] if `value` is zero.
    pub const fn new(value: Duration) -> Result<Self, MotionSpecError> {
        if value.is_zero() {
            Err(MotionSpecError::ZeroDuration)
        } else {
            Ok(Self(value))
        }
    }

    /// Build from seconds, the unit the Elisp surface uses.
    ///
    /// # Errors
    /// [`MotionSpecError::NotFinite`] if `secs` is `NaN` or infinite,
    /// [`MotionSpecError::NotPositive`] if it is zero or negative.
    pub fn from_secs_f32(secs: f32) -> Result<Self, MotionSpecError> {
        if !secs.is_finite() {
            return Err(MotionSpecError::NotFinite);
        }
        if secs <= 0.0 {
            return Err(MotionSpecError::NotPositive);
        }
        Self::new(Duration::from_secs_f32(secs))
    }

    #[must_use]
    pub const fn get(self) -> Duration {
        self.0
    }

    #[must_use]
    pub fn as_secs_f32(self) -> f32 {
        self.0.as_secs_f32()
    }
}

/// A progress or mixing value clamped to `[0, 1]`.
///
/// Used for opacity and for the previous/destination content mix during a
/// transition. Displacement is deliberately *not* a `UnitInterval`: a spring
/// overshoots past its target, and clamping that would silently remove the
/// overshoot the spring exists to produce.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd, serde::Serialize)]
#[serde(transparent)]
pub struct UnitInterval(f32);

impl UnitInterval {
    pub const ZERO: Self = Self(0.0);
    pub const ONE: Self = Self(1.0);

    /// The total constructor used on sampling paths.
    ///
    /// Values outside `[0, 1]` saturate. `NaN` becomes `ZERO`: a sampler that
    /// has gone numerically unstable should render the un-progressed frame
    /// rather than propagate `NaN` into a vertex buffer.
    #[must_use]
    pub fn clamp(value: f32) -> Self {
        if value.is_nan() {
            Self::ZERO
        } else {
            Self(value.clamp(0.0, 1.0))
        }
    }

    /// The checked constructor used at configuration boundaries.
    ///
    /// # Errors
    /// [`MotionSpecError::NotFinite`] if `value` is `NaN` or infinite.
    pub fn new(value: f32) -> Result<Self, MotionSpecError> {
        if value.is_finite() {
            Ok(Self(value.clamp(0.0, 1.0)))
        } else {
            Err(MotionSpecError::NotFinite)
        }
    }

    #[must_use]
    pub const fn get(self) -> f32 {
        self.0
    }
}

/// Undamped angular frequency of a spring, in radians per second.
///
/// Strictly positive: a spring with zero frequency never converges.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, serde::Serialize)]
#[serde(transparent)]
pub struct AngularFrequency(f32);

/// Damping ratio of a spring. `1.0` is critical damping.
///
/// Strictly positive: a spring with zero damping oscillates forever, so no
/// completion time can be computed for it.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, serde::Serialize)]
#[serde(transparent)]
pub struct DampingRatio(f32);

/// A length in physical device pixels, after scale resolution.
///
/// Motion thresholds live in this unit rather than in logical pixels so a
/// "too small to be worth animating" policy means the same thing on a HiDPI
/// display as on a 1x one.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd, serde::Serialize)]
#[serde(transparent)]
pub struct DevicePixels(f32);

macro_rules! positive_f32_newtype {
    ($name:ident) => {
        impl $name {
            /// # Errors
            /// [`MotionSpecError::NotFinite`] if `value` is `NaN` or infinite,
            /// [`MotionSpecError::NotPositive`] if it is zero or negative.
            pub fn new(value: f32) -> Result<Self, MotionSpecError> {
                if !value.is_finite() {
                    Err(MotionSpecError::NotFinite)
                } else if value <= 0.0 {
                    Err(MotionSpecError::NotPositive)
                } else {
                    Ok(Self(value))
                }
            }

            #[must_use]
            pub const fn get(self) -> f32 {
                self.0
            }
        }

        impl<'de> serde::Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                let value = <f32 as serde::Deserialize>::deserialize(deserializer)?;
                Self::new(value).map_err(serde::de::Error::custom)
            }
        }
    };
}

positive_f32_newtype!(AngularFrequency);
positive_f32_newtype!(DampingRatio);

impl DevicePixels {
    pub const ZERO: Self = Self(0.0);

    /// # Errors
    /// [`MotionSpecError::NotFinite`] if `value` is `NaN` or infinite,
    /// [`MotionSpecError::Negative`] if it is negative.
    pub fn new(value: f32) -> Result<Self, MotionSpecError> {
        if !value.is_finite() {
            Err(MotionSpecError::NotFinite)
        } else if value < 0.0 {
            Err(MotionSpecError::Negative)
        } else {
            Ok(Self(value))
        }
    }

    #[must_use]
    pub const fn get(self) -> f32 {
        self.0
    }
}

impl<'de> serde::Deserialize<'de> for MotionDuration {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = <Duration as serde::Deserialize>::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

impl<'de> serde::Deserialize<'de> for UnitInterval {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = <f32 as serde::Deserialize>::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

impl<'de> serde::Deserialize<'de> for DevicePixels {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = <f32 as serde::Deserialize>::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

/// A cubic Bézier easing, given by its two control points.
///
/// The endpoints are fixed at (0,0) and (1,1), so only the middle two points
/// are free -- the same four numbers CSS and niri take.
///
/// This lives here rather than as a payload on [`TransitionEasing`] because
/// that type is a *registry* field on four different configs, and the effect
/// registry can only carry scalar property values. A data-carrying easing
/// variant would serialize to an object and break `neomacs-effect-get` for
/// whichever slot selected it -- and `neomacs-effects-apply`, which walks every
/// effect, along with it. Keeping `TransitionEasing` fieldless makes that
/// impossible by construction rather than by discipline.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct UnitBezier {
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
}

impl UnitBezier {
    /// Control points clamped to the unit interval on x, as CSS requires.
    ///
    /// x outside `[0, 1]` makes the curve non-monotonic in time, so solving for
    /// `t` given `x` stops having a unique answer. y is deliberately *not*
    /// clamped: overshoot on y is the whole point of a curve like
    /// `cubic-bezier(0.34, 1.56, 0.64, 1.0)`.
    #[must_use]
    pub fn new(x1: f32, y1: f32, x2: f32, y2: f32) -> Self {
        Self {
            x1: x1.clamp(0.0, 1.0),
            y1,
            x2: x2.clamp(0.0, 1.0),
            y2,
        }
    }

    fn x_for_t(self, t: f64) -> f64 {
        let omt = 1.0 - t;
        3.0 * omt * omt * t * f64::from(self.x1)
            + 3.0 * omt * t * t * f64::from(self.x2)
            + t * t * t
    }

    fn y_for_t(self, t: f64) -> f64 {
        let omt = 1.0 - t;
        3.0 * omt * omt * t * f64::from(self.y1)
            + 3.0 * omt * t * t * f64::from(self.y2)
            + t * t * t
    }

    /// The curve's value at `x`.
    ///
    /// Bisection rather than Newton's method, and 30 iterations rather than a
    /// tolerance: this is libadwaita's algorithm, which niri also uses, so a
    /// curve transcribed from a niri config behaves identically here. Bisection
    /// cannot diverge on the near-vertical segments that a control point at the
    /// edge of the unit interval produces.
    #[must_use]
    pub fn apply(self, x: f32) -> f32 {
        if x <= f32::EPSILON {
            return 0.0;
        }
        if 1.0 - f32::EPSILON <= x {
            return 1.0;
        }
        let x = f64::from(x);
        let (mut low, mut high) = (0.0_f64, 1.0_f64);
        for _ in 0..=30 {
            let guess = f64::midpoint(low, high);
            if x < self.x_for_t(guess) {
                high = guess;
            } else {
                low = guess;
            }
        }
        self.y_for_t(f64::midpoint(low, high)) as f32
    }
}

/// A time-limited motion with a fixed duration and an easing curve.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TweenSpec {
    pub duration: MotionDuration,
    pub easing: TransitionEasing,
    /// The control points, when `easing` is [`TransitionEasing::CubicBezier`].
    ///
    /// `None` for every named curve, and also for a `cubic-bezier` that reached
    /// a config which has nowhere to store control points -- which is why
    /// `TransitionEasing::CubicBezier` has to mean something on its own.
    #[serde(default)]
    pub bezier: Option<UnitBezier>,
}

/// A second-order spring, sampled analytically rather than integrated.
///
/// Storing frequency and damping (instead of a running position/velocity pair)
/// is what makes the motion a *description*: the same spec sampled at the same
/// absolute time always yields the same value, regardless of how many frames
/// were drawn in between.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SpringSpec {
    pub omega: AngularFrequency,
    pub damping: DampingRatio,
}

/// Velocity decay with no target, used for flick/momentum gestures.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DecelerationSpec {
    pub friction: AngularFrequency,
}

/// How a value travels from one committed presentation to the next.
///
/// This is the only generic animation abstraction in the system. It carries no
/// target identity, no property name, no GPU resource and no clock — those
/// belong to the caller that owns the value being moved.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, IntoStaticStr, serde::Serialize, serde::Deserialize,
)]
#[strum(serialize_all = "kebab-case")]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum MotionSpec {
    /// No motion: the destination is displayed on the next frame.
    ///
    /// This is what a disabled or reduced-motion policy resolves to, and what
    /// the minibuffer and interactive resize always use.
    #[default]
    Instant,
    Tween(TweenSpec),
    Spring(SpringSpec),
    Deceleration(DecelerationSpec),
}

impl MotionSpec {
    /// Whether sampling this spec can be skipped entirely.
    ///
    /// A caller that sees `true` must install the destination without
    /// allocating a snapshot, which is what keeps disabled motion free.
    #[must_use]
    pub const fn is_instant(self) -> bool {
        matches!(self, Self::Instant)
    }
}

#[cfg(test)]
#[path = "motion_spec_test.rs"]
mod tests;
