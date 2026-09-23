use super::*;
use std::thread::sleep;
use std::time::Duration;

#[test]
fn test_easing_linear() {
    let easing = Easing::Linear;
    assert!((easing.apply(0.0) - 0.0).abs() < 0.001);
    assert!((easing.apply(0.5) - 0.5).abs() < 0.001);
    assert!((easing.apply(1.0) - 1.0).abs() < 0.001);
}

#[test]
fn test_easing_ease_in() {
    let easing = Easing::EaseIn;
    assert!((easing.apply(0.0) - 0.0).abs() < 0.001);
    assert!((easing.apply(0.5) - 0.25).abs() < 0.001); // 0.5^2 = 0.25
    assert!((easing.apply(1.0) - 1.0).abs() < 0.001);
}

#[test]
fn test_easing_ease_out() {
    let easing = Easing::EaseOut;
    assert!((easing.apply(0.0) - 0.0).abs() < 0.001);
    assert!((easing.apply(0.5) - 0.75).abs() < 0.001); // 1 - (1-0.5)^2 = 0.75
    assert!((easing.apply(1.0) - 1.0).abs() < 0.001);
}

#[test]
fn test_easing_ease_in_out() {
    let easing = Easing::EaseInOut;
    assert!((easing.apply(0.0) - 0.0).abs() < 0.001);
    assert!((easing.apply(0.5) - 0.5).abs() < 0.001);
    assert!((easing.apply(1.0) - 1.0).abs() < 0.001);
}

#[test]
fn test_easing_ease_out_bounce() {
    let easing = Easing::EaseOutBounce;
    assert!((easing.apply(0.0) - 0.0).abs() < 0.001);
    assert!((easing.apply(1.0) - 1.0).abs() < 0.001);
    // Bounce should reach values close to or above target at end
    assert!(easing.apply(0.9) > 0.8);
}

#[test]
fn test_animation_engine_basic() {
    let mut engine = AnimationEngine::new();

    let id = engine.animate(
        AnimationTarget::Window(1),
        AnimatedProperty::X,
        0.0,
        100.0,
        Duration::from_millis(100),
        Easing::Linear,
    );

    assert!(engine.has_animations());
    assert!(
        engine
            .get_value(AnimationTarget::Window(1), AnimatedProperty::X)
            .is_some()
    );

    engine.cancel(id);
    assert!(!engine.has_animations());
}

#[test]
fn test_animation_completion() {
    let mut engine = AnimationEngine::new();

    engine.animate(
        AnimationTarget::Cursor,
        AnimatedProperty::Opacity,
        0.0,
        1.0,
        Duration::from_millis(10),
        Easing::Linear,
    );

    assert!(engine.has_animations());

    // Wait for animation to complete
    sleep(Duration::from_millis(20));

    engine.tick();
    assert!(!engine.has_animations());
}

#[test]
fn test_animation_replaces_same_target_property() {
    let mut engine = AnimationEngine::new();

    engine.animate(
        AnimationTarget::Window(1),
        AnimatedProperty::X,
        0.0,
        100.0,
        Duration::from_secs(1),
        Easing::Linear,
    );

    engine.animate(
        AnimationTarget::Window(1),
        AnimatedProperty::X,
        50.0,
        150.0,
        Duration::from_secs(1),
        Easing::Linear,
    );

    // Should only have one animation
    assert_eq!(engine.animations.len(), 1);
    // The new animation should start from 50
    assert!(
        (engine
            .get_value(AnimationTarget::Window(1), AnimatedProperty::X)
            .unwrap()
            - 50.0)
            .abs()
            < 1.0
    );
}
