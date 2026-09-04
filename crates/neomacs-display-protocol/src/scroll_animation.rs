//! Scroll animation system.
//!
//! Provides a rich set of scroll transition effects, physics simulations,
//! post-processing shader effects, and geometric deformations.
//!
//! # Architecture
//!
//! Scroll effects are organized into categories:
//!
//! - **Transition effects**: How old/new content visually transition
//!   (Slide, Crossfade, ScaleZoom, FadeEdges, Cascade, Parallax)
//! - **3D effects**: Perspective-projected transformations
//!   (Tilt, PageCurl, CardFlip, CylinderRoll)
//! - **Deformation effects**: Per-line vertex displacement
//!   (Wobbly, Wave, PerLineSpring, Liquid)
//! - **Post-processing effects**: Full-screen shader passes
//!   (MotionBlur, ChromaticAberration, GhostTrails, ColorTemperature,
//!   CRTScanlines, DepthOfField)
//! - **Creative effects**: Special rendering techniques
//!   (TypewriterReveal)
//!
//! Each effect is selected via [`TransitionEffect`] enum. Physics-based timing
//! is controlled separately via [`TransitionEasing`].

use strum::{EnumString, IntoStaticStr};

/// All available snapshot-transition effects.
///
/// Each variant represents a complete visual style for scroll transitions.
/// Select one at a time via configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, EnumString, IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TransitionEffect {
    // ── Transition effects (2D, vertex position/alpha changes) ──────────
    /// Default: old content slides out, new content slides in.
    #[default]
    Slide,

    /// Alpha blend between old and new content.
    Crossfade,

    /// Destination appears at 95% scale and zooms to 100%.
    #[strum(to_string = "scale-zoom", serialize = "scalezoom", serialize = "zoom")]
    ScaleZoom,

    /// Lines fade in/out at viewport edges with soft vignette.
    #[strum(to_string = "fade-edges", serialize = "fadeedges", serialize = "fade")]
    FadeEdges,

    /// Lines drop in with staggered delay (waterfall effect).
    #[strum(to_string = "cascade", serialize = "waterfall")]
    Cascade,

    /// Different layers scroll at different speeds for depth illusion.
    #[strum(to_string = "parallax", serialize = "depth")]
    Parallax,

    // ── 3D effects (perspective-projected vertex transforms) ────────────
    /// Buffer tilts 1-3° around X-axis while scrolling, springs back flat.
    #[strum(to_string = "tilt", serialize = "perspective")]
    Tilt,

    /// Current screen curls away like a turning book page.
    #[strum(to_string = "page-curl", serialize = "pagecurl", serialize = "curl")]
    PageCurl,

    /// Screenful flips like a card rotating around the X-axis.
    #[strum(to_string = "card-flip", serialize = "cardflip", serialize = "flip")]
    CardFlip,

    /// Content wraps around a vertical cylinder; scrolling rotates it.
    #[strum(
        to_string = "cylinder-roll",
        serialize = "cylinderroll",
        serialize = "cylinder",
        serialize = "roll"
    )]
    CylinderRoll,

    // ── Deformation effects (per-line vertex displacement) ──────────────
    /// Content deforms like gelatin; top moves first, bottom lags.
    #[strum(to_string = "wobbly", serialize = "jelly", serialize = "wobble")]
    Wobbly,

    /// Horizontal sine-wave displacement propagates through text.
    #[strum(to_string = "wave", serialize = "sine")]
    Wave,

    /// Each line on its own spring; scroll propagates with stagger delay.
    #[strum(
        to_string = "per-line-spring",
        serialize = "perlinespring",
        serialize = "line-spring",
        serialize = "slinky"
    )]
    PerLineSpring,

    /// Noise-based UV warping; text ripples like viewed through water.
    #[strum(to_string = "liquid", serialize = "fluid", serialize = "water")]
    Liquid,

    // ── Post-processing effects (full-screen shader passes) ─────────────
    /// Vertical motion blur proportional to scroll speed.
    #[strum(
        to_string = "motion-blur",
        serialize = "motionblur",
        serialize = "blur"
    )]
    MotionBlur,

    /// RGB channels separate vertically during fast scroll.
    #[strum(
        to_string = "chromatic-aberration",
        serialize = "chromaticaberration",
        serialize = "chromatic",
        serialize = "aberration"
    )]
    ChromaticAberration,

    /// Semi-transparent afterimages trail behind content.
    #[strum(
        to_string = "ghost-trails",
        serialize = "ghosttrails",
        serialize = "ghost",
        serialize = "trails"
    )]
    GhostTrails,

    /// Warm tint scrolling down, cool tint scrolling up.
    #[strum(
        to_string = "color-temperature",
        serialize = "colortemperature",
        serialize = "color-temp",
        serialize = "temperature"
    )]
    ColorTemperature,

    /// Retro scanline overlay sweeps with scroll position.
    #[strum(
        to_string = "crt-scanlines",
        serialize = "crtscanlines",
        serialize = "crt",
        serialize = "scanlines"
    )]
    #[serde(rename = "crt-scanlines")]
    CRTScanlines,

    /// Center sharp, edges blurred during fast scroll.
    #[strum(
        to_string = "depth-of-field",
        serialize = "depthoffield",
        serialize = "dof"
    )]
    DepthOfField,

    // ── Creative effects (special rendering) ────────────────────────────
    /// New lines appear character-by-character left-to-right.
    #[strum(
        to_string = "typewriter-reveal",
        serialize = "typewriterreveal",
        serialize = "typewriter"
    )]
    TypewriterReveal,
}

impl TransitionEffect {
    /// Number of defined scroll effects.
    pub const COUNT: usize = 21;

    /// All effects in definition order.
    pub const ALL: [TransitionEffect; Self::COUNT] = [
        Self::Slide,
        Self::Crossfade,
        Self::ScaleZoom,
        Self::FadeEdges,
        Self::Cascade,
        Self::Parallax,
        Self::Tilt,
        Self::PageCurl,
        Self::CardFlip,
        Self::CylinderRoll,
        Self::Wobbly,
        Self::Wave,
        Self::PerLineSpring,
        Self::Liquid,
        Self::MotionBlur,
        Self::ChromaticAberration,
        Self::GhostTrails,
        Self::ColorTemperature,
        Self::CRTScanlines,
        Self::DepthOfField,
        Self::TypewriterReveal,
    ];

    /// Parse from string (for Lisp integration).
    // Inherent infallible parser that defaults on an unknown name; deliberately
    // not `FromStr` (which would return `Result`), so the name collision is fine.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Self {
        s.to_lowercase()
            .replace('_', "-")
            .parse()
            .unwrap_or(Self::Slide)
    }

    /// Convert to kebab-case string.
    pub fn as_str(&self) -> &'static str {
        (*self).into()
    }

    /// Whether this effect needs a post-processing shader pipeline.
    pub fn needs_post_process(&self) -> bool {
        matches!(
            self,
            Self::MotionBlur
                | Self::ChromaticAberration
                | Self::GhostTrails
                | Self::ColorTemperature
                | Self::CRTScanlines
                | Self::DepthOfField
        )
    }

    /// Whether this effect needs tessellated (multi-strip) quads.
    pub fn needs_tessellation(&self) -> bool {
        matches!(
            self,
            Self::Wobbly
                | Self::Wave
                | Self::PerLineSpring
                | Self::Liquid
                | Self::Cascade
                | Self::CylinderRoll
                | Self::PageCurl
                | Self::TypewriterReveal
        )
    }

    /// Whether this effect uses 3D perspective projection.
    pub fn needs_3d(&self) -> bool {
        matches!(
            self,
            Self::Tilt | Self::PageCurl | Self::CardFlip | Self::CylinderRoll
        )
    }
}

// ─── Scroll Easing (how the animation parameter `t` evolves) ────────────

/// Physics model for scroll animation timing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, EnumString, IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum TransitionEasing {
    /// Standard ease-out quadratic (current default).
    #[strum(
        to_string = "ease-out-quad",
        serialize = "ease-out",
        serialize = "quad"
    )]
    #[default]
    EaseOutQuad,

    /// Ease-out cubic (stronger deceleration).
    #[strum(to_string = "ease-out-cubic", serialize = "cubic")]
    EaseOutCubic,

    /// Critically damped spring (Neovide-style, natural feel).
    #[strum(to_string = "spring", serialize = "damped")]
    Spring,

    /// Linear interpolation.
    Linear,

    /// Ease-in-out cubic (smooth S-curve).
    #[strum(to_string = "ease-in-out-cubic", serialize = "ease-in-out")]
    EaseInOutCubic,
}

impl TransitionEasing {
    /// Apply easing to a normalized time parameter t ∈ [0, 1].
    ///
    /// For non-spring easings this is a simple function.
    /// Spring easing requires a separate simulation (see [`SpringState`]).
    pub fn apply(&self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Self::EaseOutQuad => 1.0 - (1.0 - t).powi(2),
            Self::EaseOutCubic => 1.0 - (1.0 - t).powi(3),
            Self::Linear => t,
            Self::EaseInOutCubic => {
                if t < 0.5 {
                    4.0 * t * t * t
                } else {
                    1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
                }
            }
            Self::Spring => {
                // Analytical critically-damped spring approximation.
                // x(t) = 1 - (1 + ωt) * e^(-ωt)  where ω ≈ 8 for 150ms settle
                let omega = 8.0;
                let et = (-omega * t).exp();
                1.0 - (1.0 + omega * t) * et
            }
        }
    }

    // Inherent infallible parser that defaults on an unknown name; deliberately
    // not `FromStr` (which would return `Result`), so the name collision is fine.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Self {
        s.to_lowercase()
            .replace('_', "-")
            .parse()
            .unwrap_or(Self::EaseOutQuad)
    }

    pub fn as_str(&self) -> &'static str {
        (*self).into()
    }
}

// ─── Spring physics simulation ──────────────────────────────────────────

// ─── Per-line spring simulation for PerLineSpring effect ────────────────

// ─── Tessellation helpers ───────────────────────────────────────────────

// ─── Noise function for Liquid effect ───────────────────────────────────

// ─── Effect parameter computation ───────────────────────────────────────

// ─── Post-processing parameter computation ──────────────────────────────

// ─── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
#[path = "scroll_animation_test.rs"]
mod tests;
