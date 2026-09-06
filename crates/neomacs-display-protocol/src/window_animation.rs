//! How a window layout change animates: four independently configurable slots
//! plus one global, in niri's vocabulary.
//!
//! A morph classifies every pane as entering, leaving, or persisting, and each
//! of those asks a different question. A pane appearing has no previous rect to
//! travel from; a pane leaving has no destination; a pane that persists has
//! both, and whether its *size* changed decides whether its text rewrapped.
//! One curve cannot answer all three well, which is why niri splits them and
//! why this does too.
//!
//! # Why six scalars per slot rather than a `MotionSpec`
//!
//! The effect registry reflects [`crate::visual_config::VisualConfig`] through
//! serde and can carry only scalar property values. Every `MotionSpec` variant
//! but `Instant` serializes to an *object*, so storing one here would make
//! `neomacs-effect-get` fail for the slot -- and `neomacs-effects-apply`, which
//! walks every effect, with it. [`WindowAnimation::motion`] converts to the
//! form the compositor samples.
//!
//! Every field is present whichever `kind` is selected, which is the registry's
//! requirement rather than a modelling choice: a property key is validated
//! against the value already stored, so a shape carrying only the active
//! variant's fields would make switching kind from Lisp unreachable --
//! `(neomacs-effect-set 'window-resize :kind 'spring :stiffness 800)` would
//! fail with "has no property `stiffness`".
//!
//! # Why `kind` rather than overloading `easing`
//!
//! [`TransitionEasing::Spring`] already exists and is a *different thing*: a
//! fixed-shape curve over normalized tween time. Letting `:easing spring` mean
//! "a real second-order spring parameterised by damping-ratio and stiffness"
//! here, while it keeps its old meaning on `scroll-transition`, is a trap this
//! file would have to document forever.

use crate::motion_spec::{
    AngularFrequency, DampingRatio, MotionDuration, MotionSpec, SpringSpec, TweenSpec, UnitBezier,
};
use crate::scroll_animation::TransitionEasing;
use std::time::Duration;

/// Which family of curve a slot uses.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MotionKind {
    /// A fixed-duration curve. Reads `duration` and `easing`.
    #[default]
    Easing,
    /// A second-order spring. Reads `damping_ratio` and `stiffness`.
    ///
    /// A spring has no duration, so [`WindowAnimationsConfig::slowdown`] is the
    /// only way to stretch one out far enough to watch.
    Spring,
}

/// One animation slot.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WindowAnimation {
    pub enabled: bool,
    pub kind: MotionKind,
    /// Read when `kind` is `easing`. Zero disables the slot.
    ///
    /// A plain `Duration`, not a `MotionDuration`: the latter rejects zero at
    /// deserialize time, which would turn `:duration 0` from "no motion" into
    /// an error message naming a Rust type.
    pub duration: Duration,
    /// Read when `kind` is `easing`.
    pub easing: TransitionEasing,
    /// The Bézier control points, read when `easing` is `cubic-bezier`.
    ///
    /// Four scalars rather than one curve object, for the same reason every
    /// other field here is a scalar: the registry cannot carry an object. They
    /// are the same four numbers a niri config writes as
    /// `cubic-bezier(x1, y1, x2, y2)`, so a curve can be transcribed directly.
    pub bezier_x1: f32,
    pub bezier_y1: f32,
    pub bezier_x2: f32,
    pub bezier_y2: f32,
    /// Read when `kind` is `spring`. Clamped to niri's own `0.1..=10.0`.
    pub damping_ratio: f32,
    /// Read when `kind` is `spring`. Clamped to at least 1, as niri requires.
    pub stiffness: u32,
}

impl WindowAnimation {
    const fn easing(millis: u64, easing: TransitionEasing) -> Self {
        Self {
            enabled: true,
            kind: MotionKind::Easing,
            duration: Duration::from_millis(millis),
            easing,
            // The identity curve, so a slot switched to `cubic-bezier` without
            // being given points animates linearly rather than jumping.
            bezier_x1: 0.0,
            bezier_y1: 0.0,
            bezier_x2: 1.0,
            bezier_y2: 1.0,
            damping_ratio: 1.0,
            stiffness: 800,
        }
    }

    const fn spring(damping_ratio: f32, stiffness: u32) -> Self {
        Self {
            enabled: true,
            kind: MotionKind::Spring,
            duration: Duration::from_millis(150),
            easing: TransitionEasing::EaseOutQuad,
            bezier_x1: 0.0,
            bezier_y1: 0.0,
            bezier_x2: 1.0,
            bezier_y2: 1.0,
            damping_ratio,
            stiffness,
        }
    }

    /// The spec the compositor samples, or `Instant` when there is nothing to
    /// animate.
    ///
    /// `Instant` is not merely a fast path: a caller holding it builds no
    /// motion, takes no offscreen composition, and draws exactly as it would
    /// with the feature absent.
    ///
    /// Out-of-range values are clamped rather than rejected. `apply_effects` is
    /// all-or-nothing and `neomacs-effects` is a defcustom whose `:set` calls
    /// it, so rejecting one taste parameter would silently revert the user's
    /// entire effect profile. Values that are genuinely unrepresentable --
    /// negative, NaN, fractional stiffness -- are refused earlier by the
    /// registry, with a good error.
    #[must_use]
    pub fn motion(&self, globals: WindowAnimationsConfig) -> MotionSpec {
        if !self.enabled || globals.off {
            return MotionSpec::Instant;
        }
        let slowdown = if globals.slowdown.is_finite() {
            globals.slowdown.clamp(MIN_SLOWDOWN, MAX_SLOWDOWN)
        } else {
            1.0
        };
        match self.kind {
            // Slowdown scales the duration, which is exact by construction.
            MotionKind::Easing => MotionDuration::new(self.duration.mul_f32(slowdown)).map_or(
                MotionSpec::Instant,
                |duration| {
                    MotionSpec::Tween(TweenSpec {
                        duration,
                        easing: self.easing,
                        bezier: matches!(self.easing, TransitionEasing::CubicBezier).then(|| {
                            UnitBezier::new(
                                self.bezier_x1,
                                self.bezier_y1,
                                self.bezier_x2,
                                self.bezier_y2,
                            )
                        }),
                    })
                },
            ),
            // Slowdown divides omega and leaves damping alone. Also exact, not
            // an approximation: omega appears in `spring_progress` only ever
            // multiplied by t, in all three damping regimes, so
            // `x(t; omega/s, zeta) == x(t/s; omega, zeta)` identically.
            MotionKind::Spring => {
                let omega = self.omega() / slowdown;
                let zeta = if self.damping_ratio.is_finite() {
                    self.damping_ratio
                        .clamp(MIN_DAMPING_RATIO, MAX_DAMPING_RATIO)
                } else {
                    1.0
                };
                match (AngularFrequency::new(omega), DampingRatio::new(zeta)) {
                    (Ok(omega), Ok(damping)) => MotionSpec::Spring(SpringSpec { omega, damping }),
                    _ => MotionSpec::Instant,
                }
            }
        }
    }

    /// This slot's undamped angular frequency, in radians per second.
    ///
    /// niri parameterises a spring as `{damping_ratio, stiffness, epsilon}`
    /// with mass fixed at 1, and computes `omega0 = sqrt(stiffness / mass)`.
    /// With mass 1 that is `sqrt(stiffness)`, and niri's `damping_ratio` is our
    /// `damping` verbatim -- their `beta = damping / 2m` expands to
    /// `damping_ratio * sqrt(k/m)`, which is `zeta * omega`. So the two
    /// parameterisations describe the same spring and the conversion is a
    /// square root with no scaling anywhere else.
    #[must_use]
    pub fn omega(&self) -> f32 {
        f64::from(self.stiffness.max(1)).sqrt() as f32
    }
}

/// niri's documented range for a damping ratio.
const MIN_DAMPING_RATIO: f32 = 0.1;
const MAX_DAMPING_RATIO: f32 = 10.0;

/// The floor is not taste. `Motion::rate_at` is a central difference with a
/// fixed step, and its answer gates both the finish test and the hand-off when
/// a motion is interrupted, so it stays meaningful only while `omega * step`
/// is small. Speeding a spring up without bound makes that rate noise.
const MIN_SLOWDOWN: f32 = 0.05;
/// Generous on purpose: this is how a spring gets watched. It has no duration
/// to lengthen by hand, so before this existed the only way to see one was to
/// rebuild with a probe.
const MAX_SLOWDOWN: f32 = 20.0;

/// Global controls over every slot, matching niri's `animations { }` block.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WindowAnimationsConfig {
    /// Master off switch. Every slot resolves to `MotionSpec::Instant`.
    pub off: bool,
    /// Multiplies every slot's wall-clock length. `1.0` is normal speed.
    pub slowdown: f32,
}

impl Default for WindowAnimationsConfig {
    fn default() -> Self {
        Self {
            off: false,
            slowdown: 1.0,
        }
    }
}

/// Opening a window: niri's `window-open`, 150ms ease-out-expo.
#[must_use]
pub fn default_window_open() -> WindowAnimation {
    WindowAnimation::easing(150, TransitionEasing::EaseOutExpo)
}

/// Closing a window: niri's `window-close` values, but **off**.
///
/// The values are niri's and the slot is fully configurable; only the default
/// differs, and deliberately. niri can fade a closing window because it is an
/// overlay owning no layout space. Here a departing pane's ground is being
/// taken by a neighbour growing across it, and the old picture is drawn *over*
/// the destination -- so fading it blends the deleted window with the pane
/// replacing it against a backdrop already showing the settled layout. That is
/// the doubled, half-transparent text that `delete-window` shipped with for one
/// afternoon.
#[must_use]
pub fn default_window_close() -> WindowAnimation {
    WindowAnimation {
        enabled: false,
        ..WindowAnimation::easing(150, TransitionEasing::EaseOutQuad)
    }
}

/// Resizing a pane: niri's `window-resize`, a critically damped spring.
#[must_use]
pub fn default_window_resize() -> WindowAnimation {
    WindowAnimation::spring(1.0, 800)
}

/// Moving a pane without resizing it: niri's `window-movement`, same spring.
#[must_use]
pub fn default_window_movement() -> WindowAnimation {
    WindowAnimation::spring(1.0, 800)
}

#[cfg(test)]
#[path = "window_animation_test.rs"]
mod tests;
