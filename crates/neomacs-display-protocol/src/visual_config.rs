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
}
