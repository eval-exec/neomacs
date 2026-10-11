//! How a child frame's lifecycle animates: four slots in the same vocabulary
//! the window animations use.
//!
//! A child frame (posframe, Corfu, company popup) is a floating overlay
//! composited over its parent, so its animation questions are the window
//! ones minus the tiling constraints: it can fade in when it appears, fade
//! out when Emacs deletes it, and drift or resize as it re-anchors. Two of
//! those ship enabled, two ship off:
//!
//! * `open` — on. A popup fading in costs nothing beyond drawing the popup
//!   itself across the animation, which the scheduler already does for the
//!   animated cursor.
//! * `close` — on. Unlike a deleted *window*, a deleted child frame leaves no
//!   successor growing across its ground: the parent's pixels were always
//!   underneath, so fading it out has no doubled-picture artifact to
//!   apologise for.
//! * `movement` and `resize` — off. Both change where the *next* update
//!   lands relative to where Emacs anchored the frame, and a popup whose
//!   anchor is exact (a tooltip under one specific character) reads a
//!   few-pixels-behind target as a positioning bug. They are fully
//!   configurable for users who prefer the drift.
//!
//! # Why a copy of the ten window-animation slots rather than a shared type
//!
//! [`crate::window_animation::WindowAnimation`] is deliberately flat: the
//! effect registry reflects config structs through serde and can carry only
//! scalar property values. Two slots here need one property the window ones
//! do not have — `slide-pixels`, how far the frame displaces while it fades
//! — and nesting a `WindowAnimation` inside this struct would serialize to
//! an object and break `neomacs-effect-get` for the slot. So the ten scalars
//! are restated here and the conversion to a [`MotionSpec`] mirrors
//! [`crate::window_animation::WindowAnimation::motion`] instead of sharing
//! code. The test beside the definition pins the two behaviours together;
//! that is cheaper than a registry that cannot express the slot.

use crate::motion_spec::{
    AngularFrequency, DampingRatio, MotionDuration, MotionSpec, SpringSpec, TweenSpec, UnitBezier,
};
use crate::scroll_animation::TransitionEasing;
use crate::window_animation::{
    MAX_DAMPING_RATIO, MAX_SLOWDOWN, MIN_DAMPING_RATIO, MIN_SLOWDOWN, MotionKind,
};
use std::time::Duration;

/// One child-frame lifecycle slot.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ChildFrameAnimation {
    pub enabled: bool,
    pub kind: MotionKind,
    /// Read when `kind` is `easing`. Zero disables the slot.
    pub duration: Duration,
    /// Read when `kind` is `easing`.
    pub easing: TransitionEasing,
    /// Bezier control points, read when `easing` is `cubic-bezier`.
    pub bezier_x1: f32,
    pub bezier_y1: f32,
    pub bezier_x2: f32,
    pub bezier_y2: f32,
    /// Read when `kind` is `spring`. Clamped to niri's own `0.1..=10.0`.
    pub damping_ratio: f32,
    /// Read when `kind` is `spring`. Clamped to at least 1, as niri requires.
    pub stiffness: u32,
    /// How far the frame displaces along the vertical axis while it animates,
    /// in logical pixels.
    ///
    /// An opening frame starts this far *below* its placement and rises to
    /// rest; a closing frame falls this far while it fades. Zero (the `close`
    /// default) reduces the slot to a pure fade. Displacement follows the
    /// same curve as the fade, so a spring-shaped slot overshoots through its
    /// placement and settles back — the property a pop-in reads as.
    pub slide_pixels: f32,
    /// The scale the frame starts from, as a fraction of its settled size.
    ///
    /// `1.0` (every slot's default) scales nothing: the transform is an
    /// identity in the draw pipeline and costs nothing. Below 1.0, an
    /// arriving frame grows from this fraction of its size — anchored at
    /// its own top-left, so it grows outward from the point that anchored
    /// it — while a departing frame shrinks toward it. The scale shares the
    /// slot's curve, unclamped like the slide: a spring's overshoot past
    /// 1.0 is the point.
    pub scale_from: f32,
}

impl ChildFrameAnimation {
    const fn easing(millis: u64, easing: TransitionEasing, slide_pixels: f32) -> Self {
        Self {
            enabled: true,
            kind: MotionKind::Easing,
            duration: Duration::from_millis(millis),
            easing,
            bezier_x1: 0.0,
            bezier_y1: 0.0,
            bezier_x2: 1.0,
            bezier_y2: 1.0,
            damping_ratio: 1.0,
            stiffness: 800,
            slide_pixels,
            scale_from: 1.0,
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
            slide_pixels: 0.0,
            scale_from: 1.0,
        }
    }

    /// The spec the renderer samples, or `Instant` when there is nothing to
    /// animate.
    ///
    /// Clamped rather than rejected, for the same reason
    /// [`crate::window_animation::WindowAnimation::motion`] is: a partial
    /// effect-profile push must not silently revert the user's whole child
    /// frame configuration because one number was out of range.
    #[must_use]
    pub fn motion(&self, globals: ChildFrameAnimationsConfig) -> MotionSpec {
        if !self.enabled || globals.off {
            return MotionSpec::Instant;
        }
        let slowdown = if globals.slowdown.is_finite() {
            globals.slowdown.clamp(MIN_SLOWDOWN, MAX_SLOWDOWN)
        } else {
            1.0
        };
        match self.kind {
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
    /// Same parameterisation as
    /// [`crate::window_animation::WindowAnimation::omega`]; see that method
    /// for the niri conversion.
    #[must_use]
    pub fn omega(&self) -> f32 {
        f64::from(self.stiffness.max(1)).sqrt() as f32
    }
}

/// Global controls over every child-frame slot, matching
/// [`crate::window_animation::WindowAnimationsConfig`].
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ChildFrameAnimationsConfig {
    /// Master off switch. Every slot resolves to `MotionSpec::Instant`.
    pub off: bool,
    /// Multiplies every slot's wall-clock length. `1.0` is normal speed.
    pub slowdown: f32,
}

impl Default for ChildFrameAnimationsConfig {
    fn default() -> Self {
        Self {
            off: false,
            slowdown: 1.0,
        }
    }
}

/// A child frame appearing: 150ms ease-out-expo with an 8px rise.
#[must_use]
pub fn default_child_frame_open() -> ChildFrameAnimation {
    ChildFrameAnimation::easing(150, TransitionEasing::EaseOutExpo, 8.0)
}

/// A child frame going away: 150ms ease-out quad, pure fade.
#[must_use]
pub fn default_child_frame_close() -> ChildFrameAnimation {
    ChildFrameAnimation::easing(150, TransitionEasing::EaseOutQuad, 0.0)
}

/// Drift toward a new anchor: a critically damped spring, **off**.
#[must_use]
pub fn default_child_frame_movement() -> ChildFrameAnimation {
    ChildFrameAnimation {
        enabled: false,
        ..ChildFrameAnimation::spring(1.0, 800)
    }
}

/// A child frame whose content resize arrives: a critically damped spring,
/// **off** and reserved.
///
/// The glyph content a child frame draws is fixed per update, so a geometry
/// animation between two sizes would stretch one picture across both rects.
/// This slot is wired for a future content-crossfade implementation that can
/// blend two updates rather than stretch one; until then a resize is instant
/// exactly as GNU Emacs does it.
#[must_use]
pub fn default_child_frame_resize() -> ChildFrameAnimation {
    ChildFrameAnimation {
        enabled: false,
        ..ChildFrameAnimation::spring(1.0, 800)
    }
}

#[cfg(test)]
#[path = "child_frame_animation/tests/child_frame_animation_test.rs"]
mod tests;

crate::effect_schema!(ChildFrameAnimation {
    enabled: bool,
    kind: MotionKind,
    duration: Duration,
    easing: TransitionEasing,
    bezier_x1: f32,
    bezier_y1: f32,
    bezier_x2: f32,
    bezier_y2: f32,
    damping_ratio: f32,
    stiffness: u32,
    slide_pixels: f32,
    scale_from: f32,
});
crate::effect_schema!(ChildFrameAnimationsConfig {
    off: bool,
    slowdown: f32
});
