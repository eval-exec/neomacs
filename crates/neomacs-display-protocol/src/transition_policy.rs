//! Shared, typed policy for buffer and viewport transitions.

use crate::{
    BufferTransitionConfig, Rect, ScrollTransitionConfig, TransitionEasing, TransitionEffect,
    VisualConfig,
};
use std::time::Duration;
use strum::{EnumString, IntoStaticStr};

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Default,
    serde::Serialize,
    serde::Deserialize,
    EnumString,
    IntoStaticStr,
)]
#[strum(serialize_all = "kebab-case")]
#[serde(rename_all = "kebab-case")]
pub enum TransitionAxisPreference {
    #[default]
    Auto,
    Horizontal,
    Vertical,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    EnumString,
    IntoStaticStr,
)]
#[strum(serialize_all = "kebab-case")]
#[serde(rename_all = "kebab-case")]
pub enum TransitionAxis {
    Horizontal,
    Vertical,
}

impl TransitionAxis {
    pub fn extent(self, bounds: Rect) -> f32 {
        match self {
            Self::Horizontal => bounds.width,
            Self::Vertical => bounds.height,
        }
    }
}

impl TransitionAxisPreference {
    pub fn resolve(self, automatic: TransitionAxis) -> TransitionAxis {
        match self {
            Self::Auto => automatic,
            Self::Horizontal => TransitionAxis::Horizontal,
            Self::Vertical => TransitionAxis::Vertical,
        }
    }
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Default,
    serde::Serialize,
    serde::Deserialize,
    EnumString,
    IntoStaticStr,
)]
#[strum(serialize_all = "kebab-case")]
#[serde(rename_all = "kebab-case")]
pub enum TransitionDirection {
    #[default]
    Forward,
    Backward,
}

impl TransitionDirection {
    pub const fn sign(self) -> f32 {
        match self {
            Self::Forward => 1.0,
            Self::Backward => -1.0,
        }
    }
}

/// Semantic reason for replacing stable content geometry.
///
/// Navigation carries the user's direction across the evaluator/layout/render
/// boundary.  A replacement without navigation deliberately delegates to the
/// configured fallback direction instead of inventing intent from identities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum ContentTransitionIntent {
    #[default]
    Replace,
    Navigate(TransitionDirection),
}

impl ContentTransitionIntent {
    pub const fn resolve(self, fallback: TransitionDirection) -> TransitionDirection {
        match self {
            Self::Replace => fallback,
            Self::Navigate(direction) => direction,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectionlessTransitionEffect {
    Crossfade,
    ScaleZoom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AxisMotionTransitionEffect {
    Slide,
    Parallax,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerticalTransitionEffect {
    FadeEdges,
    Cascade,
    Tilt,
    CylinderRoll,
    Wobbly,
    Wave,
    PerLineSpring,
    Liquid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerticalPostProcessTransitionEffect {
    MotionBlur,
    ChromaticAberration,
    GhostTrails,
    ColorTemperature,
    CRTScanlines,
    DepthOfField,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HorizontalTransitionEffect {
    TypewriterReveal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumString, IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
pub enum TransitionEdge {
    Left,
    Right,
    Top,
    Bottom,
}

impl TransitionEdge {
    pub const fn from_axis_direction(axis: TransitionAxis, direction: TransitionDirection) -> Self {
        match (axis, direction) {
            (TransitionAxis::Horizontal, TransitionDirection::Forward) => Self::Right,
            (TransitionAxis::Horizontal, TransitionDirection::Backward) => Self::Left,
            (TransitionAxis::Vertical, TransitionDirection::Forward) => Self::Bottom,
            (TransitionAxis::Vertical, TransitionDirection::Backward) => Self::Top,
        }
    }
}

/// Renderer-ready transition effect. Directionless effects cannot carry a
/// direction, while intrinsic text-flow effects cannot accidentally be asked
/// to run along an unsupported axis.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ResolvedTransitionEffect {
    Directionless(DirectionlessTransitionEffect),
    AxisMotion {
        effect: AxisMotionTransitionEffect,
        axis: TransitionAxis,
        direction: TransitionDirection,
        distance: f32,
    },
    CardFlip {
        axis: TransitionAxis,
    },
    PageCurl {
        edge: TransitionEdge,
    },
    Vertical {
        effect: VerticalTransitionEffect,
        direction: TransitionDirection,
        distance: f32,
    },
    VerticalPostProcess {
        effect: VerticalPostProcessTransitionEffect,
        direction: TransitionDirection,
        distance: f32,
    },
    Horizontal {
        effect: HorizontalTransitionEffect,
        direction: TransitionDirection,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TransitionPlan {
    pub duration: Duration,
    pub easing: TransitionEasing,
    pub bounds: Rect,
    pub effect: ResolvedTransitionEffect,
}

fn resolve_effect(
    effect: TransitionEffect,
    axis: TransitionAxis,
    direction: TransitionDirection,
    axis_distance: f32,
    vertical_distance: f32,
) -> ResolvedTransitionEffect {
    match effect {
        TransitionEffect::Crossfade => {
            ResolvedTransitionEffect::Directionless(DirectionlessTransitionEffect::Crossfade)
        }
        TransitionEffect::ScaleZoom => {
            ResolvedTransitionEffect::Directionless(DirectionlessTransitionEffect::ScaleZoom)
        }
        TransitionEffect::Slide => ResolvedTransitionEffect::AxisMotion {
            effect: AxisMotionTransitionEffect::Slide,
            axis,
            direction,
            distance: axis_distance,
        },
        TransitionEffect::Parallax => ResolvedTransitionEffect::AxisMotion {
            effect: AxisMotionTransitionEffect::Parallax,
            axis,
            direction,
            distance: axis_distance,
        },
        TransitionEffect::CardFlip => ResolvedTransitionEffect::CardFlip { axis },
        TransitionEffect::PageCurl => ResolvedTransitionEffect::PageCurl {
            edge: TransitionEdge::from_axis_direction(axis, direction),
        },
        TransitionEffect::TypewriterReveal => ResolvedTransitionEffect::Horizontal {
            effect: HorizontalTransitionEffect::TypewriterReveal,
            direction,
        },
        TransitionEffect::FadeEdges => vertical(
            VerticalTransitionEffect::FadeEdges,
            direction,
            vertical_distance,
        ),
        TransitionEffect::Cascade => vertical(
            VerticalTransitionEffect::Cascade,
            direction,
            vertical_distance,
        ),
        TransitionEffect::Tilt => {
            vertical(VerticalTransitionEffect::Tilt, direction, vertical_distance)
        }
        TransitionEffect::CylinderRoll => vertical(
            VerticalTransitionEffect::CylinderRoll,
            direction,
            vertical_distance,
        ),
        TransitionEffect::Wobbly => vertical(
            VerticalTransitionEffect::Wobbly,
            direction,
            vertical_distance,
        ),
        TransitionEffect::Wave => {
            vertical(VerticalTransitionEffect::Wave, direction, vertical_distance)
        }
        TransitionEffect::PerLineSpring => vertical(
            VerticalTransitionEffect::PerLineSpring,
            direction,
            vertical_distance,
        ),
        TransitionEffect::Liquid => vertical(
            VerticalTransitionEffect::Liquid,
            direction,
            vertical_distance,
        ),
        TransitionEffect::MotionBlur => vertical_post_process(
            VerticalPostProcessTransitionEffect::MotionBlur,
            direction,
            vertical_distance,
        ),
        TransitionEffect::ChromaticAberration => vertical_post_process(
            VerticalPostProcessTransitionEffect::ChromaticAberration,
            direction,
            vertical_distance,
        ),
        TransitionEffect::GhostTrails => vertical_post_process(
            VerticalPostProcessTransitionEffect::GhostTrails,
            direction,
            vertical_distance,
        ),
        TransitionEffect::ColorTemperature => vertical_post_process(
            VerticalPostProcessTransitionEffect::ColorTemperature,
            direction,
            vertical_distance,
        ),
        TransitionEffect::CRTScanlines => vertical_post_process(
            VerticalPostProcessTransitionEffect::CRTScanlines,
            direction,
            vertical_distance,
        ),
        TransitionEffect::DepthOfField => vertical_post_process(
            VerticalPostProcessTransitionEffect::DepthOfField,
            direction,
            vertical_distance,
        ),
    }
}

fn vertical_post_process(
    effect: VerticalPostProcessTransitionEffect,
    direction: TransitionDirection,
    distance: f32,
) -> ResolvedTransitionEffect {
    ResolvedTransitionEffect::VerticalPostProcess {
        effect,
        direction,
        distance,
    }
}

fn vertical(
    effect: VerticalTransitionEffect,
    direction: TransitionDirection,
    distance: f32,
) -> ResolvedTransitionEffect {
    ResolvedTransitionEffect::Vertical {
        effect,
        direction,
        distance,
    }
}

/// Animation policy for per-window transitions.
///
/// This is the authoritative transition config shared across crates; render
/// code consumes this policy instead of owning separate config fields.
/// Default minimum viewport height, in device pixels, for a scroll transition.
const MINIMUM_SCROLL_REGION_HEIGHT_PX: f32 = 50.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransitionPolicy {
    pub buffer: BufferTransitionConfig,
    pub scroll: ScrollTransitionConfig,
}

impl TransitionPolicy {
    /// True when at least one transition path needs offscreen snapshots.
    pub fn needs_offscreen(&self) -> bool {
        self.buffer.enabled || self.scroll.enabled
    }

    pub fn buffer_plan(
        &self,
        bounds: Rect,
        intent: ContentTransitionIntent,
    ) -> Option<TransitionPlan> {
        if !self.buffer.enabled {
            return None;
        }
        let axis = self.buffer.axis.resolve(TransitionAxis::Horizontal);
        let direction = intent.resolve(self.buffer.direction);
        Some(TransitionPlan {
            duration: self.buffer.duration,
            easing: self.buffer.easing,
            bounds,
            effect: resolve_effect(
                self.buffer.effect,
                axis,
                direction,
                axis.extent(bounds),
                bounds.height,
            ),
        })
    }

    /// Smallest viewport a scroll transition is worth running in.
    ///
    /// Below this a slide is more distracting than informative, and the
    /// retained strip is mostly chrome. Expressed in device pixels so the
    /// judgement means the same thing on a HiDPI display as on a 1x one.
    #[must_use]
    pub fn minimum_scroll_region_height(&self) -> f32 {
        MINIMUM_SCROLL_REGION_HEIGHT_PX
    }

    pub fn scroll_plan(
        &self,
        bounds: Rect,
        direction: TransitionDirection,
        distance: f32,
    ) -> Option<TransitionPlan> {
        if !self.scroll.enabled {
            return None;
        }
        let distance = distance.max(0.0).min(bounds.height);
        Some(TransitionPlan {
            duration: self.scroll.duration,
            easing: self.scroll.easing,
            bounds,
            effect: resolve_effect(
                self.scroll.effect,
                TransitionAxis::Vertical,
                direction,
                distance,
                distance,
            ),
        })
    }
}

impl Default for TransitionPolicy {
    fn default() -> Self {
        Self::from(&VisualConfig::default())
    }
}

impl From<&VisualConfig> for TransitionPolicy {
    fn from(config: &VisualConfig) -> Self {
        Self {
            buffer: config.buffer_transition,
            scroll: config.scroll_transition,
        }
    }
}

#[cfg(test)]
#[path = "transition_policy_test.rs"]
mod tests;
