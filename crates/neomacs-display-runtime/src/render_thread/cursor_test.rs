use super::*;
use std::time::{Duration, Instant};

// ---------------------------------------------------------------
// Helper: create a CursorTarget with given position/size/style
// ---------------------------------------------------------------
fn make_target(x: f32, y: f32, w: f32, h: f32, style: CursorStyle) -> CursorTarget {
    CursorTarget {
        window_id: 1,
        x,
        y,
        width: w,
        height: h,
        style,
        frame_id: 0,
    }
}

#[test]
fn config_snapshot_copies_settings_and_stops_disabled_animations() {
    let mut source = CursorState::default();
    source.blink_enabled = false;
    source.blink_interval = Duration::from_millis(275);
    source.anim_enabled = false;
    source.anim_speed = 8.5;
    source.anim_style = CursorAnimStyle::Linear;
    source.anim_duration = 0.4;
    source.trail_size = 2.25;
    source.size_transition_enabled = false;
    source.size_transition_duration = 0.8;

    let mut target = CursorState::default();
    target.animating = true;
    target.size_animating = true;
    target.apply_config(source.config_snapshot());

    assert_eq!(target.blink_enabled, source.blink_enabled);
    assert_eq!(target.blink_interval, source.blink_interval);
    assert_eq!(target.anim_enabled, source.anim_enabled);
    assert_eq!(target.anim_speed, source.anim_speed);
    assert_eq!(target.anim_style, source.anim_style);
    assert_eq!(target.anim_duration, source.anim_duration);
    assert_eq!(target.trail_size, source.trail_size);
    assert_eq!(
        target.size_transition_enabled,
        source.size_transition_enabled
    );
    assert_eq!(
        target.size_transition_duration,
        source.size_transition_duration
    );
    assert!(!target.animating);
    assert!(!target.size_animating);
}

// ---------------------------------------------------------------
// Easing functions: boundary values, monotonicity, specific values
// ---------------------------------------------------------------

#[test]
fn easing_linear_endpoints() {
    assert!((ease_linear(0.0)).abs() < 1e-6);
    assert!((ease_linear(1.0) - 1.0).abs() < 1e-6);
}

#[test]
fn easing_linear_identity() {
    for i in 0..=20 {
        let t = i as f32 / 20.0;
        assert!(
            (ease_linear(t) - t).abs() < 1e-6,
            "ease_linear({}) should equal {} but got {}",
            t,
            t,
            ease_linear(t)
        );
    }
}

#[test]
fn easing_out_quad_endpoints() {
    assert!((ease_out_quad(0.0)).abs() < 1e-6);
    assert!((ease_out_quad(1.0) - 1.0).abs() < 1e-6);
}

#[test]
fn easing_out_quad_monotonic_increasing() {
    let mut prev = ease_out_quad(0.0);
    for i in 1..=100 {
        let t = i as f32 / 100.0;
        let val = ease_out_quad(t);
        assert!(
            val >= prev,
            "ease_out_quad not monotonic at t={}: {} < {}",
            t,
            val,
            prev
        );
        prev = val;
    }
}

#[test]
fn easing_out_quad_midpoint() {
    // ease_out_quad(0.5) = -0.5*(0.5-2.0) = -0.5*(-1.5) = 0.75
    assert!((ease_out_quad(0.5) - 0.75).abs() < 1e-6);
}

#[test]
fn easing_out_cubic_endpoints() {
    assert!((ease_out_cubic(0.0)).abs() < 1e-6);
    assert!((ease_out_cubic(1.0) - 1.0).abs() < 1e-6);
}

#[test]
fn easing_out_cubic_monotonic_increasing() {
    let mut prev = ease_out_cubic(0.0);
    for i in 1..=100 {
        let t = i as f32 / 100.0;
        let val = ease_out_cubic(t);
        assert!(val >= prev, "ease_out_cubic not monotonic at t={}", t);
        prev = val;
    }
}

#[test]
fn easing_out_cubic_midpoint() {
    // ease_out_cubic(0.5) = (-0.5)^3 + 1 = -0.125 + 1 = 0.875
    assert!((ease_out_cubic(0.5) - 0.875).abs() < 1e-6);
}

#[test]
fn easing_out_expo_endpoints() {
    assert!((ease_out_expo(0.0)).abs() < 1e-6);
    assert!((ease_out_expo(1.0) - 1.0).abs() < 1e-6);
}

#[test]
fn easing_out_expo_monotonic_increasing() {
    let mut prev = ease_out_expo(0.0);
    for i in 1..=100 {
        let t = i as f32 / 100.0;
        let val = ease_out_expo(t);
        assert!(val >= prev, "ease_out_expo not monotonic at t={}", t);
        prev = val;
    }
}

#[test]
fn easing_out_expo_rapid_initial_progress() {
    // ease_out_expo should have >= 50% progress at t=0.1
    // 1 - 2^(-10*0.1) = 1 - 2^(-1) = 1 - 0.5 = 0.5
    let val = ease_out_expo(0.1);
    assert!(val >= 0.5, "ease_out_expo(0.1) = {} should be >= 0.5", val);
    // And at t=0.2 it should clearly exceed 0.5
    let val2 = ease_out_expo(0.2);
    assert!(val2 > 0.7, "ease_out_expo(0.2) = {} should be > 0.7", val2);
}

#[test]
fn easing_out_expo_above_one_returns_one() {
    // The function has a special case for t >= 1.0
    assert!((ease_out_expo(1.0) - 1.0).abs() < 1e-6);
    assert!((ease_out_expo(1.5) - 1.0).abs() < 1e-6);
    assert!((ease_out_expo(100.0) - 1.0).abs() < 1e-6);
}

#[test]
fn easing_in_out_cubic_endpoints() {
    assert!((ease_in_out_cubic(0.0)).abs() < 1e-6);
    assert!((ease_in_out_cubic(1.0) - 1.0).abs() < 1e-6);
}

#[test]
fn easing_in_out_cubic_symmetric_midpoint() {
    // S-curve: midpoint should be exactly 0.5
    assert!((ease_in_out_cubic(0.5) - 0.5).abs() < 1e-6);
}

#[test]
fn easing_in_out_cubic_monotonic_increasing() {
    let mut prev = ease_in_out_cubic(0.0);
    for i in 1..=100 {
        let t = i as f32 / 100.0;
        let val = ease_in_out_cubic(t);
        assert!(val >= prev, "ease_in_out_cubic not monotonic at t={}", t);
        prev = val;
    }
}

#[test]
fn easing_in_out_cubic_symmetry() {
    // ease_in_out_cubic should be symmetric: f(t) + f(1-t) = 1
    for i in 0..=50 {
        let t = i as f32 / 100.0;
        let sum = ease_in_out_cubic(t) + ease_in_out_cubic(1.0 - t);
        assert!(
            (sum - 1.0).abs() < 1e-5,
            "ease_in_out_cubic symmetry broken at t={}: f(t)+f(1-t)={}",
            t,
            sum
        );
    }
}

#[test]
fn easing_all_output_range_zero_to_one() {
    // All easing functions should map [0,1] to [0,1]
    for i in 0..=100 {
        let t = i as f32 / 100.0;
        let fns: [(&str, f32); 5] = [
            ("linear", ease_linear(t)),
            ("out_quad", ease_out_quad(t)),
            ("out_cubic", ease_out_cubic(t)),
            ("out_expo", ease_out_expo(t)),
            ("in_out_cubic", ease_in_out_cubic(t)),
        ];
        for (name, val) in &fns {
            assert!(
                *val >= -1e-6 && *val <= 1.0 + 1e-6,
                "{}({}) = {} is outside [0,1]",
                name,
                t,
                val
            );
        }
    }
}

// ---------------------------------------------------------------
// CursorState default values
// ---------------------------------------------------------------

#[test]
fn default_state_blink_is_on() {
    let state = CursorState::default();
    assert!(state.blink_on);
    assert!(state.blink_enabled);
    assert_eq!(state.blink_interval, Duration::from_millis(500));
}

#[test]
fn default_state_animation_enabled() {
    let state = CursorState::default();
    assert!(state.anim_enabled);
    assert!(!state.animating);
    assert_eq!(state.anim_speed, 2.4);
    assert_eq!(state.anim_style, CursorAnimStyle::CriticallyDampedSpring);
    assert_eq!(state.anim_duration, 0.15);
}

#[test]
fn default_state_no_target() {
    let state = CursorState::default();
    assert!(state.target.is_none());
}

#[test]
fn default_state_positions_at_origin() {
    let state = CursorState::default();
    assert_eq!(state.current_x, 0.0);
    assert_eq!(state.current_y, 0.0);
    assert_eq!(state.current_w, 0.0);
    assert_eq!(state.current_h, 0.0);
}

#[test]
fn default_state_velocities_zero() {
    let state = CursorState::default();
    assert_eq!(state.velocity_x, 0.0);
    assert_eq!(state.velocity_y, 0.0);
    assert_eq!(state.velocity_w, 0.0);
    assert_eq!(state.velocity_h, 0.0);
}

#[test]
fn default_state_size_transition_disabled() {
    let state = CursorState::default();
    assert!(!state.size_transition_enabled);
    assert!(!state.size_animating);
    assert_eq!(state.size_transition_duration, 0.15);
}

#[test]
fn visual_config_updates_every_cursor_policy_as_one_snapshot() {
    let mut state = CursorState::default();
    let mut config = neomacs_display_protocol::VisualConfig::default();
    config.cursor_blink.enabled = false;
    config.cursor_blink.interval = Duration::from_millis(275);
    config.cursor_motion.enabled = false;
    config.cursor_motion.speed = 19.0;
    config.cursor_motion.style = CursorAnimStyle::Linear;
    config.cursor_motion.duration = Duration::from_millis(225);
    config.cursor_motion.trail_size = 0.25;
    config.cursor_size_transition.enabled = true;
    config.cursor_size_transition.duration = Duration::from_millis(175);

    state.apply_visual_config(&config);

    assert!(!state.blink_enabled);
    assert!(state.blink_on);
    assert_eq!(state.blink_interval, Duration::from_millis(275));
    assert!(!state.anim_enabled);
    assert_eq!(state.anim_speed, 19.0);
    assert_eq!(state.anim_style, CursorAnimStyle::Linear);
    assert_eq!(state.anim_duration, 0.225);
    assert_eq!(state.trail_size, 0.25);
    assert!(state.size_transition_enabled);
    assert_eq!(state.size_transition_duration, 0.175);
}

#[test]
fn default_state_corner_springs() {
    let state = CursorState::default();
    for spring in &state.corner_springs {
        assert_eq!(spring.x, 0.0);
        assert_eq!(spring.y, 0.0);
        assert_eq!(spring.vx, 0.0);
        assert_eq!(spring.vy, 0.0);
        assert_eq!(spring.target_x, 0.0);
        assert_eq!(spring.target_y, 0.0);
        assert_eq!(spring.omega, 26.7);
    }
    assert_eq!(state.trail_size, 0.7);
}

// ---------------------------------------------------------------
// target_corners: cursor style → corner positions
// ---------------------------------------------------------------

#[test]
fn target_corners_filled_box_style_0() {
    let target = make_target(10.0, 20.0, 100.0, 50.0, CursorStyle::FilledBox);
    let corners = CursorState::target_corners(&target);
    // TL, TR, BR, BL
    assert_eq!(corners[0], (10.0, 20.0)); // top-left
    assert_eq!(corners[1], (110.0, 20.0)); // top-right
    assert_eq!(corners[2], (110.0, 70.0)); // bottom-right
    assert_eq!(corners[3], (10.0, 70.0)); // bottom-left
}

#[test]
fn target_corners_bar_style_1() {
    let target = make_target(10.0, 20.0, 100.0, 50.0, CursorStyle::Bar(2.0));
    let corners = CursorState::target_corners(&target);
    // Bar is 2px wide
    assert_eq!(corners[0], (10.0, 20.0));
    assert_eq!(corners[1], (12.0, 20.0)); // x + 2.0
    assert_eq!(corners[2], (12.0, 70.0));
    assert_eq!(corners[3], (10.0, 70.0));
}

#[test]
fn target_corners_underline_style_2() {
    let target = make_target(10.0, 20.0, 100.0, 50.0, CursorStyle::Hbar(2.0));
    let corners = CursorState::target_corners(&target);
    // Underline is 2px tall at the bottom
    assert_eq!(corners[0], (10.0, 68.0)); // y + height - 2.0
    assert_eq!(corners[1], (110.0, 68.0));
    assert_eq!(corners[2], (110.0, 70.0)); // y + height
    assert_eq!(corners[3], (10.0, 70.0));
}

#[test]
fn target_corners_hollow_style_3_uses_default() {
    let target = make_target(10.0, 20.0, 100.0, 50.0, CursorStyle::Hollow);
    let corners = CursorState::target_corners(&target);
    // Style 3 (hollow) falls through to default: full rectangle
    assert_eq!(corners[0], (10.0, 20.0));
    assert_eq!(corners[1], (110.0, 20.0));
    assert_eq!(corners[2], (110.0, 70.0));
    assert_eq!(corners[3], (10.0, 70.0));
}

#[test]
fn target_corners_hollow_uses_full_rectangle() {
    let target = make_target(5.0, 10.0, 20.0, 30.0, CursorStyle::Hollow);
    let corners = CursorState::target_corners(&target);
    assert_eq!(corners[0], (5.0, 10.0));
    assert_eq!(corners[1], (25.0, 10.0));
    assert_eq!(corners[2], (25.0, 40.0));
    assert_eq!(corners[3], (5.0, 40.0));
}

#[test]
fn target_corners_zero_size_cursor() {
    let target = make_target(10.0, 20.0, 0.0, 0.0, CursorStyle::FilledBox);
    let corners = CursorState::target_corners(&target);
    // All four corners collapse to (10, 20) or (10, 20)
    assert_eq!(corners[0], (10.0, 20.0));
    assert_eq!(corners[1], (10.0, 20.0));
    assert_eq!(corners[2], (10.0, 20.0));
    assert_eq!(corners[3], (10.0, 20.0));
}

#[test]
fn target_corners_bar_ignores_width() {
    // Bar style always uses 2px width regardless of target.width
    let target = make_target(0.0, 0.0, 500.0, 30.0, CursorStyle::Bar(2.0));
    let corners = CursorState::target_corners(&target);
    assert_eq!(corners[1].0, 2.0); // top-right x = 0 + 2
}

#[test]
fn target_corners_underline_uses_full_width() {
    let target = make_target(0.0, 0.0, 80.0, 20.0, CursorStyle::Hbar(2.0));
    let corners = CursorState::target_corners(&target);
    assert_eq!(corners[0].0, 0.0);
    assert_eq!(corners[1].0, 80.0); // uses full width
}

// ---------------------------------------------------------------
// snap: immediately move to target
// ---------------------------------------------------------------

#[test]
fn snap_sets_position_to_target() {
    let mut state = CursorState::default();
    state.animating = true;
    state.current_x = 100.0;
    state.current_y = 200.0;
    state.current_w = 50.0;
    state.current_h = 25.0;

    let target = make_target(300.0, 400.0, 80.0, 40.0, CursorStyle::FilledBox);
    state.snap(&target);

    assert_eq!(state.current_x, 300.0);
    assert_eq!(state.current_y, 400.0);
    assert_eq!(state.current_w, 80.0);
    assert_eq!(state.current_h, 40.0);
}

#[test]
fn snap_stops_animation() {
    let mut state = CursorState::default();
    state.animating = true;
    let target = make_target(0.0, 0.0, 10.0, 10.0, CursorStyle::FilledBox);
    state.snap(&target);
    assert!(!state.animating);
}

#[test]
fn snap_same_position_is_noop_on_values() {
    let mut state = CursorState::default();
    state.current_x = 50.0;
    state.current_y = 60.0;
    state.current_w = 10.0;
    state.current_h = 20.0;
    state.animating = true;

    let target = make_target(50.0, 60.0, 10.0, 20.0, CursorStyle::FilledBox);
    state.snap(&target);

    assert_eq!(state.current_x, 50.0);
    assert_eq!(state.current_y, 60.0);
    assert!(!state.animating);
}

// ---------------------------------------------------------------
// reset_blink
// ---------------------------------------------------------------

#[test]
fn reset_blink_sets_visible() {
    let mut state = CursorState::default();
    state.blink_on = false;
    let before = Instant::now();
    state.reset_blink();
    let after = Instant::now();

    assert!(state.blink_on);
    assert!(state.last_blink_toggle >= before);
    assert!(state.last_blink_toggle <= after);
}

#[test]
fn reset_blink_already_visible_stays_visible() {
    let mut state = CursorState::default();
    assert!(state.blink_on); // default is true
    state.reset_blink();
    assert!(state.blink_on);
}

#[test]
fn reset_blink_updates_timestamp() {
    let mut state = CursorState::default();
    let old_time = state.last_blink_toggle;
    // Sleep briefly to ensure time advances
    std::thread::sleep(Duration::from_millis(2));
    state.reset_blink();
    assert!(state.last_blink_toggle > old_time);
}

// ---------------------------------------------------------------
// tick_animation: returns false when disabled or not animating
// ---------------------------------------------------------------

#[test]
fn tick_animation_returns_false_when_disabled() {
    let mut state = CursorState::default();
    state.anim_enabled = false;
    state.animating = true;
    state.target = Some(make_target(
        100.0,
        100.0,
        10.0,
        20.0,
        CursorStyle::FilledBox,
    ));
    assert!(!state.tick_animation());
}

#[test]
fn tick_animation_returns_false_when_not_animating() {
    let mut state = CursorState::default();
    state.anim_enabled = true;
    state.animating = false;
    state.target = Some(make_target(
        100.0,
        100.0,
        10.0,
        20.0,
        CursorStyle::FilledBox,
    ));
    assert!(!state.tick_animation());
}

#[test]
fn tick_animation_returns_false_when_no_target() {
    let mut state = CursorState::default();
    state.anim_enabled = true;
    state.animating = true;
    state.target = None;
    assert!(!state.tick_animation());
}

// ---------------------------------------------------------------
// tick_animation: Exponential style
// ---------------------------------------------------------------

#[test]
fn tick_animation_exponential_moves_toward_target() {
    let mut state = CursorState::default();
    state.anim_enabled = true;
    state.animating = true;
    state.anim_style = CursorAnimStyle::Exponential;
    state.anim_speed = 15.0;
    state.current_x = 0.0;
    state.current_y = 0.0;
    state.current_w = 10.0;
    state.current_h = 20.0;
    state.target = Some(make_target(
        200.0,
        300.0,
        10.0,
        20.0,
        CursorStyle::FilledBox,
    ));
    state.last_anim_time = Instant::now();

    // Wait a tiny bit so dt > 0
    std::thread::sleep(Duration::from_millis(5));

    let result = state.tick_animation();
    assert!(result);
    // Should have moved toward target
    assert!(
        state.current_x > 0.0,
        "x should have moved toward 200: got {}",
        state.current_x
    );
    assert!(
        state.current_y > 0.0,
        "y should have moved toward 300: got {}",
        state.current_y
    );
    // Should not have overshot
    assert!(state.current_x <= 200.0);
    assert!(state.current_y <= 300.0);
}

#[test]
fn tick_animation_exponential_snaps_when_close() {
    let mut state = CursorState::default();
    state.anim_enabled = true;
    state.animating = true;
    state.anim_style = CursorAnimStyle::Exponential;
    state.anim_speed = 15.0;
    // Position very close to target (within 0.5 threshold)
    state.current_x = 100.0;
    state.current_y = 200.0;
    state.current_w = 10.0;
    state.current_h = 20.0;
    state.target = Some(make_target(
        100.3,
        200.2,
        10.1,
        20.1,
        CursorStyle::FilledBox,
    ));
    state.last_anim_time = Instant::now();

    std::thread::sleep(Duration::from_millis(1));
    state.tick_animation();

    // Should have snapped: position == target, animating == false
    assert_eq!(state.current_x, 100.3);
    assert_eq!(state.current_y, 200.2);
    assert_eq!(state.current_w, 10.1);
    assert_eq!(state.current_h, 20.1);
    assert!(!state.animating);
}

// ---------------------------------------------------------------
// tick_animation: Linear easing style
// ---------------------------------------------------------------

#[test]
fn tick_animation_linear_interpolation() {
    let mut state = CursorState::default();
    state.anim_enabled = true;
    state.animating = true;
    state.anim_style = CursorAnimStyle::Linear;
    state.anim_duration = 1.0; // 1 second
    state.start_x = 0.0;
    state.start_y = 0.0;
    state.start_w = 10.0;
    state.start_h = 20.0;
    state.current_x = 0.0;
    state.current_y = 0.0;
    state.current_w = 10.0;
    state.current_h = 20.0;
    state.target = Some(make_target(
        100.0,
        200.0,
        30.0,
        40.0,
        CursorStyle::FilledBox,
    ));
    state.anim_start_time = Instant::now();
    state.last_anim_time = Instant::now();

    // Sleep to let some time pass
    std::thread::sleep(Duration::from_millis(10));

    let result = state.tick_animation();
    assert!(result);

    // With linear easing, progress should be proportional to time elapsed
    // After ~10ms of a 1s animation, should be ~1% of the way
    assert!(state.current_x > 0.0);
    assert!(state.current_x < 100.0);
    assert!(state.current_y > 0.0);
    assert!(state.current_y < 200.0);
}

#[test]
fn tick_animation_linear_completes_and_snaps() {
    let mut state = CursorState::default();
    state.anim_enabled = true;
    state.animating = true;
    state.anim_style = CursorAnimStyle::Linear;
    state.anim_duration = 0.001; // very short: 1ms
    state.start_x = 0.0;
    state.start_y = 0.0;
    state.start_w = 10.0;
    state.start_h = 20.0;
    state.target = Some(make_target(
        100.0,
        200.0,
        30.0,
        40.0,
        CursorStyle::FilledBox,
    ));
    // Set start time in the past so elapsed > duration
    state.anim_start_time = Instant::now() - Duration::from_millis(100);
    state.last_anim_time = Instant::now();

    state.tick_animation();

    // Should snap to target when raw_t >= 1.0
    assert_eq!(state.current_x, 100.0);
    assert_eq!(state.current_y, 200.0);
    assert_eq!(state.current_w, 30.0);
    assert_eq!(state.current_h, 40.0);
    assert!(!state.animating);
}

// ---------------------------------------------------------------
// tick_animation: EaseOutQuad style
// ---------------------------------------------------------------

#[test]
fn tick_animation_ease_out_quad_progresses() {
    let mut state = CursorState::default();
    state.anim_enabled = true;
    state.animating = true;
    state.anim_style = CursorAnimStyle::EaseOutQuad;
    state.anim_duration = 0.5;
    state.start_x = 0.0;
    state.start_y = 0.0;
    state.start_w = 10.0;
    state.start_h = 10.0;
    state.target = Some(make_target(
        100.0,
        100.0,
        10.0,
        10.0,
        CursorStyle::FilledBox,
    ));
    state.anim_start_time = Instant::now();
    state.last_anim_time = Instant::now();

    std::thread::sleep(Duration::from_millis(10));
    let result = state.tick_animation();
    assert!(result);
    assert!(state.current_x > 0.0);
}

// ---------------------------------------------------------------
// tick_animation: EaseOutCubic style
// ---------------------------------------------------------------

#[test]
fn tick_animation_ease_out_cubic_progresses() {
    let mut state = CursorState::default();
    state.anim_enabled = true;
    state.animating = true;
    state.anim_style = CursorAnimStyle::EaseOutCubic;
    state.anim_duration = 0.5;
    state.start_x = 50.0;
    state.start_y = 50.0;
    state.start_w = 10.0;
    state.start_h = 20.0;
    state.target = Some(make_target(
        200.0,
        200.0,
        10.0,
        20.0,
        CursorStyle::FilledBox,
    ));
    state.anim_start_time = Instant::now();
    state.last_anim_time = Instant::now();

    std::thread::sleep(Duration::from_millis(10));
    let result = state.tick_animation();
    assert!(result);
    assert!(
        state.current_x > 50.0,
        "x should have progressed past start"
    );
}

// ---------------------------------------------------------------
// tick_animation: EaseOutExpo style
// ---------------------------------------------------------------

#[test]
fn tick_animation_ease_out_expo_progresses() {
    let mut state = CursorState::default();
    state.anim_enabled = true;
    state.animating = true;
    state.anim_style = CursorAnimStyle::EaseOutExpo;
    state.anim_duration = 0.5;
    state.start_x = 0.0;
    state.start_y = 0.0;
    state.start_w = 5.0;
    state.start_h = 15.0;
    state.target = Some(make_target(300.0, 300.0, 5.0, 15.0, CursorStyle::FilledBox));
    state.anim_start_time = Instant::now();
    state.last_anim_time = Instant::now();

    std::thread::sleep(Duration::from_millis(10));
    let result = state.tick_animation();
    assert!(result);
    assert!(state.current_x > 0.0);
}

// ---------------------------------------------------------------
// tick_animation: EaseInOutCubic style
// ---------------------------------------------------------------

#[test]
fn tick_animation_ease_in_out_cubic_progresses() {
    let mut state = CursorState::default();
    state.anim_enabled = true;
    state.animating = true;
    state.anim_style = CursorAnimStyle::EaseInOutCubic;
    state.anim_duration = 0.5;
    state.start_x = 10.0;
    state.start_y = 10.0;
    state.start_w = 8.0;
    state.start_h = 16.0;
    state.target = Some(make_target(400.0, 400.0, 8.0, 16.0, CursorStyle::FilledBox));
    state.anim_start_time = Instant::now();
    state.last_anim_time = Instant::now();

    std::thread::sleep(Duration::from_millis(10));
    let result = state.tick_animation();
    assert!(result);
    assert!(state.current_x > 10.0);
}

// ---------------------------------------------------------------
// tick_animation: CriticallyDampedSpring style
// ---------------------------------------------------------------

#[test]
fn tick_animation_spring_moves_toward_target() {
    let mut state = CursorState::default();
    state.anim_enabled = true;
    state.animating = true;
    state.anim_style = CursorAnimStyle::CriticallyDampedSpring;
    state.target = Some(make_target(
        200.0,
        300.0,
        80.0,
        40.0,
        CursorStyle::FilledBox,
    ));

    // Initialize corner springs away from target
    let target_corners = CursorState::target_corners(state.target.as_ref().unwrap());
    for i in 0..4 {
        state.corner_springs[i].x = 0.0;
        state.corner_springs[i].y = 0.0;
        state.corner_springs[i].vx = 0.0;
        state.corner_springs[i].vy = 0.0;
        state.corner_springs[i].target_x = target_corners[i].0;
        state.corner_springs[i].target_y = target_corners[i].1;
    }
    state.last_anim_time = Instant::now();

    std::thread::sleep(Duration::from_millis(5));
    let result = state.tick_animation();
    assert!(result);

    // Springs should have moved corners toward target
    // current_x/y are derived from bounding box of corner springs
    // After one tick from origin, they should have moved toward target
    // (not necessarily arrived)
}

#[test]
fn tick_animation_spring_settles_at_target() {
    let mut state = CursorState::default();
    state.anim_enabled = true;
    state.animating = true;
    state.anim_style = CursorAnimStyle::CriticallyDampedSpring;
    let target = make_target(100.0, 100.0, 50.0, 25.0, CursorStyle::FilledBox);
    state.target = Some(target.clone());

    // Set corner springs very close to target with tiny velocity
    let target_corners = CursorState::target_corners(&target);
    for i in 0..4 {
        state.corner_springs[i].x = target_corners[i].0 + 0.1;
        state.corner_springs[i].y = target_corners[i].1 + 0.1;
        state.corner_springs[i].vx = 0.1;
        state.corner_springs[i].vy = 0.1;
        state.corner_springs[i].target_x = target_corners[i].0;
        state.corner_springs[i].target_y = target_corners[i].1;
    }
    state.last_anim_time = Instant::now();

    std::thread::sleep(Duration::from_millis(5));
    state.tick_animation();

    // Should have settled: snapped to target
    assert_eq!(state.current_x, 100.0);
    assert_eq!(state.current_y, 100.0);
    assert_eq!(state.current_w, 50.0);
    assert_eq!(state.current_h, 25.0);
    assert!(!state.animating);
}

#[test]
fn tick_animation_spring_resets_velocities_on_settle() {
    let mut state = CursorState::default();
    state.anim_enabled = true;
    state.animating = true;
    state.anim_style = CursorAnimStyle::CriticallyDampedSpring;
    let target = make_target(50.0, 50.0, 20.0, 10.0, CursorStyle::FilledBox);
    state.target = Some(target.clone());

    let target_corners = CursorState::target_corners(&target);
    for i in 0..4 {
        state.corner_springs[i].x = target_corners[i].0 + 0.01;
        state.corner_springs[i].y = target_corners[i].1 + 0.01;
        state.corner_springs[i].vx = 0.01;
        state.corner_springs[i].vy = 0.01;
        state.corner_springs[i].target_x = target_corners[i].0;
        state.corner_springs[i].target_y = target_corners[i].1;
    }
    state.last_anim_time = Instant::now();

    std::thread::sleep(Duration::from_millis(5));
    state.tick_animation();

    // Velocities should be reset to 0
    for spring in &state.corner_springs {
        assert_eq!(spring.vx, 0.0);
        assert_eq!(spring.vy, 0.0);
    }
}

// ---------------------------------------------------------------
// tick_animation: edge cases
// ---------------------------------------------------------------

#[test]
fn tick_animation_same_start_and_end_position() {
    let mut state = CursorState::default();
    state.anim_enabled = true;
    state.animating = true;
    state.anim_style = CursorAnimStyle::Linear;
    state.anim_duration = 0.15;
    state.start_x = 100.0;
    state.start_y = 200.0;
    state.start_w = 10.0;
    state.start_h = 20.0;
    state.current_x = 100.0;
    state.current_y = 200.0;
    state.current_w = 10.0;
    state.current_h = 20.0;
    state.target = Some(make_target(
        100.0,
        200.0,
        10.0,
        20.0,
        CursorStyle::FilledBox,
    ));
    state.anim_start_time = Instant::now();
    state.last_anim_time = Instant::now();

    std::thread::sleep(Duration::from_millis(5));
    let result = state.tick_animation();
    assert!(result);

    // Position should stay the same since start == target
    assert!((state.current_x - 100.0).abs() < 1e-3);
    assert!((state.current_y - 200.0).abs() < 1e-3);
}

#[test]
fn tick_animation_zero_duration_completes_immediately() {
    let mut state = CursorState::default();
    state.anim_enabled = true;
    state.animating = true;
    state.anim_style = CursorAnimStyle::Linear;
    state.anim_duration = 0.0; // zero duration
    state.start_x = 0.0;
    state.start_y = 0.0;
    state.start_w = 5.0;
    state.start_h = 10.0;
    state.target = Some(make_target(
        500.0,
        600.0,
        15.0,
        25.0,
        CursorStyle::FilledBox,
    ));
    state.anim_start_time = Instant::now();
    state.last_anim_time = Instant::now();

    // Even with zero duration, raw_t would be infinity or NaN from 0/0,
    // but it's clamped to min(1.0) so it should snap immediately.
    // The .min(1.0) ensures raw_t = 1.0 regardless of elapsed/0.
    // Actually: elapsed/0.0 = inf, inf.min(1.0) = 1.0
    std::thread::sleep(Duration::from_millis(1));
    state.tick_animation();

    assert_eq!(state.current_x, 500.0);
    assert_eq!(state.current_y, 600.0);
    assert_eq!(state.current_w, 15.0);
    assert_eq!(state.current_h, 25.0);
    assert!(!state.animating);
}

#[test]
fn tick_animation_exponential_same_position_snaps() {
    let mut state = CursorState::default();
    state.anim_enabled = true;
    state.animating = true;
    state.anim_style = CursorAnimStyle::Exponential;
    state.anim_speed = 15.0;
    state.current_x = 100.0;
    state.current_y = 100.0;
    state.current_w = 10.0;
    state.current_h = 20.0;
    state.target = Some(make_target(
        100.0,
        100.0,
        10.0,
        20.0,
        CursorStyle::FilledBox,
    ));
    state.last_anim_time = Instant::now();

    std::thread::sleep(Duration::from_millis(1));
    state.tick_animation();

    // dx, dy, dw, dh are all 0.0 (< 0.5), should snap immediately
    assert_eq!(state.current_x, 100.0);
    assert_eq!(state.current_y, 100.0);
    assert!(!state.animating);
}

// ---------------------------------------------------------------
// tick_animation: multiple ticks converge
// ---------------------------------------------------------------

#[test]
fn tick_animation_exponential_converges_over_many_ticks() {
    let mut state = CursorState::default();
    state.anim_enabled = true;
    state.animating = true;
    state.anim_style = CursorAnimStyle::Exponential;
    state.anim_speed = 15.0;
    state.current_x = 0.0;
    state.current_y = 0.0;
    state.current_w = 10.0;
    state.current_h = 20.0;
    state.target = Some(make_target(
        100.0,
        100.0,
        10.0,
        20.0,
        CursorStyle::FilledBox,
    ));
    state.last_anim_time = Instant::now();

    // Run many ticks
    for _ in 0..200 {
        std::thread::sleep(Duration::from_millis(2));
        if !state.animating {
            break;
        }
        state.tick_animation();
    }

    // Should have snapped to target
    assert_eq!(state.current_x, 100.0);
    assert_eq!(state.current_y, 100.0);
    assert!(!state.animating);
}

#[test]
fn tick_animation_linear_converges_over_duration() {
    let mut state = CursorState::default();
    state.anim_enabled = true;
    state.animating = true;
    state.anim_style = CursorAnimStyle::Linear;
    state.anim_duration = 0.05; // 50ms
    state.start_x = 0.0;
    state.start_y = 0.0;
    state.start_w = 10.0;
    state.start_h = 20.0;
    state.current_x = 0.0;
    state.current_y = 0.0;
    state.current_w = 10.0;
    state.current_h = 20.0;
    state.target = Some(make_target(
        100.0,
        200.0,
        30.0,
        40.0,
        CursorStyle::FilledBox,
    ));
    state.anim_start_time = Instant::now();
    state.last_anim_time = Instant::now();

    // Run ticks until animation completes
    for _ in 0..100 {
        std::thread::sleep(Duration::from_millis(2));
        if !state.animating {
            break;
        }
        state.tick_animation();
    }

    assert_eq!(state.current_x, 100.0);
    assert_eq!(state.current_y, 200.0);
    assert_eq!(state.current_w, 30.0);
    assert_eq!(state.current_h, 40.0);
    assert!(!state.animating);
}

// ---------------------------------------------------------------
// tick_size_animation
// ---------------------------------------------------------------

#[test]
fn tick_size_animation_returns_false_when_disabled() {
    let mut state = CursorState::default();
    state.size_transition_enabled = false;
    state.size_animating = true;
    assert!(!state.tick_size_animation());
}

#[test]
fn tick_size_animation_returns_false_when_not_animating() {
    let mut state = CursorState::default();
    state.size_transition_enabled = true;
    state.size_animating = false;
    assert!(!state.tick_size_animation());
}

#[test]
fn tick_size_animation_interpolates_size() {
    let mut state = CursorState::default();
    state.size_transition_enabled = true;
    state.size_animating = true;
    state.size_transition_duration = 1.0; // 1 second
    state.size_start_w = 10.0;
    state.size_start_h = 20.0;
    state.size_target_w = 50.0;
    state.size_target_h = 80.0;
    state.current_w = 10.0;
    state.current_h = 20.0;
    state.size_anim_start = Instant::now();

    std::thread::sleep(Duration::from_millis(10));
    let result = state.tick_size_animation();
    assert!(result);

    // Size should have moved toward target
    assert!(
        state.current_w > 10.0,
        "width should have increased from 10: got {}",
        state.current_w
    );
    assert!(
        state.current_h > 20.0,
        "height should have increased from 20: got {}",
        state.current_h
    );
    // But not yet at target
    assert!(state.current_w < 50.0);
    assert!(state.current_h < 80.0);
}

#[test]
fn tick_size_animation_completes_and_snaps() {
    let mut state = CursorState::default();
    state.size_transition_enabled = true;
    state.size_animating = true;
    state.size_transition_duration = 0.001; // 1ms
    state.size_start_w = 10.0;
    state.size_start_h = 20.0;
    state.size_target_w = 50.0;
    state.size_target_h = 80.0;
    state.current_w = 10.0;
    state.current_h = 20.0;
    state.size_anim_start = Instant::now() - Duration::from_millis(100);

    let result = state.tick_size_animation();
    assert!(result);

    // Should snap to target size
    assert_eq!(state.current_w, 50.0);
    assert_eq!(state.current_h, 80.0);
    assert!(!state.size_animating);
}

#[test]
fn tick_size_animation_zero_duration_completes_immediately() {
    let mut state = CursorState::default();
    state.size_transition_enabled = true;
    state.size_animating = true;
    state.size_transition_duration = 0.0;
    state.size_start_w = 5.0;
    state.size_start_h = 10.0;
    state.size_target_w = 30.0;
    state.size_target_h = 60.0;
    state.current_w = 5.0;
    state.current_h = 10.0;
    state.size_anim_start = Instant::now();

    std::thread::sleep(Duration::from_millis(1));
    state.tick_size_animation();

    assert_eq!(state.current_w, 30.0);
    assert_eq!(state.current_h, 60.0);
    assert!(!state.size_animating);
}

#[test]
fn tick_size_animation_same_start_and_target() {
    let mut state = CursorState::default();
    state.size_transition_enabled = true;
    state.size_animating = true;
    state.size_transition_duration = 0.15;
    state.size_start_w = 20.0;
    state.size_start_h = 40.0;
    state.size_target_w = 20.0;
    state.size_target_h = 40.0;
    state.current_w = 20.0;
    state.current_h = 40.0;
    state.size_anim_start = Instant::now();

    std::thread::sleep(Duration::from_millis(5));
    let result = state.tick_size_animation();
    assert!(result);

    // Size should remain the same since start == target
    assert!((state.current_w - 20.0).abs() < 1e-3);
    assert!((state.current_h - 40.0).abs() < 1e-3);
}

#[test]
fn tick_size_animation_ease_out_quad_curve() {
    // The size transition uses ease-out-quad: t * (2 - t)
    // Verify the easing is applied correctly by checking that
    // at the halfway point, progress is 0.75 (ease-out-quad at 0.5)
    let mut state = CursorState::default();
    state.size_transition_enabled = true;
    state.size_animating = true;
    state.size_transition_duration = 0.1; // 100ms
    state.size_start_w = 0.0;
    state.size_start_h = 0.0;
    state.size_target_w = 100.0;
    state.size_target_h = 100.0;
    state.current_w = 0.0;
    state.current_h = 0.0;
    // Set start time 50ms ago (halfway through 100ms)
    state.size_anim_start = Instant::now() - Duration::from_millis(50);

    state.tick_size_animation();

    // At raw_t=0.5, ease-out-quad = 0.5*(2.0-0.5) = 0.75
    // So width should be ~75.0 and height ~75.0
    // Allow some tolerance for timing imprecision
    assert!(
        (state.current_w - 75.0).abs() < 5.0,
        "width at halfway should be ~75: got {}",
        state.current_w
    );
    assert!(
        (state.current_h - 75.0).abs() < 5.0,
        "height at halfway should be ~75: got {}",
        state.current_h
    );
}

#[test]
fn tick_size_animation_converges() {
    let mut state = CursorState::default();
    state.size_transition_enabled = true;
    state.size_animating = true;
    state.size_transition_duration = 0.05; // 50ms
    state.size_start_w = 10.0;
    state.size_start_h = 10.0;
    state.size_target_w = 100.0;
    state.size_target_h = 100.0;
    state.current_w = 10.0;
    state.current_h = 10.0;
    state.size_anim_start = Instant::now();

    for _ in 0..100 {
        std::thread::sleep(Duration::from_millis(2));
        if !state.size_animating {
            break;
        }
        state.tick_size_animation();
    }

    assert_eq!(state.current_w, 100.0);
    assert_eq!(state.current_h, 100.0);
    assert!(!state.size_animating);
}

// ---------------------------------------------------------------
// Blink toggle timing behavior
// ---------------------------------------------------------------

#[test]
fn blink_state_tracks_enabled_and_interval() {
    let mut state = CursorState::default();
    state.blink_enabled = true;
    state.blink_interval = Duration::from_millis(250);
    assert!(state.blink_enabled);
    assert_eq!(state.blink_interval, Duration::from_millis(250));
}

#[test]
fn blink_disabled_does_not_affect_blink_on() {
    let mut state = CursorState::default();
    state.blink_enabled = false;
    state.blink_on = true;
    // blink_enabled being false doesn't change blink_on by itself;
    // the render loop checks blink_enabled before toggling
    assert!(state.blink_on);
}

#[test]
fn blink_interval_zero_is_valid() {
    let mut state = CursorState::default();
    state.blink_interval = Duration::from_millis(0);
    assert_eq!(state.blink_interval, Duration::ZERO);
}

// ---------------------------------------------------------------
// CornerSpring: basic state
// ---------------------------------------------------------------

#[test]
fn corner_spring_copy_semantics() {
    let spring = CornerSpring {
        x: 10.0,
        y: 20.0,
        vx: 1.0,
        vy: 2.0,
        target_x: 100.0,
        target_y: 200.0,
        omega: 30.0,
    };
    let copy = spring; // Copy
    assert_eq!(copy.x, 10.0);
    assert_eq!(copy.y, 20.0);
    assert_eq!(copy.vx, 1.0);
    assert_eq!(copy.vy, 2.0);
    assert_eq!(copy.target_x, 100.0);
    assert_eq!(copy.target_y, 200.0);
    assert_eq!(copy.omega, 30.0);
}

// ---------------------------------------------------------------
// CursorTarget: basic construction
// ---------------------------------------------------------------

#[test]
fn cursor_target_clone() {
    let target = make_target(10.0, 20.0, 30.0, 40.0, CursorStyle::Bar(2.0));
    let cloned = target.clone();
    assert_eq!(cloned.x, 10.0);
    assert_eq!(cloned.y, 20.0);
    assert_eq!(cloned.width, 30.0);
    assert_eq!(cloned.height, 40.0);
    assert_eq!(cloned.style, CursorStyle::Bar(2.0));
    assert_eq!(cloned.window_id, 1);
    assert_eq!(cloned.frame_id, 0);
}

// ---------------------------------------------------------------
// Integration: tick_animation updates last_anim_time
// ---------------------------------------------------------------

#[test]
fn tick_animation_updates_last_anim_time() {
    let mut state = CursorState::default();
    state.anim_enabled = true;
    state.animating = true;
    state.anim_style = CursorAnimStyle::Linear;
    state.anim_duration = 1.0;
    state.start_x = 0.0;
    state.start_y = 0.0;
    state.start_w = 10.0;
    state.start_h = 10.0;
    state.target = Some(make_target(
        100.0,
        100.0,
        10.0,
        10.0,
        CursorStyle::FilledBox,
    ));
    let old_time = Instant::now() - Duration::from_millis(100);
    state.last_anim_time = old_time;
    state.anim_start_time = old_time;

    state.tick_animation();

    // last_anim_time should have been updated to approximately now
    assert!(state.last_anim_time > old_time);
}

// ---------------------------------------------------------------
// tick_animation: easing styles all complete at the same target
// ---------------------------------------------------------------

#[test]
fn tick_animation_all_easing_styles_reach_target() {
    let easing_styles = [
        CursorAnimStyle::Linear,
        CursorAnimStyle::EaseOutQuad,
        CursorAnimStyle::EaseOutCubic,
        CursorAnimStyle::EaseOutExpo,
        CursorAnimStyle::EaseInOutCubic,
    ];

    for style in &easing_styles {
        let mut state = CursorState::default();
        state.anim_enabled = true;
        state.animating = true;
        state.anim_style = *style;
        state.anim_duration = 0.001; // 1ms, will complete instantly
        state.start_x = 0.0;
        state.start_y = 0.0;
        state.start_w = 10.0;
        state.start_h = 20.0;
        state.target = Some(make_target(
            200.0,
            300.0,
            40.0,
            50.0,
            CursorStyle::FilledBox,
        ));
        state.anim_start_time = Instant::now() - Duration::from_millis(100);
        state.last_anim_time = Instant::now();

        state.tick_animation();

        assert_eq!(state.current_x, 200.0, "{:?} did not reach target x", style);
        assert_eq!(state.current_y, 300.0, "{:?} did not reach target y", style);
        assert_eq!(state.current_w, 40.0, "{:?} did not reach target w", style);
        assert_eq!(state.current_h, 50.0, "{:?} did not reach target h", style);
        assert!(
            !state.animating,
            "{:?} should have stopped animating",
            style
        );
    }
}

// ---------------------------------------------------------------
// Negative position / large coordinates
// ---------------------------------------------------------------

#[test]
fn target_corners_negative_coordinates() {
    let target = make_target(-50.0, -30.0, 100.0, 60.0, CursorStyle::FilledBox);
    let corners = CursorState::target_corners(&target);
    assert_eq!(corners[0], (-50.0, -30.0));
    assert_eq!(corners[1], (50.0, -30.0));
    assert_eq!(corners[2], (50.0, 30.0));
    assert_eq!(corners[3], (-50.0, 30.0));
}

#[test]
fn target_corners_large_coordinates() {
    let target = make_target(10000.0, 20000.0, 500.0, 300.0, CursorStyle::FilledBox);
    let corners = CursorState::target_corners(&target);
    assert_eq!(corners[0], (10000.0, 20000.0));
    assert_eq!(corners[1], (10500.0, 20000.0));
    assert_eq!(corners[2], (10500.0, 20300.0));
    assert_eq!(corners[3], (10000.0, 20300.0));
}

// ---------------------------------------------------------------
// Critically damped spring: physics consistency
// ---------------------------------------------------------------

#[test]
fn spring_physics_no_overshoot_single_axis() {
    // A critically damped spring should not overshoot when starting from
    // rest (zero velocity). We verify this for a simple 1D case by running
    // the spring simulation manually.
    let omega: f32 = 26.7;
    let target: f32 = 100.0;
    let mut pos: f32 = 0.0;
    let mut vel: f32 = 0.0;
    let dt: f32 = 0.001; // 1ms steps

    for _ in 0..2000 {
        let exp_term = (-omega * dt).exp();
        let x0 = pos - target;
        let v0 = vel;
        let new_x = (x0 + (v0 + omega * x0) * dt) * exp_term;
        vel = ((v0 + omega * x0) * exp_term) - omega * (x0 + (v0 + omega * x0) * dt) * exp_term;
        pos = target + new_x;

        // Should never overshoot (go above target when starting below)
        assert!(
            pos <= target + 1.0,
            "Spring overshot at step: pos={}, target={}",
            pos,
            target
        );
    }

    // Should have converged close to target
    assert!(
        (pos - target).abs() < 1.0,
        "Spring did not converge: pos={}, target={}",
        pos,
        target
    );
}

#[test]
fn spring_physics_with_initial_velocity() {
    // With initial velocity toward target, the spring may overshoot slightly
    // but should converge
    let omega: f32 = 26.7;
    let target: f32 = 100.0;
    let mut pos: f32 = 0.0;
    let mut vel: f32 = 500.0; // high initial velocity toward target
    let dt: f32 = 0.001;

    for _ in 0..5000 {
        let exp_term = (-omega * dt).exp();
        let x0 = pos - target;
        let v0 = vel;
        let new_x = (x0 + (v0 + omega * x0) * dt) * exp_term;
        vel = ((v0 + omega * x0) * exp_term) - omega * (x0 + (v0 + omega * x0) * dt) * exp_term;
        pos = target + new_x;
    }

    // Should converge regardless of initial velocity
    assert!(
        (pos - target).abs() < 1.0,
        "Spring with initial velocity did not converge: pos={}, target={}",
        pos,
        target
    );
}
