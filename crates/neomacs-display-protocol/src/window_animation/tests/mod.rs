use super::*;
use crate::motion_spec::MotionSpec;

fn globals() -> WindowAnimationsConfig {
    WindowAnimationsConfig::default()
}

#[test]
fn nirs_stiffness_translates_to_our_angular_frequency_by_a_square_root() {
    // niri fixes mass at 1 and uses omega0 = sqrt(stiffness / mass); its
    // damping_ratio is our damping verbatim. If this drifts, every spring in
    // the config means something other than what niri's docs say it does.
    let resize = default_window_resize();
    assert!((resize.omega() - 800f32.sqrt()).abs() < 1e-4);
    assert!((resize.omega() - 28.284_271).abs() < 1e-3);

    let MotionSpec::Spring(spec) = resize.motion(globals()) else {
        panic!("window-resize is a spring");
    };
    assert!((spec.omega.get() - 28.284_271).abs() < 1e-3);
    assert!((spec.damping.get() - 1.0).abs() < 1e-6, "critically damped");
}

#[test]
fn slowdown_divides_omega_and_multiplies_a_duration() {
    // Both are exact rather than approximate. omega appears in the spring
    // solution only ever multiplied by t, so dividing it is the same motion
    // played slower -- which is the whole reason a spring can be watched at all.
    let slow = WindowAnimationsConfig {
        off: false,
        slowdown: 10.0,
    };

    let MotionSpec::Spring(spec) = default_window_resize().motion(slow) else {
        panic!("still a spring");
    };
    assert!((spec.omega.get() - 28.284_271 / 10.0).abs() < 1e-3);

    let MotionSpec::Tween(tween) = default_window_open().motion(slow) else {
        panic!("still a tween");
    };
    assert_eq!(tween.duration.get(), std::time::Duration::from_millis(1500));
}

#[test]
fn a_disabled_slot_and_the_master_switch_both_resolve_to_instant() {
    // Instant is what makes disabling free: a caller holding it builds no
    // motion and takes no offscreen composition.
    let off = WindowAnimationsConfig {
        off: true,
        slowdown: 1.0,
    };
    assert_eq!(default_window_resize().motion(off), MotionSpec::Instant);
    assert_eq!(default_window_open().motion(off), MotionSpec::Instant);
    assert_eq!(
        default_window_close().motion(globals()),
        MotionSpec::Instant,
        "window-close ships disabled"
    );
}

#[test]
fn a_zero_duration_disables_a_slot_rather_than_erroring() {
    // `:duration 0` has to mean "no motion". Storing a MotionDuration here
    // instead would reject zero at deserialize time, with a message naming a
    // Rust type.
    let slot = WindowAnimation {
        duration: std::time::Duration::ZERO,
        ..default_window_open()
    };
    assert_eq!(slot.motion(globals()), MotionSpec::Instant);
}

#[test]
fn out_of_range_taste_parameters_are_clamped_not_rejected() {
    // `apply_effects` is all-or-nothing and `neomacs-effects` is a defcustom
    // whose `:set` calls it, so rejecting one value silently reverts the user's
    // whole profile. Clamping still produces a watchable animation.
    let wild = WindowAnimation {
        damping_ratio: 1e9,
        stiffness: 0,
        ..default_window_resize()
    };
    let MotionSpec::Spring(spec) = wild.motion(globals()) else {
        panic!("clamped, not dropped");
    };
    assert!((spec.damping.get() - 10.0).abs() < 1e-6, "damping clamped");
    assert!(spec.omega.get() > 0.0, "stiffness floored to 1");

    let nan = WindowAnimation {
        damping_ratio: f32::NAN,
        ..default_window_resize()
    };
    let MotionSpec::Spring(spec) = nan.motion(globals()) else {
        panic!("NaN falls back rather than propagating");
    };
    assert!((spec.damping.get() - 1.0).abs() < 1e-6);

    let wild_globals = WindowAnimationsConfig {
        off: false,
        slowdown: f32::INFINITY,
    };
    assert!(matches!(
        default_window_open().motion(wild_globals),
        MotionSpec::Tween(_)
    ));
}

#[test]
fn ease_out_expo_reaches_exactly_one() {
    // niri's own curve is `1 - 2^(-10t)`, which is 0.999 at t = 1; it gets away
    // with that by forcing the endpoint from outside the curve. We do not, so
    // the guarded form is the only correct one here -- an animation that stops
    // 0.1% short leaves the layout permanently a fraction of a pixel off.
    assert_eq!(TransitionEasing::EaseOutExpo.apply(1.0), 1.0);
    assert_eq!(TransitionEasing::EaseOutExpo.apply(0.0), 0.0);
    assert!(TransitionEasing::EaseOutExpo.apply(0.5) > 0.9, "fast start");
}

#[test]
fn a_cubic_bezier_slot_carries_its_control_points_into_the_spec() {
    use crate::motion_spec::MotionSpec;

    // The four numbers a niri config writes as `cubic-bezier(x1, y1, x2, y2)`,
    // transcribed directly. Storing them as scalars beside a fieldless easing
    // symbol is what keeps the effect registry able to read the slot at all.
    let slot = WindowAnimation {
        easing: TransitionEasing::CubicBezier,
        bezier_x1: 0.05,
        bezier_y1: 0.7,
        bezier_x2: 0.1,
        bezier_y2: 1.0,
        ..default_window_open()
    };
    let MotionSpec::Tween(tween) = slot.motion(globals()) else {
        panic!("an easing slot builds a tween");
    };
    let bezier = tween
        .bezier
        .expect("the control points travel with the spec");
    assert!((bezier.x1 - 0.05).abs() < 1e-6);
    assert!((bezier.y2 - 1.0).abs() < 1e-6);

    // A named curve carries none, so the sampler uses the named curve.
    let named = default_window_open();
    let MotionSpec::Tween(named) = named.motion(globals()) else {
        panic!("a named easing still builds a tween");
    };
    assert!(
        named.bezier.is_none(),
        "a named easing must not smuggle in control points"
    );
}

#[test]
fn a_bezier_is_pinned_at_both_ends_and_is_monotonic_in_time() {
    use crate::motion_spec::UnitBezier;

    // libadwaita's algorithm, which niri also uses, so a curve transcribed from
    // a niri config behaves identically. Endpoints are pinned exactly rather
    // than approached, because an animation that stops a fraction short leaves
    // the layout permanently off.
    let ease = UnitBezier::new(0.25, 0.1, 0.25, 1.0);
    assert_eq!(ease.apply(0.0), 0.0);
    assert_eq!(ease.apply(1.0), 1.0);
    let mut previous = 0.0;
    for step in 1..=20 {
        let value = ease.apply(step as f32 / 20.0);
        assert!(value >= previous - 1e-4, "went backwards at {step}");
        previous = value;
    }

    // The identity curve is a straight line, which is what a slot switched to
    // `cubic-bezier` without being given points animates on.
    let linear = UnitBezier::new(0.0, 0.0, 1.0, 1.0);
    assert!((linear.apply(0.5) - 0.5).abs() < 1e-3);

    // x is clamped to the unit interval so solving for t stays unique; y is
    // not, because overshoot is the point of a curve like this one.
    let overshoot = UnitBezier::new(0.34, 1.56, 0.64, 1.0);
    assert!(
        overshoot.apply(0.5) > 1.0,
        "y must be free to exceed 1: {}",
        overshoot.apply(0.5)
    );
}
