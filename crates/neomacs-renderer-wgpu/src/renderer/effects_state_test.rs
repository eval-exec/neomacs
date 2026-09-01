use super::*;
use neomacs_display_protocol::types::{Color, Rect};

// ────────────────────────────────────────────────────────────────────
// Helper: float comparison with tolerance
// ────────────────────────────────────────────────────────────────────

fn approx_eq(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-5
}

fn assert_approx(a: f32, b: f32, context: &str) {
    assert!(
        approx_eq(a, b),
        "{}: expected {}, got {} (diff {})",
        context,
        b,
        a,
        (a - b).abs()
    );
}

// ────────────────────────────────────────────────────────────────────
// 1. scale_rect: pure static function
// ────────────────────────────────────────────────────────────────────

#[test]
fn scale_rect_identity() {
    // scale=1.0 should return the same rectangle
    let (nx, ny, nw, nh) = WgpuRenderer::scale_rect(10.0, 20.0, 100.0, 50.0, 1.0);
    assert_approx(nx, 10.0, "x");
    assert_approx(ny, 20.0, "y");
    assert_approx(nw, 100.0, "w");
    assert_approx(nh, 50.0, "h");
}

#[test]
fn scale_rect_double() {
    // Doubling a rect centered at (60, 45) with size (100, 50)
    // center = (10+50, 20+25) = (60, 45)
    // new size = (200, 100)
    // new origin = (60-100, 45-50) = (-40, -5)
    let (nx, ny, nw, nh) = WgpuRenderer::scale_rect(10.0, 20.0, 100.0, 50.0, 2.0);
    assert_approx(nx, -40.0, "x doubled");
    assert_approx(ny, -5.0, "y doubled");
    assert_approx(nw, 200.0, "w doubled");
    assert_approx(nh, 100.0, "h doubled");
}

#[test]
fn scale_rect_half() {
    // Halving a rect: center stays at (60, 45)
    // new size = (50, 25)
    // new origin = (60-25, 45-12.5) = (35, 32.5)
    let (nx, ny, nw, nh) = WgpuRenderer::scale_rect(10.0, 20.0, 100.0, 50.0, 0.5);
    assert_approx(nx, 35.0, "x halved");
    assert_approx(ny, 32.5, "y halved");
    assert_approx(nw, 50.0, "w halved");
    assert_approx(nh, 25.0, "h halved");
}

#[test]
fn scale_rect_zero() {
    // scale=0 collapses to the center point
    let (nx, ny, nw, nh) = WgpuRenderer::scale_rect(10.0, 20.0, 100.0, 50.0, 0.0);
    assert_approx(nx, 60.0, "x at center");
    assert_approx(ny, 45.0, "y at center");
    assert_approx(nw, 0.0, "w zero");
    assert_approx(nh, 0.0, "h zero");
}

#[test]
fn scale_rect_origin_zero() {
    // Rectangle at origin: center = (50, 25)
    let (nx, ny, nw, nh) = WgpuRenderer::scale_rect(0.0, 0.0, 100.0, 50.0, 1.5);
    // new size = (150, 75), new origin = (50-75, 25-37.5) = (-25, -12.5)
    assert_approx(nx, -25.0, "x");
    assert_approx(ny, -12.5, "y");
    assert_approx(nw, 150.0, "w");
    assert_approx(nh, 75.0, "h");
}

// ────────────────────────────────────────────────────────────────────
// 2. hsl_to_color: pure static function
// ────────────────────────────────────────────────────────────────────

#[test]
fn hsl_to_color_red() {
    // Pure red: h=0, s=1, l=0.5
    let c = WgpuRenderer::hsl_to_color(0.0, 1.0, 0.5);
    assert_approx(c.r, 1.0, "red r");
    assert_approx(c.g, 0.0, "red g");
    assert_approx(c.b, 0.0, "red b");
    assert_approx(c.a, 1.0, "red a");
}

#[test]
fn hsl_to_color_green() {
    // Pure green: h=1/3, s=1, l=0.5
    let c = WgpuRenderer::hsl_to_color(1.0 / 3.0, 1.0, 0.5);
    assert_approx(c.r, 0.0, "green r");
    assert_approx(c.g, 1.0, "green g");
    assert_approx(c.b, 0.0, "green b");
}

#[test]
fn hsl_to_color_blue() {
    // Pure blue: h=2/3, s=1, l=0.5
    let c = WgpuRenderer::hsl_to_color(2.0 / 3.0, 1.0, 0.5);
    assert_approx(c.r, 0.0, "blue r");
    assert_approx(c.g, 0.0, "blue g");
    assert_approx(c.b, 1.0, "blue b");
}

#[test]
fn hsl_to_color_white() {
    // White: any hue, s=0, l=1
    let c = WgpuRenderer::hsl_to_color(0.0, 0.0, 1.0);
    assert_approx(c.r, 1.0, "white r");
    assert_approx(c.g, 1.0, "white g");
    assert_approx(c.b, 1.0, "white b");
}

#[test]
fn hsl_to_color_black() {
    // Black: any hue, s=0, l=0
    let c = WgpuRenderer::hsl_to_color(0.0, 0.0, 0.0);
    assert_approx(c.r, 0.0, "black r");
    assert_approx(c.g, 0.0, "black g");
    assert_approx(c.b, 0.0, "black b");
}

#[test]
fn hsl_to_color_gray() {
    // 50% gray: s=0, l=0.5
    let c = WgpuRenderer::hsl_to_color(0.0, 0.0, 0.5);
    assert_approx(c.r, 0.5, "gray r");
    assert_approx(c.g, 0.5, "gray g");
    assert_approx(c.b, 0.5, "gray b");
}

#[test]
fn hsl_to_color_yellow() {
    // Yellow: h=1/6, s=1, l=0.5
    let c = WgpuRenderer::hsl_to_color(1.0 / 6.0, 1.0, 0.5);
    assert_approx(c.r, 1.0, "yellow r");
    assert_approx(c.g, 1.0, "yellow g");
    assert_approx(c.b, 0.0, "yellow b");
}

#[test]
fn hsl_to_color_cyan() {
    // Cyan: h=0.5, s=1, l=0.5
    let c = WgpuRenderer::hsl_to_color(0.5, 1.0, 0.5);
    assert_approx(c.r, 0.0, "cyan r");
    assert_approx(c.g, 1.0, "cyan g");
    assert_approx(c.b, 1.0, "cyan b");
}

#[test]
fn hsl_to_color_always_opaque() {
    // All HSL conversions should produce alpha=1.0
    for h in [0.0, 0.1, 0.25, 0.5, 0.75, 0.9] {
        let c = WgpuRenderer::hsl_to_color(h, 0.6, 0.4);
        assert_approx(c.a, 1.0, &format!("alpha at h={}", h));
    }
}

#[test]
fn hsl_to_color_desaturated() {
    // Low saturation should produce near-gray at any hue
    let c = WgpuRenderer::hsl_to_color(0.3, 0.0, 0.5);
    assert_approx(c.r, 0.5, "desat r");
    assert_approx(c.g, 0.5, "desat g");
    assert_approx(c.b, 0.5, "desat b");
}

// ────────────────────────────────────────────────────────────────────
// 3. Easing function math (extracted from method patterns)
// ────────────────────────────────────────────────────────────────────

/// Quadratic ease-out: t * (2 - t)
/// Used by: line_y_offset, cursor_wake_factor, resize_padding_amount
fn ease_out_quad(t: f32) -> f32 {
    t * (2.0 - t)
}

/// Quadratic ease-in: t * t
/// Used by: text_fade_alpha
fn ease_in_quad(t: f32) -> f32 {
    t * t
}

/// Exponential decay (squared): (1-t)^2
/// Used by: cursor_error_pulse_override (alpha), scroll line spacing decay
fn decay_squared(t: f32) -> f32 {
    (1.0 - t) * (1.0 - t)
}

#[test]
fn ease_out_quad_boundary_values() {
    // At t=0, ease=0; at t=1, ease=1
    assert_approx(ease_out_quad(0.0), 0.0, "ease-out at t=0");
    assert_approx(ease_out_quad(1.0), 1.0, "ease-out at t=1");
}

#[test]
fn ease_out_quad_midpoint() {
    // At t=0.5: 0.5 * (2 - 0.5) = 0.5 * 1.5 = 0.75
    assert_approx(ease_out_quad(0.5), 0.75, "ease-out at t=0.5");
}

#[test]
fn ease_out_quad_is_monotonically_increasing() {
    let mut prev = ease_out_quad(0.0);
    for i in 1..=100 {
        let t = i as f32 / 100.0;
        let val = ease_out_quad(t);
        assert!(
            val >= prev,
            "ease-out should be monotonically increasing at t={}",
            t
        );
        prev = val;
    }
}

#[test]
fn ease_out_quad_starts_fast() {
    // Ease-out should cover more than half the range in the first half of time
    let at_half = ease_out_quad(0.5);
    assert!(
        at_half > 0.5,
        "ease-out at t=0.5 should be > 0.5, got {}",
        at_half
    );
}

#[test]
fn ease_in_quad_boundary_values() {
    // At t=0, ease=0; at t=1, ease=1
    assert_approx(ease_in_quad(0.0), 0.0, "ease-in at t=0");
    assert_approx(ease_in_quad(1.0), 1.0, "ease-in at t=1");
}

#[test]
fn ease_in_quad_midpoint() {
    // At t=0.5: 0.5 * 0.5 = 0.25
    assert_approx(ease_in_quad(0.5), 0.25, "ease-in at t=0.5");
}

#[test]
fn ease_in_quad_starts_slow() {
    // Ease-in should cover less than half the range in the first half of time
    let at_half = ease_in_quad(0.5);
    assert!(
        at_half < 0.5,
        "ease-in at t=0.5 should be < 0.5, got {}",
        at_half
    );
}

#[test]
fn ease_in_quad_is_monotonically_increasing() {
    let mut prev = ease_in_quad(0.0);
    for i in 1..=100 {
        let t = i as f32 / 100.0;
        let val = ease_in_quad(t);
        assert!(
            val >= prev,
            "ease-in should be monotonically increasing at t={}",
            t
        );
        prev = val;
    }
}

#[test]
fn decay_squared_boundary_values() {
    // At t=0, alpha=1; at t=1, alpha=0
    assert_approx(decay_squared(0.0), 1.0, "decay at t=0");
    assert_approx(decay_squared(1.0), 0.0, "decay at t=1");
}

#[test]
fn decay_squared_midpoint() {
    // At t=0.5: (1-0.5)^2 = 0.25
    assert_approx(decay_squared(0.5), 0.25, "decay at t=0.5");
}

#[test]
fn decay_squared_is_monotonically_decreasing() {
    let mut prev = decay_squared(0.0);
    for i in 1..=100 {
        let t = i as f32 / 100.0;
        let val = decay_squared(t);
        assert!(
            val <= prev,
            "decay should be monotonically decreasing at t={}",
            t
        );
        prev = val;
    }
}

// ────────────────────────────────────────────────────────────────────
// 4. Time-based progress computation
// ────────────────────────────────────────────────────────────────────

/// Simulates the progress calculation used throughout: t = (elapsed / duration).min(1.0)
fn time_progress(elapsed_ms: f32, duration_ms: f32) -> f32 {
    (elapsed_ms / duration_ms).min(1.0)
}

#[test]
fn time_progress_at_start() {
    assert_approx(time_progress(0.0, 200.0), 0.0, "progress at start");
}

#[test]
fn time_progress_at_halfway() {
    assert_approx(time_progress(100.0, 200.0), 0.5, "progress at halfway");
}

#[test]
fn time_progress_at_end() {
    assert_approx(time_progress(200.0, 200.0), 1.0, "progress at end");
}

#[test]
fn time_progress_clamped_past_end() {
    // After duration, progress should be clamped to 1.0
    assert_approx(time_progress(500.0, 200.0), 1.0, "progress clamped");
}

#[test]
fn time_progress_very_small_elapsed() {
    let t = time_progress(1.0, 1000.0);
    assert_approx(t, 0.001, "small elapsed");
}

// ────────────────────────────────────────────────────────────────────
// 5. Cursor wake scale factor computation
// ────────────────────────────────────────────────────────────────────

/// Simulates cursor_wake_factor logic for a given t (0..1), scale, and enabled state
fn cursor_wake_factor_at(t: f32, scale: f32, enabled: bool) -> f32 {
    if !enabled {
        return 1.0;
    }
    if t >= 1.0 {
        return 1.0;
    }
    let ease = t * (2.0 - t);
    1.0 + (scale - 1.0) * (1.0 - ease)
}

#[test]
fn cursor_wake_disabled_returns_one() {
    assert_approx(cursor_wake_factor_at(0.0, 1.5, false), 1.0, "disabled");
    assert_approx(cursor_wake_factor_at(0.5, 2.0, false), 1.0, "disabled mid");
}

#[test]
fn cursor_wake_at_start_returns_scale() {
    // At t=0, factor should equal the configured scale
    assert_approx(cursor_wake_factor_at(0.0, 1.3, true), 1.3, "wake at start");
    assert_approx(
        cursor_wake_factor_at(0.0, 2.0, true),
        2.0,
        "wake at start 2x",
    );
}

#[test]
fn cursor_wake_at_end_returns_one() {
    assert_approx(cursor_wake_factor_at(1.0, 1.3, true), 1.0, "wake at end");
}

#[test]
fn cursor_wake_midway_between_scale_and_one() {
    let factor = cursor_wake_factor_at(0.5, 1.3, true);
    // At t=0.5, ease=0.75, so factor = 1.0 + 0.3 * 0.25 = 1.075
    assert_approx(factor, 1.075, "wake at mid");
}

#[test]
fn cursor_wake_settles_monotonically_toward_one() {
    let scale = 1.5;
    let mut prev = cursor_wake_factor_at(0.0, scale, true);
    for i in 1..=100 {
        let t = i as f32 / 100.0;
        let val = cursor_wake_factor_at(t, scale, true);
        assert!(
            val <= prev + 1e-6,
            "wake factor should decrease toward 1.0 at t={}: {} > {}",
            t,
            val,
            prev
        );
        prev = val;
    }
}

// ────────────────────────────────────────────────────────────────────
// 6. Resize padding amount computation
// ────────────────────────────────────────────────────────────────────

/// Simulates resize_padding_amount: max * (1 - ease_out_quad(t))
fn resize_padding_at(t: f32, max: f32) -> f32 {
    if t >= 1.0 {
        return 0.0;
    }
    let ease = t * (2.0 - t);
    max * (1.0 - ease)
}

#[test]
fn resize_padding_at_start_equals_max() {
    assert_approx(resize_padding_at(0.0, 12.0), 12.0, "padding at start");
}

#[test]
fn resize_padding_at_end_equals_zero() {
    assert_approx(resize_padding_at(1.0, 12.0), 0.0, "padding at end");
}

#[test]
fn resize_padding_decreases_over_time() {
    let max = 12.0;
    let mut prev = resize_padding_at(0.0, max);
    for i in 1..=100 {
        let t = i as f32 / 100.0;
        let val = resize_padding_at(t, max);
        assert!(
            val <= prev + 1e-6,
            "padding should decrease at t={}: {} > {}",
            t,
            val,
            prev
        );
        prev = val;
    }
}

// ────────────────────────────────────────────────────────────────────
// 7. Cursor error pulse alpha computation
// ────────────────────────────────────────────────────────────────────

/// Simulates the error pulse alpha: (1-t)^2
fn error_pulse_alpha_at(t: f32) -> f32 {
    if t >= 1.0 {
        return 0.0;
    }
    (1.0 - t) * (1.0 - t)
}

#[test]
fn error_pulse_alpha_at_start_is_full() {
    assert_approx(error_pulse_alpha_at(0.0), 1.0, "pulse at start");
}

#[test]
fn error_pulse_alpha_at_end_is_zero() {
    assert_approx(error_pulse_alpha_at(1.0), 0.0, "pulse at end");
}

#[test]
fn error_pulse_alpha_fades_out() {
    let mut prev = error_pulse_alpha_at(0.0);
    for i in 1..=100 {
        let t = i as f32 / 100.0;
        let val = error_pulse_alpha_at(t);
        assert!(
            val <= prev + 1e-6,
            "pulse alpha should decrease at t={}: {} > {}",
            t,
            val,
            prev
        );
        prev = val;
    }
}

#[test]
fn error_pulse_color_construction() {
    // Verifies the Color::new pattern used in cursor_error_pulse_override
    let (r, g, b) = (1.0_f32, 0.2_f32, 0.2_f32);
    let t = 0.5_f32;
    let alpha = (1.0 - t) * (1.0 - t);
    let color = Color::new(r, g, b, alpha);
    assert_approx(color.r, 1.0, "pulse color r");
    assert_approx(color.g, 0.2, "pulse color g");
    assert_approx(color.b, 0.2, "pulse color b");
    assert_approx(color.a, 0.25, "pulse color a");
}

// ────────────────────────────────────────────────────────────────────
// 8. Line animation offset computation
// ────────────────────────────────────────────────────────────────────

/// Simulates line_y_offset for a single animation: initial_offset * (1 - ease_out(t))
fn line_anim_offset_at(t: f32, initial_offset: f32) -> f32 {
    let t = t.min(1.0);
    let eased = t * (2.0 - t);
    initial_offset * (1.0 - eased)
}

#[test]
fn line_anim_at_start_returns_full_offset() {
    assert_approx(line_anim_offset_at(0.0, 20.0), 20.0, "line anim at start");
}

#[test]
fn line_anim_at_end_returns_zero() {
    assert_approx(line_anim_offset_at(1.0, 20.0), 0.0, "line anim at end");
}

#[test]
fn line_anim_negative_offset() {
    // Insertion animations use negative offset
    assert_approx(
        line_anim_offset_at(0.0, -15.0),
        -15.0,
        "neg offset at start",
    );
    assert_approx(line_anim_offset_at(1.0, -15.0), 0.0, "neg offset at end");
}

#[test]
fn line_anim_midway() {
    // At t=0.5, eased=0.75, offset=20*(1-0.75)=5
    assert_approx(line_anim_offset_at(0.5, 20.0), 5.0, "line anim at mid");
}

// ────────────────────────────────────────────────────────────────────
// 9. Scroll line spacing decay computation
// ────────────────────────────────────────────────────────────────────

/// Simulates the scroll line spacing accordion effect edge factor
fn scroll_spacing_edge_factor(norm: f32, direction: i32) -> f32 {
    let norm = norm.clamp(0.0, 1.0);
    if direction > 0 { 1.0 - norm } else { norm }
}

#[test]
fn scroll_spacing_direction_down_at_top() {
    // Scroll down (direction > 0), at top of window (norm=0) => full effect
    assert_approx(scroll_spacing_edge_factor(0.0, 1), 1.0, "down at top");
}

#[test]
fn scroll_spacing_direction_down_at_bottom() {
    // Scroll down, at bottom of window (norm=1) => no effect
    assert_approx(scroll_spacing_edge_factor(1.0, 1), 0.0, "down at bottom");
}

#[test]
fn scroll_spacing_direction_up_at_top() {
    // Scroll up (direction < 0), at top of window => no effect
    assert_approx(scroll_spacing_edge_factor(0.0, -1), 0.0, "up at top");
}

#[test]
fn scroll_spacing_direction_up_at_bottom() {
    // Scroll up, at bottom of window => full effect
    assert_approx(scroll_spacing_edge_factor(1.0, -1), 1.0, "up at bottom");
}

#[test]
fn scroll_spacing_norm_clamped() {
    // Values outside [0, 1] should be clamped
    assert_approx(scroll_spacing_edge_factor(-0.5, 1), 1.0, "clamped negative");
    assert_approx(scroll_spacing_edge_factor(1.5, 1), 0.0, "clamped above 1");
}

// ────────────────────────────────────────────────────────────────────
// 10. Text fade-in alpha computation
// ────────────────────────────────────────────────────────────────────

/// Simulates text_fade_alpha: quadratic ease-in (t^2)
fn text_fade_alpha_at(t: f32) -> f32 {
    if t >= 1.0 {
        return 1.0;
    }
    t * t
}

#[test]
fn text_fade_at_start_is_transparent() {
    assert_approx(text_fade_alpha_at(0.0), 0.0, "text fade at start");
}

#[test]
fn text_fade_at_end_is_opaque() {
    assert_approx(text_fade_alpha_at(1.0), 1.0, "text fade at end");
}

#[test]
fn text_fade_quarter() {
    // At t=0.25: 0.0625
    assert_approx(text_fade_alpha_at(0.25), 0.0625, "text fade at 0.25");
}

// ────────────────────────────────────────────────────────────────────
// 11. Mode-line fade alpha (linear)
// ────────────────────────────────────────────────────────────────────

/// Simulates mode_line_fade_alpha: linear t
fn mode_line_fade_at(t: f32) -> f32 {
    if t >= 1.0 {
        return 1.0;
    }
    t
}

#[test]
fn mode_line_fade_linear() {
    assert_approx(mode_line_fade_at(0.0), 0.0, "mode line at start");
    assert_approx(mode_line_fade_at(0.5), 0.5, "mode line at mid");
    assert_approx(mode_line_fade_at(1.0), 1.0, "mode line at end");
}

// ────────────────────────────────────────────────────────────────────
// 12. Effect config defaults
// ────────────────────────────────────────────────────────────────────

#[test]
fn default_effects_config() {
    let cfg = crate::effect_config::EffectsConfig::default();

    // Inactive dim
    assert!(!cfg.inactive_dim.enabled);
    assert_approx(cfg.inactive_dim.opacity, 0.15, "inactive dim opacity");

    // Cursor wake
    assert!(!cfg.cursor_wake.enabled);
    assert_eq!(cfg.cursor_wake.duration_ms, 120);
    assert_approx(cfg.cursor_wake.scale, 1.3, "cursor wake scale");

    // Edge snap
    assert!(!cfg.edge_snap.enabled);
    assert_eq!(cfg.edge_snap.duration_ms, 200);

    // Typing heatmap
    assert!(!cfg.typing_heatmap.enabled);
    assert_eq!(cfg.typing_heatmap.fade_ms, 2000);
    assert_approx(cfg.typing_heatmap.opacity, 0.15, "heatmap opacity");

    // Click halo
    assert!(!cfg.click_halo.enabled);
    assert_eq!(cfg.click_halo.duration_ms, 300);
    assert_approx(cfg.click_halo.max_radius, 30.0, "halo radius");

    // Scroll velocity fade
    assert!(!cfg.scroll_velocity_fade.enabled);
    assert_eq!(cfg.scroll_velocity_fade.ms, 300);

    // Resize padding
    assert!(!cfg.resize_padding.enabled);
    assert_eq!(cfg.resize_padding.duration_ms, 200);
    assert_approx(cfg.resize_padding.max, 12.0, "resize max");

    // Cursor error pulse
    assert!(!cfg.cursor_error_pulse.enabled);
    assert_eq!(cfg.cursor_error_pulse.duration_ms, 250);

    // Scroll momentum
    assert!(!cfg.scroll_momentum.enabled);
    assert_eq!(cfg.scroll_momentum.fade_ms, 300);

    // Matrix rain
    assert!(!cfg.matrix_rain.enabled);
    assert_eq!(cfg.matrix_rain.column_count, 40);
    assert_approx(cfg.matrix_rain.speed, 150.0, "matrix speed");

    // Frost border
    assert!(!cfg.frost_border.enabled);
    assert_approx(cfg.frost_border.width, 6.0, "frost width");

    // Edge glow
    assert!(!cfg.edge_glow.enabled);
    assert_eq!(cfg.edge_glow.fade_ms, 400);

    // Sonar ping
    assert!(!cfg.cursor_sonar_ping.enabled);
    assert_eq!(cfg.cursor_sonar_ping.duration_ms, 600);

    // Mode-line transition
    assert!(!cfg.mode_line_transition.enabled);
    assert_eq!(cfg.mode_line_transition.duration_ms, 200);

    // Text fade-in
    assert!(!cfg.text_fade_in.enabled);
    assert_eq!(cfg.text_fade_in.duration_ms, 150);

    // Scroll line spacing
    assert!(!cfg.scroll_line_spacing.enabled);
    assert_approx(cfg.scroll_line_spacing.max, 6.0, "scroll spacing max");

    // Cursor trail fade
    assert!(!cfg.cursor_trail_fade.enabled);
    assert_eq!(cfg.cursor_trail_fade.length, 8);

    // Window switch fade
    assert!(!cfg.window_switch_fade.enabled);
    assert_eq!(cfg.window_switch_fade.duration_ms, 200);
    assert_approx(cfg.window_switch_fade.intensity, 0.15, "switch intensity");

    // Show whitespace
    assert!(!cfg.show_whitespace.enabled);

    // Line highlight
    assert!(!cfg.line_highlight.enabled);

    // Indent guides
    assert!(!cfg.indent_guides.enabled);
    assert!(!cfg.indent_guides.rainbow_enabled);
    assert!(cfg.indent_guides.rainbow_colors.is_empty());

    // Typing ripple
    assert!(!cfg.typing_ripple.enabled);
}

// ────────────────────────────────────────────────────────────────────
// 13. Entry struct creation and fields
// ────────────────────────────────────────────────────────────────────

#[test]
fn line_anim_entry_fields() {
    let bounds = Rect::new(10.0, 20.0, 400.0, 300.0);
    let now = std::time::Instant::now();
    let entry = LineAnimEntry {
        window_bounds: bounds,
        edit_y: 100.0,
        initial_offset: -15.0,
        started: now,
        duration: std::time::Duration::from_millis(200),
    };
    assert_approx(entry.window_bounds.x, 10.0, "bounds x");
    assert_approx(entry.edit_y, 100.0, "edit_y");
    assert_approx(entry.initial_offset, -15.0, "offset");
    assert_eq!(entry.duration, std::time::Duration::from_millis(200));
}

#[test]
fn click_halo_entry_fields() {
    let now = std::time::Instant::now();
    let entry = ClickHaloEntry {
        x: 150.0,
        y: 200.0,
        started: now,
        duration: std::time::Duration::from_millis(300),
    };
    assert_approx(entry.x, 150.0, "halo x");
    assert_approx(entry.y, 200.0, "halo y");
    assert_eq!(entry.duration, std::time::Duration::from_millis(300));
}

#[test]
fn scroll_spacing_entry_fields() {
    let bounds = Rect::new(0.0, 0.0, 800.0, 600.0);
    let now = std::time::Instant::now();
    let entry = ScrollSpacingEntry {
        window_id: 42,
        bounds,
        direction: -1,
        started: now,
        duration: std::time::Duration::from_millis(200),
    };
    assert_eq!(entry.window_id, 42);
    assert_eq!(entry.direction, -1);
    assert_approx(entry.bounds.width, 800.0, "bounds width");
}

#[test]
fn text_fade_entry_fields() {
    let bounds = Rect::new(5.0, 10.0, 500.0, 400.0);
    let now = std::time::Instant::now();
    let entry = TextFadeEntry {
        window_id: 7,
        bounds,
        started: now,
        duration: std::time::Duration::from_millis(150),
    };
    assert_eq!(entry.window_id, 7);
    assert_approx(entry.bounds.x, 5.0, "fade bounds x");
    assert_eq!(entry.duration, std::time::Duration::from_millis(150));
}

#[test]
fn edge_snap_entry_fields() {
    let bounds = Rect::new(0.0, 0.0, 800.0, 600.0);
    let now = std::time::Instant::now();
    let entry = EdgeSnapEntry {
        bounds,
        mode_line_height: 22.0,
        at_top: true,
        at_bottom: false,
        started: now,
        duration: std::time::Duration::from_millis(200),
    };
    assert!(entry.at_top);
    assert!(!entry.at_bottom);
    assert_approx(entry.mode_line_height, 22.0, "mode line height");
}

#[test]
fn sonar_ping_entry_fields() {
    let now = std::time::Instant::now();
    let entry = SonarPingEntry {
        cx: 100.0,
        cy: 200.0,
        started: now,
        duration: std::time::Duration::from_millis(600),
    };
    assert_approx(entry.cx, 100.0, "ping cx");
    assert_approx(entry.cy, 200.0, "ping cy");
    assert_eq!(entry.duration, std::time::Duration::from_millis(600));
}

#[test]
fn edge_glow_entry_fields() {
    let now = std::time::Instant::now();
    let bounds = Rect::new(0.0, 0.0, 400.0, 300.0);
    let entry = EdgeGlowEntry {
        window_id: 99,
        bounds,
        at_top: false,
        started: now,
        duration: std::time::Duration::from_millis(400),
    };
    assert_eq!(entry.window_id, 99);
    assert!(!entry.at_top);
    assert_eq!(entry.duration, std::time::Duration::from_millis(400));
}

#[test]
fn window_fade_entry_fields() {
    let now = std::time::Instant::now();
    let bounds = Rect::new(10.0, 10.0, 300.0, 250.0);
    let entry = WindowFadeEntry {
        window_id: 3,
        bounds,
        started: now,
        duration: std::time::Duration::from_millis(200),
        intensity: 0.15,
    };
    assert_eq!(entry.window_id, 3);
    assert_approx(entry.intensity, 0.15, "fade intensity");
}

#[test]
fn scroll_momentum_entry_fields() {
    let now = std::time::Instant::now();
    let bounds = Rect::new(0.0, 0.0, 600.0, 400.0);
    let entry = ScrollMomentumEntry {
        window_id: 11,
        bounds,
        direction: 1,
        started: now,
        duration: std::time::Duration::from_millis(300),
    };
    assert_eq!(entry.window_id, 11);
    assert_eq!(entry.direction, 1);
}

#[test]
fn scroll_velocity_fade_entry_fields() {
    let now = std::time::Instant::now();
    let bounds = Rect::new(0.0, 0.0, 800.0, 600.0);
    let entry = ScrollVelocityFadeEntry {
        window_id: 5,
        bounds,
        velocity: 3.5,
        started: now,
        duration: std::time::Duration::from_millis(300),
    };
    assert_eq!(entry.window_id, 5);
    assert_approx(entry.velocity, 3.5, "velocity");
}

#[test]
fn mode_line_fade_entry_fields() {
    let now = std::time::Instant::now();
    let entry = ModeLineFadeEntry {
        window_id: 1,
        mode_line_y: 580.0,
        mode_line_h: 20.0,
        bounds_x: 0.0,
        bounds_w: 800.0,
        started: now,
        duration: std::time::Duration::from_millis(200),
    };
    assert_eq!(entry.window_id, 1);
    assert_approx(entry.mode_line_y, 580.0, "mode_line_y");
    assert_approx(entry.mode_line_h, 20.0, "mode_line_h");
}

// ────────────────────────────────────────────────────────────────────
// 14. Cursor trail distance check
// ────────────────────────────────────────────────────────────────────

#[test]
fn cursor_trail_distance_threshold() {
    // record_cursor_trail skips movements < 2.0 pixels
    let threshold = 2.0_f32;

    // Movement of 1.0 pixel: should be skipped
    let dx = 1.0_f32;
    let dy = 0.0_f32;
    let dist = (dx * dx + dy * dy).sqrt();
    assert!(dist < threshold, "should skip tiny movement");

    // Movement of 3.0 pixels: should be recorded
    let dx = 3.0_f32;
    let dist = (dx * dx + dy * dy).sqrt();
    assert!(dist >= threshold, "should record larger movement");

    // Diagonal 1.5, 1.5: sqrt(4.5) ~= 2.12 > 2.0
    let dx = 1.5_f32;
    let dy = 1.5_f32;
    let dist = (dx * dx + dy * dy).sqrt();
    assert!(dist >= threshold, "diagonal should be recorded");

    // Diagonal 1.0, 1.0: sqrt(2) ~= 1.414 < 2.0
    let dx = 1.0_f32;
    let dy = 1.0_f32;
    let dist = (dx * dx + dy * dy).sqrt();
    assert!(dist < threshold, "small diagonal should be skipped");
}

// ────────────────────────────────────────────────────────────────────
// 15. Bounds checking / point-in-rect logic used by multiple methods
// ────────────────────────────────────────────────────────────────────

fn point_in_rect(gx: f32, gy: f32, rect: &Rect) -> bool {
    gx >= rect.x && gx < rect.x + rect.width && gy >= rect.y && gy < rect.y + rect.height
}

#[test]
fn point_in_rect_inside() {
    let r = Rect::new(10.0, 20.0, 100.0, 50.0);
    assert!(point_in_rect(50.0, 40.0, &r));
}

#[test]
fn point_in_rect_at_origin() {
    let r = Rect::new(10.0, 20.0, 100.0, 50.0);
    assert!(point_in_rect(10.0, 20.0, &r)); // inclusive at min
}

#[test]
fn point_in_rect_at_max_boundary() {
    let r = Rect::new(10.0, 20.0, 100.0, 50.0);
    // Exclusive at max (gx < x + width)
    assert!(!point_in_rect(110.0, 20.0, &r));
    assert!(!point_in_rect(10.0, 70.0, &r));
}

#[test]
fn point_in_rect_outside() {
    let r = Rect::new(10.0, 20.0, 100.0, 50.0);
    assert!(!point_in_rect(5.0, 40.0, &r)); // left of
    assert!(!point_in_rect(50.0, 15.0, &r)); // above
}

// ────────────────────────────────────────────────────────────────────
// 16. Color construction patterns
// ────────────────────────────────────────────────────────────────────

#[test]
fn color_new_basic() {
    let c = Color::new(0.5, 0.6, 0.7, 0.8);
    assert_approx(c.r, 0.5, "r");
    assert_approx(c.g, 0.6, "g");
    assert_approx(c.b, 0.7, "b");
    assert_approx(c.a, 0.8, "a");
}

#[test]
fn color_from_tuple_pattern() {
    // This pattern is used in cursor_error_pulse_override
    let (r, g, b) = (1.0_f32, 0.2_f32, 0.2_f32);
    let alpha = 0.5_f32;
    let c = Color::new(r, g, b, alpha);
    assert_approx(c.r, 1.0, "r");
    assert_approx(c.g, 0.2, "g");
    assert_approx(c.b, 0.2, "b");
    assert_approx(c.a, 0.5, "a");
}

#[test]
fn hsl_color_struct_fields() {
    // hsl_to_color always sets a=1.0
    let c = WgpuRenderer::hsl_to_color(0.5, 0.5, 0.5);
    assert_approx(c.a, 1.0, "hsl always opaque");
    // r, g, b should all be in [0, 1]
    assert!(c.r >= 0.0 && c.r <= 1.0, "r in range");
    assert!(c.g >= 0.0 && c.g <= 1.0, "g in range");
    assert!(c.b >= 0.0 && c.b <= 1.0, "b in range");
}

// ────────────────────────────────────────────────────────────────────
// 17. Rect construction
// ────────────────────────────────────────────────────────────────────

#[test]
fn rect_new() {
    let r = Rect::new(1.0, 2.0, 3.0, 4.0);
    assert_approx(r.x, 1.0, "rect x");
    assert_approx(r.y, 2.0, "rect y");
    assert_approx(r.width, 3.0, "rect w");
    assert_approx(r.height, 4.0, "rect h");
}

// ────────────────────────────────────────────────────────────────────
// 18. HSL edge cases and sector transitions
// ────────────────────────────────────────────────────────────────────

#[test]
fn hsl_sector_boundaries() {
    // Test each of the 6 HSL sectors (h*6 as u32 = 0..5)
    // Sector 0: h=0.0 (red)
    let c0 = WgpuRenderer::hsl_to_color(0.0, 1.0, 0.5);
    assert_approx(c0.r, 1.0, "sector 0 r");

    // Sector 1: h=1/6 (yellow)
    let c1 = WgpuRenderer::hsl_to_color(1.0 / 6.0, 1.0, 0.5);
    assert_approx(c1.g, 1.0, "sector 1 g");

    // Sector 2: h=2/6 (green)
    let c2 = WgpuRenderer::hsl_to_color(2.0 / 6.0, 1.0, 0.5);
    assert_approx(c2.g, 1.0, "sector 2 g");

    // Sector 3: h=3/6 (cyan)
    let c3 = WgpuRenderer::hsl_to_color(3.0 / 6.0, 1.0, 0.5);
    assert_approx(c3.g, 1.0, "sector 3 g");
    assert_approx(c3.b, 1.0, "sector 3 b");

    // Sector 4: h=4/6 (blue-ish)
    let c4 = WgpuRenderer::hsl_to_color(4.0 / 6.0, 1.0, 0.5);
    assert_approx(c4.b, 1.0, "sector 4 b");

    // Sector 5: h=5/6 (magenta)
    let c5 = WgpuRenderer::hsl_to_color(5.0 / 6.0, 1.0, 0.5);
    assert_approx(c5.r, 1.0, "sector 5 r");
    assert_approx(c5.b, 1.0, "sector 5 b");
}

#[test]
fn hsl_light_and_dark_extremes() {
    // l=0 => black regardless of h, s
    let dark = WgpuRenderer::hsl_to_color(0.3, 0.8, 0.0);
    assert_approx(dark.r, 0.0, "dark r");
    assert_approx(dark.g, 0.0, "dark g");
    assert_approx(dark.b, 0.0, "dark b");

    // l=1 => white regardless of h, s
    let light = WgpuRenderer::hsl_to_color(0.7, 0.9, 1.0);
    assert_approx(light.r, 1.0, "light r");
    assert_approx(light.g, 1.0, "light g");
    assert_approx(light.b, 1.0, "light b");
}

// ────────────────────────────────────────────────────────────────────
// 19. Combined easing: offset decay over full animation lifecycle
// ────────────────────────────────────────────────────────────────────

#[test]
fn full_animation_lifecycle_line_anim() {
    let initial_offset = 30.0;
    let steps = 10;
    let mut values = Vec::new();
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        values.push(line_anim_offset_at(t, initial_offset));
    }
    // First value should be the initial offset
    assert_approx(values[0], initial_offset, "lifecycle start");
    // Last value should be zero
    assert_approx(values[steps], 0.0, "lifecycle end");
    // All intermediate values should be strictly decreasing
    for i in 1..=steps {
        assert!(
            values[i] <= values[i - 1] + 1e-6,
            "lifecycle decreasing at step {}: {} > {}",
            i,
            values[i],
            values[i - 1]
        );
    }
}

#[test]
fn full_animation_lifecycle_error_pulse() {
    let steps = 20;
    let mut values = Vec::new();
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        values.push(error_pulse_alpha_at(t));
    }
    // Starts at 1.0
    assert_approx(values[0], 1.0, "pulse lifecycle start");
    // Ends at 0.0
    assert_approx(values[steps], 0.0, "pulse lifecycle end");
    // Monotonically decreasing
    for i in 1..=steps {
        assert!(
            values[i] <= values[i - 1] + 1e-6,
            "pulse decreasing at step {}",
            i
        );
    }
}

#[test]
fn full_animation_lifecycle_text_fade() {
    let steps = 20;
    let mut values = Vec::new();
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        values.push(text_fade_alpha_at(t));
    }
    // Starts at 0.0
    assert_approx(values[0], 0.0, "text fade lifecycle start");
    // Ends at 1.0
    assert_approx(values[steps], 1.0, "text fade lifecycle end");
    // Monotonically increasing
    for i in 1..=steps {
        assert!(
            values[i] >= values[i - 1] - 1e-6,
            "text fade increasing at step {}",
            i
        );
    }
}
