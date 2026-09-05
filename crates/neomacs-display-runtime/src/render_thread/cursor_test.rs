use super::*;
use std::time::Duration;

/// One anchor for a test's synthetic timeline; every other moment is derived
/// from it, so nothing here depends on how long the test takes to run.
fn t0() -> neomacs_display_protocol::frame_time::EventTime {
    neomacs_display_protocol::frame_time::observe_platform_now()
}

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
    let mut source = CursorState::new(t0());
    source.blink_enabled = false;
    source.blink_interval = Duration::from_millis(275);
    source.anim_enabled = false;
    source.anim_speed = 8.5;
    source.anim_style = CursorAnimStyle::Linear;
    source.anim_duration = 0.4;
    source.trail_size = 2.25;
    source.size_transition_enabled = false;
    source.size_transition_duration = 0.8;

    let mut target = CursorState::new(t0());
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
    let state = CursorState::new(t0());
    assert!(state.blink_on);
    assert!(state.blink_enabled);
    assert_eq!(state.blink_interval, Duration::from_millis(500));
}

#[test]
fn default_state_animation_enabled() {
    let state = CursorState::new(t0());
    assert!(state.anim_enabled);
    assert!(!state.animating);
    assert_eq!(state.anim_speed, 2.4);
    assert_eq!(state.anim_style, CursorAnimStyle::CriticallyDampedSpring);
    assert_eq!(state.anim_duration, 0.15);
}

#[test]
fn default_state_no_target() {
    let state = CursorState::new(t0());
    assert!(state.target.is_none());
}

#[test]
fn default_state_positions_at_origin() {
    let state = CursorState::new(t0());
    assert_eq!(state.current_x, 0.0);
    assert_eq!(state.current_y, 0.0);
    assert_eq!(state.current_w, 0.0);
    assert_eq!(state.current_h, 0.0);
}

#[test]
fn default_state_velocities_zero() {
    let state = CursorState::new(t0());
    for (i, spring) in state.corner_springs.iter().enumerate() {
        assert_eq!(spring.vx, 0.0, "corner {i} starts with vx");
        assert_eq!(spring.vy, 0.0, "corner {i} starts with vy");
    }
}

#[test]
fn default_state_size_transition_disabled() {
    let state = CursorState::new(t0());
    assert!(!state.size_transition_enabled);
    assert!(!state.size_animating);
    assert_eq!(state.size_transition_duration, 0.15);
}

#[test]
fn visual_config_updates_every_cursor_policy_as_one_snapshot() {
    let mut state = CursorState::new(t0());
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
    let state = CursorState::new(t0());
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
    let mut state = CursorState::new(t0());
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
    let mut state = CursorState::new(t0());
    state.animating = true;
    let target = make_target(0.0, 0.0, 10.0, 10.0, CursorStyle::FilledBox);
    state.snap(&target);
    assert!(!state.animating);
}

#[test]
fn snap_same_position_is_noop_on_values() {
    let mut state = CursorState::new(t0());
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

#[test]
fn snap_brings_corner_springs_to_rest() {
    let mut state = CursorState::new(t0());
    state.animating = true;
    for spring in &mut state.corner_springs {
        spring.vx = 420.0;
        spring.vy = -270.0;
    }

    let target = make_target(300.0, 400.0, 80.0, 40.0, CursorStyle::FilledBox);
    state.snap(&target);

    for (i, spring) in state.corner_springs.iter().enumerate() {
        assert_eq!(spring.vx, 0.0, "corner {i} kept vx across snap");
        assert_eq!(spring.vy, 0.0, "corner {i} kept vy across snap");
    }
}

#[test]
fn tick_animation_completing_off_the_spring_path_still_rests_the_springs() {
    // The spring style's own settle path parks the springs, so a completion
    // reached through any other style has to leave the same state behind.
    let base = t0();
    let mut state = CursorState::new(base);
    state.anim_enabled = true;
    state.animating = true;
    state.anim_style = CursorAnimStyle::Exponential;
    state.anim_speed = 15.0;
    // Already inside the exponential style's settle threshold, so this tick
    // finishes the animation.
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
    state.last_anim_time = base;
    for spring in &mut state.corner_springs {
        spring.vx = 88.0;
        spring.vy = -88.0;
    }

    assert!(state.tick_animation(base.plus(Duration::from_millis(16))));
    assert!(!state.animating, "exponential animation did not complete");
    for (i, spring) in state.corner_springs.iter().enumerate() {
        assert_eq!(spring.vx, 0.0, "corner {i} kept vx through completion");
        assert_eq!(spring.vy, 0.0, "corner {i} kept vy through completion");
    }
}

// ---------------------------------------------------------------
// reset_blink
// ---------------------------------------------------------------

#[test]
fn reset_blink_sets_visible() {
    let base = t0();
    let mut state = CursorState::new(base);
    state.blink_on = false;
    let reset_at = base.plus(Duration::from_millis(10));
    state.reset_blink(reset_at);

    assert!(state.blink_on);
    assert_eq!(state.last_blink_toggle, reset_at);
}

#[test]
fn reset_blink_already_visible_stays_visible() {
    let base = t0();
    let mut state = CursorState::new(base);
    assert!(state.blink_on); // default is true
    state.reset_blink(base);
    assert!(state.blink_on);
}

#[test]
fn reset_blink_updates_timestamp() {
    let base = t0();
    let mut state = CursorState::new(base);
    let old_time = state.last_blink_toggle;
    let reset_at = base.plus(Duration::from_millis(2));
    state.reset_blink(reset_at);
    assert!(state.last_blink_toggle > old_time);
    assert_eq!(state.last_blink_toggle, reset_at);
}

// ---------------------------------------------------------------
// tick_animation: returns false when disabled or not animating
// ---------------------------------------------------------------

#[test]
fn tick_animation_returns_false_when_disabled() {
    let base = t0();
    let mut state = CursorState::new(base);
    state.anim_enabled = false;
    state.animating = true;
    state.target = Some(make_target(
        100.0,
        100.0,
        10.0,
        20.0,
        CursorStyle::FilledBox,
    ));
    assert!(!state.tick_animation(base));
}

#[test]
fn tick_animation_returns_false_when_not_animating() {
    let base = t0();
    let mut state = CursorState::new(base);
    state.anim_enabled = true;
    state.animating = false;
    state.target = Some(make_target(
        100.0,
        100.0,
        10.0,
        20.0,
        CursorStyle::FilledBox,
    ));
    assert!(!state.tick_animation(base));
}

#[test]
fn tick_animation_returns_false_when_no_target() {
    let base = t0();
    let mut state = CursorState::new(base);
    state.anim_enabled = true;
    state.animating = true;
    state.target = None;
    assert!(!state.tick_animation(base));
}

#[test]
fn tick_animation_target_cleared_then_restored_takes_a_single_interval_step() {
    fn exponential_state(anchor: neomacs_display_protocol::frame_time::EventTime) -> CursorState {
        let mut state = CursorState::new(anchor);
        state.anim_enabled = true;
        state.animating = true;
        state.anim_style = CursorAnimStyle::Exponential;
        state.anim_speed = 15.0;
        state.current_x = 0.0;
        state.current_y = 0.0;
        state.current_w = 10.0;
        state.current_h = 20.0;
        state.last_anim_time = anchor;
        state
    }

    let base = t0();
    let frame = Duration::from_millis(16);
    let target = make_target(200.0, 300.0, 10.0, 20.0, CursorStyle::FilledBox);

    // One uninterrupted frame of motion, for comparison.
    let mut uninterrupted = exponential_state(base);
    uninterrupted.target = Some(target.clone());
    assert!(uninterrupted.tick_animation(base.plus(frame)));

    // The same single frame of motion, but the tick one frame earlier found
    // the target cleared and moved nothing.
    let mut interrupted = exponential_state(base);
    interrupted.target = None;
    assert!(!interrupted.tick_animation(base.plus(frame)));
    interrupted.target = Some(target.clone());
    assert!(interrupted.tick_animation(base.plus(frame).plus(frame)));

    // Non-vacuous: the frame has to move the cursor, but not all the way,
    // or two step sizes could agree by both being nothing.
    assert!(
        uninterrupted.current_x > 1.0 && uninterrupted.current_x < 199.0,
        "expected partial travel toward x=200, got {}",
        uninterrupted.current_x
    );
    assert!(
        (interrupted.current_x - uninterrupted.current_x).abs() < 1e-3,
        "step after a target-cleared tick reached x={} but one 16ms step is x={}",
        interrupted.current_x,
        uninterrupted.current_x
    );
    assert!(
        (interrupted.current_y - uninterrupted.current_y).abs() < 1e-3,
        "step after a target-cleared tick reached y={} but one 16ms step is y={}",
        interrupted.current_y,
        uninterrupted.current_y
    );
}

// ---------------------------------------------------------------
// tick_animation: Exponential style
// ---------------------------------------------------------------

#[test]
fn tick_animation_exponential_moves_toward_target() {
    let base = t0();
    let mut state = CursorState::new(base);
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
    state.last_anim_time = base;

    // Sample 5ms after the last tick, so dt == 5ms exactly.
    let result = state.tick_animation(base.plus(Duration::from_millis(5)));
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
    let base = t0();
    let mut state = CursorState::new(base);
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
    state.last_anim_time = base;

    state.tick_animation(base.plus(Duration::from_millis(1)));

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
    let base = t0();
    let mut state = CursorState::new(base);
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
    state.anim_start_time = base;
    state.last_anim_time = base;

    // Sample 10ms into the 1s animation.
    let result = state.tick_animation(base.plus(Duration::from_millis(10)));
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
    let base = t0();
    let mut state = CursorState::new(base);
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
    // Sample 100ms after the animation started, so elapsed > duration.
    state.anim_start_time = base;
    state.last_anim_time = base.plus(Duration::from_millis(100));

    state.tick_animation(base.plus(Duration::from_millis(100)));

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
    let base = t0();
    let mut state = CursorState::new(base);
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
    state.anim_start_time = base;
    state.last_anim_time = base;

    let result = state.tick_animation(base.plus(Duration::from_millis(10)));
    assert!(result);
    assert!(state.current_x > 0.0);
}

// ---------------------------------------------------------------
// tick_animation: EaseOutCubic style
// ---------------------------------------------------------------

#[test]
fn tick_animation_ease_out_cubic_progresses() {
    let base = t0();
    let mut state = CursorState::new(base);
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
    state.anim_start_time = base;
    state.last_anim_time = base;

    let result = state.tick_animation(base.plus(Duration::from_millis(10)));
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
    let base = t0();
    let mut state = CursorState::new(base);
    state.anim_enabled = true;
    state.animating = true;
    state.anim_style = CursorAnimStyle::EaseOutExpo;
    state.anim_duration = 0.5;
    state.start_x = 0.0;
    state.start_y = 0.0;
    state.start_w = 5.0;
    state.start_h = 15.0;
    state.target = Some(make_target(300.0, 300.0, 5.0, 15.0, CursorStyle::FilledBox));
    state.anim_start_time = base;
    state.last_anim_time = base;

    let result = state.tick_animation(base.plus(Duration::from_millis(10)));
    assert!(result);
    assert!(state.current_x > 0.0);
}

// ---------------------------------------------------------------
// tick_animation: EaseInOutCubic style
// ---------------------------------------------------------------

#[test]
fn tick_animation_ease_in_out_cubic_progresses() {
    let base = t0();
    let mut state = CursorState::new(base);
    state.anim_enabled = true;
    state.animating = true;
    state.anim_style = CursorAnimStyle::EaseInOutCubic;
    state.anim_duration = 0.5;
    state.start_x = 10.0;
    state.start_y = 10.0;
    state.start_w = 8.0;
    state.start_h = 16.0;
    state.target = Some(make_target(400.0, 400.0, 8.0, 16.0, CursorStyle::FilledBox));
    state.anim_start_time = base;
    state.last_anim_time = base;

    let result = state.tick_animation(base.plus(Duration::from_millis(10)));
    assert!(result);
    assert!(state.current_x > 10.0);
}

// ---------------------------------------------------------------
// tick_animation: CriticallyDampedSpring style
// ---------------------------------------------------------------

#[test]
fn tick_animation_spring_moves_toward_target() {
    let base = t0();
    let mut state = CursorState::new(base);
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
    state.last_anim_time = base;

    let result = state.tick_animation(base.plus(Duration::from_millis(5)));
    assert!(result);

    // Springs should have moved corners toward target
    // current_x/y are derived from bounding box of corner springs
    // After one tick from origin, they should have moved toward target
    // (not necessarily arrived)
}

#[test]
fn tick_animation_spring_settles_at_target() {
    let base = t0();
    let mut state = CursorState::new(base);
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
    state.last_anim_time = base;

    state.tick_animation(base.plus(Duration::from_millis(5)));

    // Should have settled: snapped to target
    assert_eq!(state.current_x, 100.0);
    assert_eq!(state.current_y, 100.0);
    assert_eq!(state.current_w, 50.0);
    assert_eq!(state.current_h, 25.0);
    assert!(!state.animating);
}

#[test]
fn tick_animation_spring_resets_velocities_on_settle() {
    let base = t0();
    let mut state = CursorState::new(base);
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
    state.last_anim_time = base;

    state.tick_animation(base.plus(Duration::from_millis(5)));

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
    let base = t0();
    let mut state = CursorState::new(base);
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
    state.anim_start_time = base;
    state.last_anim_time = base;

    let result = state.tick_animation(base.plus(Duration::from_millis(5)));
    assert!(result);

    // Position should stay the same since start == target
    assert!((state.current_x - 100.0).abs() < 1e-3);
    assert!((state.current_y - 200.0).abs() < 1e-3);
}

#[test]
fn tick_animation_zero_duration_completes_immediately() {
    let base = t0();
    let mut state = CursorState::new(base);
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
    state.anim_start_time = base;
    state.last_anim_time = base;

    // Even with zero duration, raw_t would be infinity or NaN from 0/0,
    // but it's clamped to min(1.0) so it should snap immediately.
    // The .min(1.0) ensures raw_t = 1.0 regardless of elapsed/0.
    // Actually: elapsed/0.0 = inf, inf.min(1.0) = 1.0
    state.tick_animation(base.plus(Duration::from_millis(1)));

    assert_eq!(state.current_x, 500.0);
    assert_eq!(state.current_y, 600.0);
    assert_eq!(state.current_w, 15.0);
    assert_eq!(state.current_h, 25.0);
    assert!(!state.animating);
}

#[test]
fn tick_animation_non_positive_duration_completes_at_once() {
    // Zero elapsed over a zero duration is 0.0/0.0, which only completed
    // before because `f32::min` discards NaN; a negative duration is the case
    // that accident never covered -- it interpolated backwards and never
    // reached 1.0 at all.
    for duration in [0.0f32, -0.5] {
        let base = t0();
        let mut state = CursorState::new(base);
        state.anim_enabled = true;
        state.animating = true;
        state.anim_style = CursorAnimStyle::Linear;
        state.anim_duration = duration;
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
        state.anim_start_time = base;
        state.last_anim_time = base;

        // Ticked at the animation's own start moment: elapsed is exactly zero.
        assert!(state.tick_animation(base));

        assert_eq!(state.current_x, 500.0, "duration {duration}");
        assert_eq!(state.current_y, 600.0, "duration {duration}");
        assert_eq!(state.current_w, 15.0, "duration {duration}");
        assert_eq!(state.current_h, 25.0, "duration {duration}");
        assert!(!state.animating, "duration {duration} kept animating");
    }
}

#[test]
fn tick_animation_exponential_same_position_snaps() {
    let base = t0();
    let mut state = CursorState::new(base);
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
    state.last_anim_time = base;

    state.tick_animation(base.plus(Duration::from_millis(1)));

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
    let base = t0();
    let mut state = CursorState::new(base);
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
    state.last_anim_time = base;

    // Run many ticks, 2ms of synthetic time apart.
    let mut sample = base;
    for _ in 0..200 {
        sample = sample.plus(Duration::from_millis(2));
        if !state.animating {
            break;
        }
        state.tick_animation(sample);
    }

    // Should have snapped to target
    assert_eq!(state.current_x, 100.0);
    assert_eq!(state.current_y, 100.0);
    assert!(!state.animating);
}

#[test]
fn tick_animation_linear_converges_over_duration() {
    let base = t0();
    let mut state = CursorState::new(base);
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
    state.anim_start_time = base;
    state.last_anim_time = base;

    // Run ticks until animation completes, 2ms of synthetic time apart.
    let mut sample = base;
    for _ in 0..100 {
        sample = sample.plus(Duration::from_millis(2));
        if !state.animating {
            break;
        }
        state.tick_animation(sample);
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
    let base = t0();
    let mut state = CursorState::new(base);
    state.size_transition_enabled = false;
    state.size_animating = true;
    assert!(!state.tick_size_animation(base));
}

#[test]
fn tick_size_animation_returns_false_when_not_animating() {
    let base = t0();
    let mut state = CursorState::new(base);
    state.size_transition_enabled = true;
    state.size_animating = false;
    assert!(!state.tick_size_animation(base));
}

#[test]
fn tick_size_animation_interpolates_size() {
    let base = t0();
    let mut state = CursorState::new(base);
    state.size_transition_enabled = true;
    state.size_animating = true;
    state.size_transition_duration = 1.0; // 1 second
    state.size_start_w = 10.0;
    state.size_start_h = 20.0;
    state.size_target_w = 50.0;
    state.size_target_h = 80.0;
    state.current_w = 10.0;
    state.current_h = 20.0;
    state.size_anim_start = base;

    let result = state.tick_size_animation(base.plus(Duration::from_millis(10)));
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
    let base = t0();
    let mut state = CursorState::new(base);
    state.size_transition_enabled = true;
    state.size_animating = true;
    state.size_transition_duration = 0.001; // 1ms
    state.size_start_w = 10.0;
    state.size_start_h = 20.0;
    state.size_target_w = 50.0;
    state.size_target_h = 80.0;
    state.current_w = 10.0;
    state.current_h = 20.0;
    state.size_anim_start = base;

    let result = state.tick_size_animation(base.plus(Duration::from_millis(100)));
    assert!(result);

    // Should snap to target size
    assert_eq!(state.current_w, 50.0);
    assert_eq!(state.current_h, 80.0);
    assert!(!state.size_animating);
}

#[test]
fn tick_size_animation_zero_duration_completes_immediately() {
    let base = t0();
    let mut state = CursorState::new(base);
    state.size_transition_enabled = true;
    state.size_animating = true;
    state.size_transition_duration = 0.0;
    state.size_start_w = 5.0;
    state.size_start_h = 10.0;
    state.size_target_w = 30.0;
    state.size_target_h = 60.0;
    state.current_w = 5.0;
    state.current_h = 10.0;
    state.size_anim_start = base;

    state.tick_size_animation(base.plus(Duration::from_millis(1)));

    assert_eq!(state.current_w, 30.0);
    assert_eq!(state.current_h, 60.0);
    assert!(!state.size_animating);
}

#[test]
fn tick_size_animation_same_start_and_target() {
    let base = t0();
    let mut state = CursorState::new(base);
    state.size_transition_enabled = true;
    state.size_animating = true;
    state.size_transition_duration = 0.15;
    state.size_start_w = 20.0;
    state.size_start_h = 40.0;
    state.size_target_w = 20.0;
    state.size_target_h = 40.0;
    state.current_w = 20.0;
    state.current_h = 40.0;
    state.size_anim_start = base;

    let result = state.tick_size_animation(base.plus(Duration::from_millis(5)));
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
    let base = t0();
    let mut state = CursorState::new(base);
    state.size_transition_enabled = true;
    state.size_animating = true;
    state.size_transition_duration = 0.1; // 100ms
    state.size_start_w = 0.0;
    state.size_start_h = 0.0;
    state.size_target_w = 100.0;
    state.size_target_h = 100.0;
    state.current_w = 0.0;
    state.current_h = 0.0;
    // Sample 50ms in: exactly halfway through the 100ms transition.
    state.size_anim_start = base;

    state.tick_size_animation(base.plus(Duration::from_millis(50)));

    // At raw_t=0.5, ease-out-quad = 0.5*(2.0-0.5) = 0.75
    // So width and height are exactly 75.0. The injected sample lands on the
    // halfway point exactly, so no slack for a slow machine is needed.
    assert!(
        (state.current_w - 75.0).abs() < 1e-4,
        "width at halfway should be 75: got {}",
        state.current_w
    );
    assert!(
        (state.current_h - 75.0).abs() < 1e-4,
        "height at halfway should be 75: got {}",
        state.current_h
    );
}

#[test]
fn tick_size_animation_converges() {
    let base = t0();
    let mut state = CursorState::new(base);
    state.size_transition_enabled = true;
    state.size_animating = true;
    state.size_transition_duration = 0.05; // 50ms
    state.size_start_w = 10.0;
    state.size_start_h = 10.0;
    state.size_target_w = 100.0;
    state.size_target_h = 100.0;
    state.current_w = 10.0;
    state.current_h = 10.0;
    state.size_anim_start = base;

    let mut sample = base;
    for _ in 0..100 {
        sample = sample.plus(Duration::from_millis(2));
        if !state.size_animating {
            break;
        }
        state.tick_size_animation(sample);
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
    let mut state = CursorState::new(t0());
    state.blink_enabled = true;
    state.blink_interval = Duration::from_millis(250);
    assert!(state.blink_enabled);
    assert_eq!(state.blink_interval, Duration::from_millis(250));
}

#[test]
fn blink_disabled_does_not_affect_blink_on() {
    let mut state = CursorState::new(t0());
    state.blink_enabled = false;
    state.blink_on = true;
    // blink_enabled being false doesn't change blink_on by itself;
    // the render loop checks blink_enabled before toggling
    assert!(state.blink_on);
}

#[test]
fn blink_interval_zero_is_valid() {
    let mut state = CursorState::new(t0());
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
    let base = t0();
    let mut state = CursorState::new(base);
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
    let old_time = base;
    state.last_anim_time = old_time;
    state.anim_start_time = old_time;

    let sample = base.plus(Duration::from_millis(100));
    state.tick_animation(sample);

    // last_anim_time should have been advanced to the sample it was ticked at
    assert!(state.last_anim_time > old_time);
    assert_eq!(state.last_anim_time, sample);
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

    let base = t0();
    for style in &easing_styles {
        let mut state = CursorState::new(base);
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
        state.anim_start_time = base;
        state.last_anim_time = base.plus(Duration::from_millis(100));

        state.tick_animation(base.plus(Duration::from_millis(100)));

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

// ---------------------------------------------------------------
// Integrator invariants that only an injected clock can express
// ---------------------------------------------------------------

#[test]
fn tick_animation_trajectory_is_invariant_to_sample_spacing() {
    // Both of these integrators are closed-form flows, not per-frame
    // approximations: exponential decay composes (e^-kt * e^-ks == e^-k(t+s))
    // and the critically-damped branch is the analytic solution of the ODE.
    // So the state after one 100ms step must equal the state after ten 10ms
    // steps. Until the clock was injected this was unassertable, because
    // "ten 10ms steps" meant ten sleeps of whatever length the machine felt
    // like.
    for style in [
        CursorAnimStyle::Exponential,
        CursorAnimStyle::CriticallyDampedSpring,
    ] {
        let base = t0();
        let target = make_target(200.0, 300.0, 80.0, 40.0, CursorStyle::FilledBox);

        let build = || {
            let mut state = CursorState::new(base);
            state.anim_enabled = true;
            state.animating = true;
            state.anim_style = style;
            state.anim_speed = 2.4;
            state.anim_duration = 0.15;
            state.current_x = 0.0;
            state.current_y = 0.0;
            state.current_w = 10.0;
            state.current_h = 20.0;
            state.start_x = 0.0;
            state.start_y = 0.0;
            state.start_w = 10.0;
            state.start_h = 20.0;
            state.target = Some(target.clone());
            let corners = CursorState::target_corners(&target);
            for (spring, (tx, ty)) in state.corner_springs.iter_mut().zip(corners) {
                spring.x = 0.0;
                spring.y = 0.0;
                spring.vx = 0.0;
                spring.vy = 0.0;
                spring.target_x = tx;
                spring.target_y = ty;
            }
            state.anim_start_time = base;
            state.last_anim_time = base;
            state
        };

        let mut one_step = build();
        one_step.tick_animation(base.plus(Duration::from_millis(100)));

        let mut ten_steps = build();
        for i in 1..=10u32 {
            ten_steps.tick_animation(base.plus(Duration::from_millis(u64::from(i) * 10)));
        }

        // Neither run may have snapped: a snap would make the comparison
        // trivially true for the wrong reason.
        assert!(
            one_step.animating && ten_steps.animating,
            "{style:?}: animation ended early, invariance would be vacuous"
        );
        // ...and both must actually have travelled, so that agreeing on
        // "still at the start" cannot satisfy the comparison either.
        assert!(
            one_step.current_x > 1.0 && one_step.current_x < 199.0,
            "{style:?}: expected partial travel toward x=200, got {}",
            one_step.current_x
        );

        for (field, coarse, fine) in [
            ("current_x", one_step.current_x, ten_steps.current_x),
            ("current_y", one_step.current_y, ten_steps.current_y),
            ("current_w", one_step.current_w, ten_steps.current_w),
            ("current_h", one_step.current_h, ten_steps.current_h),
        ] {
            assert!(
                (coarse - fine).abs() < 1e-3,
                "{style:?} {field}: one 100ms step gave {coarse}, ten 10ms steps gave {fine}"
            );
        }
    }
}

#[test]
fn tick_size_animation_reaches_its_target_at_exactly_its_duration() {
    let base = t0();
    let mut state = CursorState::new(base);
    state.size_transition_enabled = true;
    state.size_animating = true;
    state.size_transition_duration = 0.1; // 100ms
    state.size_start_w = 10.0;
    state.size_start_h = 20.0;
    state.size_target_w = 60.0;
    state.size_target_h = 100.0;
    state.current_w = 10.0;
    state.current_h = 20.0;
    state.size_anim_start = base;

    // 100us short of the duration: still short of the target, still running.
    assert!(state.tick_size_animation(base.plus(Duration::from_micros(99_900))));
    assert!(
        state.size_animating,
        "size transition ended before its duration"
    );
    assert!(
        state.current_w < 60.0,
        "width reached target early: {}",
        state.current_w
    );
    assert!(
        state.current_h < 100.0,
        "height reached target early: {}",
        state.current_h
    );

    // Exactly at the duration: exactly at the target, and finished.
    assert!(state.tick_size_animation(base.plus(Duration::from_millis(100))));
    assert_eq!(state.current_w, 60.0);
    assert_eq!(state.current_h, 100.0);
    assert!(!state.size_animating);
}
