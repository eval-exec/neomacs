//! Geometric deformations applied by transition shaders.
//!
//! These are pure functions of a normalized progress value: they describe how a
//! surface bends, not when. They live in the renderer because they are how a
//! transition is *drawn*; the protocol crate names the effect, and the
//! compositor decides whether it runs.
//!
//! Moved here from `neomacs-display-protocol::scroll_animation`, whose only
//! remaining job is the effect and easing vocabulary.

use std::f32::consts::PI;

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

#[cfg(test)]
#[path = "deform_test.rs"]
mod tests;
