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

use std::f32::consts::PI;
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

/// Critically damped spring state for smooth scroll physics.
///
/// Uses the analytical solution to the critically damped harmonic oscillator:
///   x(t) = target - (c1 + c2*t) * e^(-ω*t)
/// where ω = sqrt(stiffness/mass), c1 = x0 - target, c2 = v0 + ω*c1.
#[derive(Debug, Clone)]
pub struct SpringState {
    /// Current position (0.0 = start, 1.0 = target).
    pub position: f32,
    /// Current velocity.
    pub velocity: f32,
    /// Target position (usually 1.0).
    pub target: f32,
    /// Angular frequency ω = sqrt(k/m).
    pub omega: f32,
}

impl SpringState {
    pub fn new(omega: f32) -> Self {
        Self {
            position: 0.0,
            velocity: 0.0,
            target: 1.0,
            omega,
        }
    }

    /// Step the spring forward by `dt` seconds.
    /// Returns true if the spring has settled (position ≈ target).
    pub fn step(&mut self, dt: f32) -> bool {
        let x = self.position - self.target;
        let v = self.velocity;
        let w = self.omega;

        // Critically damped: ζ = 1
        // x(dt) = (c1 + c2*dt) * e^(-w*dt) + target
        // where c1 = x, c2 = v + w*x
        let exp = (-w * dt).exp();
        let c1 = x;
        let c2 = v + w * x;

        self.position = self.target + (c1 + c2 * dt) * exp;
        self.velocity = (c2 - w * (c1 + c2 * dt)) * exp;

        // Settled when close enough
        let settled = (self.position - self.target).abs() < 0.001 && self.velocity.abs() < 0.01;
        if settled {
            self.position = self.target;
            self.velocity = 0.0;
        }
        settled
    }
}

// ─── Per-line spring simulation for PerLineSpring effect ────────────────

/// State for per-line spring stagger animation.
#[derive(Debug, Clone)]
pub struct PerLineSpringState {
    /// Spring state for each visible line.
    pub springs: Vec<SpringState>,
    /// Stagger delay in seconds between consecutive lines.
    pub stagger_delay: f32,
    /// Total elapsed time since animation start.
    pub elapsed: f32,
}

impl PerLineSpringState {
    pub fn new(num_lines: usize, omega: f32, stagger_delay: f32) -> Self {
        Self {
            springs: (0..num_lines).map(|_| SpringState::new(omega)).collect(),
            stagger_delay,
            elapsed: 0.0,
        }
    }

    /// Step all springs. Each line starts its spring `stagger_delay` after the previous.
    /// Returns true when all springs have settled.
    pub fn step(&mut self, dt: f32) -> bool {
        self.elapsed += dt;
        let mut all_settled = true;

        for (i, spring) in self.springs.iter_mut().enumerate() {
            let line_start = i as f32 * self.stagger_delay;
            if self.elapsed > line_start {
                let line_dt = dt.min(self.elapsed - line_start);
                if !spring.step(line_dt) {
                    all_settled = false;
                }
            } else {
                all_settled = false;
            }
        }

        all_settled
    }

    /// Get the offset for a specific line (0.0 = start position, 1.0 = target).
    pub fn line_offset(&self, line: usize) -> f32 {
        if line < self.springs.len() {
            self.springs[line].position
        } else {
            0.0
        }
    }
}

// ─── Tessellation helpers ───────────────────────────────────────────────

/// Generate a tessellated quad as horizontal strips for deformation effects.
///
/// Returns vertices as (position, tex_coords) pairs for `num_strips` horizontal
/// strips spanning the given bounds. Each strip is 2 triangles (6 vertices).
///
/// The `deform` closure receives (strip_index, num_strips, normalized_y) and
/// returns (x_offset, y_offset) to apply to that strip's vertices.
pub fn tessellate_quad_strips(
    bounds_x: f32,
    bounds_y: f32,
    bounds_w: f32,
    bounds_h: f32,
    uv_left: f32,
    uv_top: f32,
    uv_right: f32,
    uv_bottom: f32,
    num_strips: usize,
    y_offset_base: f32,
    deform: impl Fn(usize, usize, f32) -> (f32, f32),
) -> Vec<[f32; 8]> {
    // Each vertex: [pos_x, pos_y, uv_x, uv_y, r, g, b, a]
    let mut vertices = Vec::with_capacity(num_strips * 6);
    let strip_h = bounds_h / num_strips as f32;
    let uv_strip_h = (uv_bottom - uv_top) / num_strips as f32;

    for i in 0..num_strips {
        let t0 = i as f32 / num_strips as f32;
        let t1 = (i + 1) as f32 / num_strips as f32;

        let (dx0, dy0) = deform(i, num_strips, t0);
        let (dx1, dy1) = deform(i + 1, num_strips, t1);

        let x0 = bounds_x + dx0;
        let x1 = bounds_x + bounds_w + dx0;
        let y0 = bounds_y + y_offset_base + i as f32 * strip_h + dy0;

        let x2 = bounds_x + dx1;
        let x3 = bounds_x + bounds_w + dx1;
        let y1 = bounds_y + y_offset_base + (i + 1) as f32 * strip_h + dy1;

        let uv_y0 = uv_top + i as f32 * uv_strip_h;
        let uv_y1 = uv_top + (i + 1) as f32 * uv_strip_h;

        // Triangle 1: top-left, top-right, bottom-right
        vertices.push([x0, y0, uv_left, uv_y0, 1.0, 1.0, 1.0, 1.0]);
        vertices.push([x1, y0, uv_right, uv_y0, 1.0, 1.0, 1.0, 1.0]);
        vertices.push([x3, y1, uv_right, uv_y1, 1.0, 1.0, 1.0, 1.0]);

        // Triangle 2: top-left, bottom-right, bottom-left
        vertices.push([x0, y0, uv_left, uv_y0, 1.0, 1.0, 1.0, 1.0]);
        vertices.push([x3, y1, uv_right, uv_y1, 1.0, 1.0, 1.0, 1.0]);
        vertices.push([x2, y1, uv_left, uv_y1, 1.0, 1.0, 1.0, 1.0]);
    }

    vertices
}

/// Generate vertices for a simple (non-tessellated) quad with alpha.
pub fn make_quad_vertices(
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    uv_left: f32,
    uv_top: f32,
    uv_right: f32,
    uv_bottom: f32,
    alpha: f32,
) -> [[f32; 8]; 6] {
    [
        [x0, y0, uv_left, uv_top, 1.0, 1.0, 1.0, alpha],
        [x1, y0, uv_right, uv_top, 1.0, 1.0, 1.0, alpha],
        [x1, y1, uv_right, uv_bottom, 1.0, 1.0, 1.0, alpha],
        [x0, y0, uv_left, uv_top, 1.0, 1.0, 1.0, alpha],
        [x1, y1, uv_right, uv_bottom, 1.0, 1.0, 1.0, alpha],
        [x0, y1, uv_left, uv_bottom, 1.0, 1.0, 1.0, alpha],
    ]
}

// ─── Noise function for Liquid effect ───────────────────────────────────

/// Simple 2D hash-based noise (deterministic, no external dependency).
pub fn noise2d(x: f32, y: f32) -> f32 {
    let n = (x * 12.9898 + y * 78.233).sin() * 43_758.547;
    n.fract()
}

/// Smooth noise with bilinear interpolation.
pub fn smooth_noise2d(x: f32, y: f32) -> f32 {
    let ix = x.floor() as i32;
    let iy = y.floor() as i32;
    let fx = x.fract();
    let fy = y.fract();

    // Smoothstep
    let sx = fx * fx * (3.0 - 2.0 * fx);
    let sy = fy * fy * (3.0 - 2.0 * fy);

    let n00 = noise2d(ix as f32, iy as f32);
    let n10 = noise2d((ix + 1) as f32, iy as f32);
    let n01 = noise2d(ix as f32, (iy + 1) as f32);
    let n11 = noise2d((ix + 1) as f32, (iy + 1) as f32);

    let nx0 = n00 + sx * (n10 - n00);
    let nx1 = n01 + sx * (n11 - n01);

    nx0 + sy * (nx1 - nx0)
}

// ─── Effect parameter computation ───────────────────────────────────────

/// Compute parameters for the Wobbly/jelly effect.
///
/// Returns (x_offset, y_offset) for a given strip at normalized position `t`.
/// `eased_t` is the overall animation progress, `direction` is ±1.
pub fn wobbly_deform(
    _strip: usize,
    _num_strips: usize,
    t: f32,
    eased_t: f32,
    direction: f32,
    amplitude: f32,
) -> (f32, f32) {
    // Top moves first, bottom lags (or reverse for scroll-up)
    let strip_t = if direction > 0.0 { t } else { 1.0 - t };
    // Phase offset creates wave propagation
    let phase = strip_t * PI * 2.0 - eased_t * PI * 4.0;
    let damping = 1.0 - eased_t; // Dampen as animation progresses
    let x_offset = amplitude * phase.sin() * damping * (1.0 - strip_t);
    (x_offset, 0.0)
}

/// Compute parameters for the Wave/sine effect.
pub fn wave_deform(
    _strip: usize,
    _num_strips: usize,
    t: f32,
    eased_t: f32,
    elapsed_secs: f32,
    amplitude: f32,
    frequency: f32,
) -> (f32, f32) {
    let damping = 1.0 - eased_t;
    let phase = t * frequency * PI * 2.0 + elapsed_secs * 8.0;
    let x_offset = amplitude * phase.sin() * damping;
    (x_offset, 0.0)
}

/// Compute parameters for the Liquid/fluid effect.
pub fn liquid_deform(
    _strip: usize,
    _num_strips: usize,
    t: f32,
    eased_t: f32,
    elapsed_secs: f32,
    amplitude: f32,
) -> (f32, f32) {
    let damping = 1.0 - eased_t;
    let nx = smooth_noise2d(t * 4.0 + elapsed_secs * 2.0, elapsed_secs * 1.5);
    let ny = smooth_noise2d(t * 3.0 + elapsed_secs * 1.8, elapsed_secs * 2.2 + 100.0);
    let x_offset = (nx - 0.5) * amplitude * 2.0 * damping;
    let y_offset = (ny - 0.5) * amplitude * damping;
    (x_offset, y_offset)
}

/// Compute parameters for the 3D tilt effect.
///
/// Returns the Y-offset for a strip at normalized position `t`,
/// simulating a perspective tilt around the X-axis.
/// `velocity_factor` is scroll_direction * (1 - eased_t) to create
/// tilt that decays as animation settles.
pub fn tilt_y_offset(t: f32, velocity_factor: f32, max_tilt_pixels: f32) -> f32 {
    // Parabolic tilt: center stays put, edges deflect
    // y_offset = max_tilt * velocity * (t - 0.5)
    let centered = t - 0.5;
    max_tilt_pixels * velocity_factor * centered * 2.0
}

/// Compute parameters for the CylinderRoll effect.
///
/// Returns (x_offset, y_offset, scale) for a strip at normalized position `t`,
/// simulating content wrapped around a vertical cylinder.
pub fn cylinder_roll_transform(
    t: f32,
    eased_t: f32,
    direction: f32,
    bounds_w: f32,
) -> (f32, f32, f32) {
    // Map t to angle on cylinder surface
    let angle = (t - 0.5) * PI * 0.3; // Subtle curvature
    let rotation = direction * (1.0 - eased_t) * PI * 0.15;
    let total_angle = angle + rotation;

    let scale = total_angle.cos().abs().max(0.3);
    let y_offset = total_angle.sin() * bounds_w * 0.1;
    let x_offset = 0.0;

    (x_offset, y_offset, scale)
}

/// Compute page curl deformation for a strip.
///
/// Returns (x_offset, y_offset, alpha) where alpha handles the
/// backside darkening of the curled page.
pub fn page_curl_transform(t: f32, curl_progress: f32, bounds_h: f32) -> (f32, f32, f32) {
    // The curl line moves from bottom to top as progress increases
    let curl_y = 1.0 - curl_progress;

    if t > curl_y {
        // Below curl line: this part is curling away
        let curl_t = (t - curl_y) / (1.0 - curl_y).max(0.001);
        let curl_angle = curl_t * PI;

        // Cylinder deformation
        let radius = bounds_h * 0.15;
        let y_offset = -radius * curl_angle.sin();
        let x_offset = radius * (1.0 - curl_angle.cos()) * 0.5;
        let alpha = (1.0 - curl_t * 0.6).max(0.2); // Darken backside

        (x_offset, y_offset, alpha)
    } else {
        // Above curl line: flat, no deformation
        (0.0, 0.0, 1.0)
    }
}

/// Compute card flip rotation parameters.
///
/// Returns (scale_x, alpha) for simulating a 3D card flip.
/// The card rotates around the X-axis: shrinks to 0 width at midpoint,
/// then expands showing the new side.
pub fn card_flip_transform(t: f32) -> (f32, f32) {
    let angle = t * PI;
    let scale_y = angle.cos().abs().max(0.02); // Perspective scaling
    let alpha = if t < 0.5 { 1.0 } else { 0.0 }; // Show old first half, new second
    (scale_y, alpha)
}

// ─── Post-processing parameter computation ──────────────────────────────

/// Parameters for post-processing shader effects.
#[derive(Debug, Clone, Copy, Default)]
pub struct PostProcessParams {
    /// Scroll velocity in pixels/second (signed, positive = down).
    pub scroll_velocity: f32,
    /// Normalized scroll speed (0.0 = still, 1.0 = fast).
    pub scroll_speed: f32,
    /// Scroll direction: +1.0 = down, -1.0 = up, 0.0 = still.
    pub scroll_direction: f32,
    /// Current scroll position in pixels (for CRT scanline phase).
    pub scroll_position: f32,
    /// Elapsed time in seconds (for animated effects).
    pub time: f32,
}

impl PostProcessParams {
    /// Motion blur sample offset in pixels, proportional to velocity.
    pub fn motion_blur_offset(&self) -> f32 {
        (self.scroll_speed * 8.0).min(12.0)
    }

    /// Chromatic aberration offset in pixels.
    pub fn chromatic_offset(&self) -> f32 {
        (self.scroll_speed * 3.0).min(5.0)
    }

    /// Ghost trail opacity (0 = no trail, higher = more visible).
    pub fn ghost_opacity(&self) -> f32 {
        (self.scroll_speed * 0.3).min(0.25)
    }

    /// Color temperature shift (-1 = cool/blue, +1 = warm/orange).
    pub fn color_temp_shift(&self) -> f32 {
        self.scroll_direction * self.scroll_speed * 0.04
    }

    /// CRT scanline phase offset.
    pub fn scanline_phase(&self) -> f32 {
        self.scroll_position * 0.1
    }

    /// Depth-of-field blur radius at edges.
    pub fn dof_blur_radius(&self) -> f32 {
        (self.scroll_speed * 4.0).min(6.0)
    }
}

// ─── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
#[path = "scroll_animation_test.rs"]
mod tests;
