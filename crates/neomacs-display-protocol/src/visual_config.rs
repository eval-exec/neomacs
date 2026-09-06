//! Complete user-facing visual configuration snapshot.
//!
//! Shader effects, cursor behavior, and window transitions share one control
//! plane even though the renderer stores and executes them in different
//! subsystems.  Keeping that distinction behind `VisualConfig` gives Elisp a
//! single named, typed, atomic interface without flattening the runtime model.

use crate::{
    CursorAnimStyle, EffectsConfig, TransitionAxisPreference, TransitionDirection,
    TransitionEasing, TransitionEffect,
};
use std::time::Duration;

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CursorBlinkConfig {
    pub enabled: bool,
    pub interval: Duration,
}

impl Default for CursorBlinkConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            interval: Duration::from_millis(500),
        }
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CursorMotionConfig {
    pub enabled: bool,
    pub speed: f32,
    pub style: CursorAnimStyle,
    pub duration: Duration,
    pub trail_size: f32,
}

impl Default for CursorMotionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            speed: 2.4,
            style: CursorAnimStyle::CriticallyDampedSpring,
            duration: Duration::from_millis(150),
            trail_size: 0.7,
        }
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CursorSizeTransitionConfig {
    pub enabled: bool,
    pub duration: Duration,
}

impl Default for CursorSizeTransitionConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            duration: Duration::from_millis(150),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BufferTransitionConfig {
    pub enabled: bool,
    pub duration: Duration,
    pub effect: TransitionEffect,
    pub easing: TransitionEasing,
    /// Preferred orientation for effects that support either axis.
    pub axis: TransitionAxisPreference,
    /// Semantic movement direction for effects that move content. Forward
    /// moves old content left/up and introduces new content from the
    /// right/bottom.
    pub direction: TransitionDirection,
}

impl Default for BufferTransitionConfig {
    fn default() -> Self {
        Self {
            // Off by default: switching buffers repaints instantly, matching
            // stock Emacs, the same choice ScrollTransitionConfig makes below.
            // Opt in at runtime with
            //   (neomacs-effect-set 'buffer-transition :enabled t)
            // or via the `neomacs-effects' profile. The effect/duration/easing
            // below are the values used once it is enabled.
            enabled: false,
            duration: Duration::from_millis(200),
            effect: TransitionEffect::Slide,
            easing: TransitionEasing::EaseOutQuad,
            axis: TransitionAxisPreference::Auto,
            direction: TransitionDirection::Forward,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ScrollTransitionConfig {
    pub enabled: bool,
    pub duration: Duration,
    pub effect: TransitionEffect,
    pub easing: TransitionEasing,
}

impl Default for ScrollTransitionConfig {
    fn default() -> Self {
        Self {
            // Off by default: C-v/M-v (and other scrolls) update instantly,
            // matching stock Emacs. Opt in at runtime with
            //   (neomacs-effect-set 'scroll-transition :enabled t)
            // or via the `neomacs-effects' profile. The effect/duration/easing
            // below are the values used once it is enabled.
            enabled: false,
            duration: Duration::from_millis(150),
            effect: TransitionEffect::Slide,
            easing: TransitionEasing::EaseOutQuad,
        }
    }
}

/// How panes travel when a layout change moves them.
///
/// Splitting a window, deleting one, or resizing the frame rearranges every
/// pane at once; committed as a single presentation, that arrives as a jump.
/// This says whether — and how — the compositor carries them there instead.
///
/// The shape is three scalars rather than a [`MotionSpec`] field, and that is
/// load-bearing: the effect registry reflects `VisualConfig` through serde and
/// can only carry scalar property values (plus the one special case for
/// `Duration`). A `MotionSpec` serializes to an externally tagged *object* for
/// every variant but `Instant`, so storing one here would make
/// `neomacs-effect-get 'pane-motion` fail the moment motion was switched on —
/// and `neomacs-effects-apply`, which walks every effect, with it.
/// [`Self::movement`] converts to the precise form the compositor samples.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PaneMotionConfig {
    pub enabled: bool,
    pub duration: Duration,
    pub easing: TransitionEasing,
}

impl Default for PaneMotionConfig {
    /// On.
    ///
    /// It was off for as long as nothing could watch it: pane motion is
    /// disabled on a software adapter, which is the only adapter class
    /// available under Xvfb (llvmpipe, and lavapipe under
    /// `WGPU_BACKEND=vulkan`), so no headless run could execute the per-pane
    /// pass end to end. Shipping it on that evidence would have meant an
    /// animation that had never once run in a live editor, on every layout
    /// change and every echo-area resize.
    ///
    /// It has since been watched on hardware — the divider sweep across a
    /// `C-x 3` verified frame by frame, `tmp/impl/capture-split.sh` — and that
    /// watching is what found the last two defects, both of which rendered as a
    /// completely static frame while every placement in the model interpolated
    /// perfectly: a morph with no pinned picture to fade from, and a vacated
    /// strip that crossfaded instead of holding.
    ///
    /// Enabling it has a standing cost, not just a per-motion one. A morph
    /// needs the frame as it was *before* the change, and a picture that was
    /// never composed offscreen was never kept — so with this on, every frame
    /// composes through the ring: two resident full-frame textures and one
    /// extra full-frame blit per frame, whether or not anything is moving. See
    /// `RenderFeaturePlan::compose_offscreen`. Turning this off gets that back.
    fn default() -> Self {
        Self {
            enabled: true,
            duration: Duration::from_millis(160),
            easing: TransitionEasing::EaseOutCubic,
        }
    }
}

impl PaneMotionConfig {
    /// How a pane travels under this configuration.
    ///
    /// [`MotionSpec::Instant`] whenever there is nothing to animate — disabled,
    /// or a duration of zero. That is not merely the fast path: a caller that
    /// sees an instant spec builds no motion, takes no offscreen and composes
    /// exactly as it would with this feature absent.
    #[must_use]
    pub fn movement(&self) -> crate::motion_spec::MotionSpec {
        use crate::motion_spec::{MotionDuration, MotionSpec, TweenSpec};
        if !self.enabled {
            return MotionSpec::Instant;
        }
        MotionDuration::new(self.duration).map_or(MotionSpec::Instant, |duration| {
            MotionSpec::Tween(TweenSpec {
                duration,
                easing: self.easing,
            })
        })
    }
}

/// Desired visual configuration owned by the evaluator and published as one
/// snapshot to the render thread.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct VisualConfig {
    /// The large shader-effect catalog remains a focused renderer type.  Serde
    /// flattening exposes it beside the behavioral configs in the registry.
    #[serde(flatten)]
    pub effects: EffectsConfig,
    pub cursor_blink: CursorBlinkConfig,
    pub cursor_motion: CursorMotionConfig,
    pub cursor_size_transition: CursorSizeTransitionConfig,
    pub buffer_transition: BufferTransitionConfig,
    pub scroll_transition: ScrollTransitionConfig,
    #[serde(default)]
    pub pane_motion: PaneMotionConfig,
}
