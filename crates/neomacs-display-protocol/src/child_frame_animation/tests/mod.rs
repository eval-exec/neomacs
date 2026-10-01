use super::*;
use crate::VisualConfig;
use crate::effect_command::{EffectOperation, EffectValue};
use crate::motion_spec::MotionSpec;

fn globals() -> ChildFrameAnimationsConfig {
    ChildFrameAnimationsConfig::default()
}

#[test]
fn an_easing_slot_builds_a_tween_carrying_the_slide_distance() {
    let MotionSpec::Tween(tween) = default_child_frame_open().motion(globals()) else {
        panic!("child-frame-open is an easing");
    };
    // Duration::mul_f32 rounds; compare in seconds with slack rather than
    // pinning the exact nanos the multiply happens to produce.
    assert!((tween.duration.get().as_secs_f32() - 0.15).abs() < 1e-3);
    assert_eq!(tween.easing, TransitionEasing::EaseOutExpo);
    assert!(tween.bezier.is_none(), "a named easing carries no points");

    // The slide distance is the one property this module adds over the
    // window-animation slots; it travels on the slot, not the spec, because
    // the fade and the displacement share one curve by design.
    assert!((default_child_frame_open().slide_pixels - 8.0).abs() < 1e-6);
    assert!((default_child_frame_close().slide_pixels).abs() < 1e-6);
}

#[test]
fn a_spring_slot_translates_stiffness_the_same_way_window_animations_do() {
    // The niri conversion is restated here rather than shared; this test is
    // what keeps the copy honest -- if the two ever diverge, one of them
    // means something other than what a niri config says.
    let movement = ChildFrameAnimation {
        enabled: true,
        ..default_child_frame_movement()
    };
    assert!((movement.omega() - 800f32.sqrt()).abs() < 1e-4);

    let MotionSpec::Spring(spec) = movement.motion(globals()) else {
        panic!("child-frame-movement is a spring");
    };
    assert!((spec.omega.get() - 28.284_271).abs() < 1e-3);
    assert!((spec.damping.get() - 1.0).abs() < 1e-6, "critically damped");
}

#[test]
fn a_disabled_slot_and_the_master_switch_both_resolve_to_instant() {
    let off = ChildFrameAnimationsConfig {
        off: true,
        slowdown: 1.0,
    };
    assert_eq!(default_child_frame_open().motion(off), MotionSpec::Instant);
    assert_eq!(default_child_frame_close().motion(off), MotionSpec::Instant);

    // Movement and resize ship off: re-anchoring must land exactly where
    // Emacs put the frame until the user opts into the drift.
    assert_eq!(
        default_child_frame_movement().motion(globals()),
        MotionSpec::Instant,
        "child-frame-movement ships disabled"
    );
    assert_eq!(
        default_child_frame_resize().motion(globals()),
        MotionSpec::Instant,
        "child-frame-resize ships disabled"
    );
}

#[test]
fn a_zero_duration_disables_a_slot_rather_than_erroring() {
    let slot = ChildFrameAnimation {
        duration: std::time::Duration::ZERO,
        ..default_child_frame_open()
    };
    assert_eq!(slot.motion(globals()), MotionSpec::Instant);
}

#[test]
fn out_of_range_taste_parameters_are_clamped_not_rejected() {
    let wild = ChildFrameAnimation {
        enabled: true,
        damping_ratio: 1e9,
        stiffness: 0,
        ..default_child_frame_movement()
    };
    let MotionSpec::Spring(spec) = wild.motion(globals()) else {
        panic!("clamped, not dropped");
    };
    assert!((spec.damping.get() - 10.0).abs() < 1e-6, "damping clamped");
    assert!(spec.omega.get() > 0.0, "stiffness floored to 1");

    let wild_globals = ChildFrameAnimationsConfig {
        off: false,
        slowdown: f32::NEG_INFINITY,
    };
    assert!(matches!(
        default_child_frame_open().motion(wild_globals),
        MotionSpec::Tween(_)
    ));
}

#[test]
fn a_cubic_bezier_slot_carries_its_control_points_into_the_spec() {
    let slot = ChildFrameAnimation {
        easing: TransitionEasing::CubicBezier,
        bezier_x1: 0.05,
        bezier_y1: 0.7,
        bezier_x2: 0.1,
        bezier_y2: 1.0,
        ..default_child_frame_open()
    };
    let MotionSpec::Tween(tween) = slot.motion(globals()) else {
        panic!("an easing slot builds a tween");
    };
    let bezier = tween
        .bezier
        .expect("the control points travel with the spec");
    assert!((bezier.x1 - 0.05).abs() < 1e-6);
    assert!((bezier.y2 - 1.0).abs() < 1e-6);
}

#[test]
fn the_registry_carries_every_slot_and_the_slide_property() {
    // `neomacs-effect-set` validates a property key against the value already
    // stored, so a property missing from the serialized shape makes switching
    // it from Lisp unreachable. Walking every name of the config struct and
    // round-tripping one value through the registry proves the opposite.
    let config = VisualConfig::default();
    for effect in [
        "child-frame-animations",
        "child-frame-open",
        "child-frame-close",
        "child-frame-movement",
        "child-frame-resize",
    ] {
        let values = config
            .effect_values(effect)
            .unwrap_or_else(|error| panic!("{effect} must publish its values: {error}"));
        // The globals slot publishes its master switch rather than an
        // `enabled`; every per-frame slot exposes `enabled`.
        let expected = if effect == "child-frame-animations" {
            "off"
        } else {
            "enabled"
        };
        assert!(
            values.iter().any(|(property, _)| property == expected),
            "{effect} must expose `{expected}`"
        );
    }
    let open = config
        .effect_values("child-frame-open")
        .expect("child-frame-open publishes");
    assert!(
        open.iter().any(|(property, _)| property == "slide-pixels"),
        "the slide distance is a registry property"
    );

    let pushed = config
        .apply_effects(&[EffectOperation::set(
            "child-frame-open",
            [
                ("slide-pixels", EffectValue::Number(12.0)),
                ("easing", EffectValue::Symbol("ease-out-quad".into())),
            ],
        )])
        .expect("the open slot accepts its own properties");
    assert!((pushed.child_frame_open.slide_pixels - 12.0).abs() < 1e-6);
    assert_eq!(
        pushed.child_frame_open.easing,
        TransitionEasing::EaseOutQuad
    );
}
