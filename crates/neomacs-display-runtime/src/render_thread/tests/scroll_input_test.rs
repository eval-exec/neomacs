use super::*;
use winit::dpi::PhysicalPosition;

fn target() -> ScrollTarget {
    ScrollTarget {
        frame: 1,
        window: Some(DisplayWindowId::new(2)),
        buffer: Some(3),
        height: 64.0,
        scale: 2.0,
        x11_factor: 1.0,
        modifiers: 0,
        device: Some(DeviceId::from_raw(4)),
    }
}

#[test]
fn native_pixels_are_scaled_once_and_tiny_motion_is_conserved() {
    let mut input = ScrollInput::default();
    let mut sum = 0.0;
    for _ in 0..80 {
        if let Some(ScrollDelta::Pixels { x, y }) = input.convert(
            MouseScrollDelta::PixelDelta(PhysicalPosition::new(0.0, 0.25)),
            target(),
            TouchPhase::Moved,
        ) {
            assert_eq!(x, 0.0);
            assert!(y >= 1.0);
            sum += y;
        }
    }
    assert_eq!(sum, 10.0);
}

#[test]
fn xi2_units_use_logical_viewport_and_only_x11_factor() {
    let mut input = ScrollInput::default();
    let mut t = target();
    t.x11_factor = 0.5;
    assert_eq!(
        input.convert(
            MouseScrollDelta::ContinuousLineDelta(0.125, -2.0),
            t,
            TouchPhase::Moved
        ),
        Some(ScrollDelta::Pixels { x: 1.0, y: -16.0 })
    );
    // A native pixel distance is independent of the X11 wheel-distance policy.
    assert_eq!(
        input.convert(
            MouseScrollDelta::PixelDelta(PhysicalPosition::new(2.0, -4.0)),
            t,
            TouchPhase::Moved
        ),
        Some(ScrollDelta::Pixels { x: 1.0, y: -2.0 })
    );
}

#[test]
fn wheel_fractions_preserve_detents_and_horizontal_magnitude() {
    let mut input = ScrollInput::default();
    for _ in 0..3 {
        assert_eq!(
            input.convert(
                MouseScrollDelta::LineDelta(0.25, 0.0),
                target(),
                TouchPhase::Moved
            ),
            None
        );
    }
    assert_eq!(
        input.convert(
            MouseScrollDelta::LineDelta(0.25, 0.0),
            target(),
            TouchPhase::Moved
        ),
        Some(ScrollDelta::Lines { x: 1.0, y: 0.0 })
    );
    assert_eq!(
        input.convert(
            MouseScrollDelta::LineDelta(-3.0, 0.0),
            target(),
            TouchPhase::Moved
        ),
        Some(ScrollDelta::Lines { x: -3.0, y: 0.0 })
    );
}

#[test]
fn fractional_debt_cannot_leak_across_input_targets() {
    let original = target();
    let mut changed = Vec::new();
    let mut t = original;
    t.frame += 1;
    changed.push(t);
    let mut t = original;
    t.window = Some(DisplayWindowId::new(9));
    changed.push(t);
    let mut t = original;
    t.buffer = Some(9);
    changed.push(t);
    let mut t = original;
    t.device = Some(DeviceId::from_raw(9));
    changed.push(t);
    let mut t = original;
    t.modifiers = 1;
    changed.push(t);
    let mut t = original;
    t.scale = 3.0;
    changed.push(t);
    let mut t = original;
    t.height = 100.0;
    changed.push(t);
    for new_target in changed {
        let mut input = ScrollInput::default();
        let delta = MouseScrollDelta::PixelDelta(PhysicalPosition::new(0.0, 0.4));
        assert_eq!(input.convert(delta, original, TouchPhase::Moved), None);
        assert_eq!(input.convert(delta, original, TouchPhase::Moved), None);
        assert_eq!(input.convert(delta, new_target, TouchPhase::Moved), None);
    }
}

#[test]
fn gesture_end_and_invalid_input_discard_fractional_debt() {
    for end in [TouchPhase::Ended, TouchPhase::Cancelled] {
        let mut input = ScrollInput::default();
        let delta = MouseScrollDelta::PixelDelta(PhysicalPosition::new(0.0, 0.4));
        assert_eq!(input.convert(delta, target(), TouchPhase::Moved), None);
        assert_eq!(input.convert(delta, target(), end), None);
        assert_eq!(input.convert(delta, target(), TouchPhase::Moved), None);
    }
    let mut input = ScrollInput::default();
    assert_eq!(
        input.convert(
            MouseScrollDelta::ContinuousLineDelta(0.0, f32::NAN),
            target(),
            TouchPhase::Moved
        ),
        None
    );
    assert_eq!(
        input.convert(
            MouseScrollDelta::PixelDelta(PhysicalPosition::new(f64::INFINITY, 0.0)),
            target(),
            TouchPhase::Moved
        ),
        None
    );
    assert_eq!(
        input.convert(
            MouseScrollDelta::PixelDelta(PhysicalPosition::new(0.0, 2.0)),
            target(),
            TouchPhase::Moved
        ),
        Some(ScrollDelta::Pixels { x: 0.0, y: 1.0 })
    );
}

#[test]
fn pixel_reversals_conserve_net_displacement() {
    let mut input = ScrollInput::default();
    let mut actual = 0.0;
    for y in [0.25, 0.25, 0.25, -0.25, -0.25, -0.25, 2.0, -2.0] {
        if let Some(ScrollDelta::Pixels { y, .. }) = input.convert(
            MouseScrollDelta::PixelDelta(PhysicalPosition::new(0.0, y)),
            target(),
            TouchPhase::Moved,
        ) {
            actual += y;
        }
    }
    assert_eq!(actual, 0.0);
}
