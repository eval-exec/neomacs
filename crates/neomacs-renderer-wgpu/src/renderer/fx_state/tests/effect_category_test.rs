use super::*;
use std::time::Duration;

#[test]
fn default_effects_are_quiet() {
    let fx = RendererFrameEffects::default();
    assert!(!fx.needs_redraw());
    assert!(!fx.cursor_effects_active());
    assert!(!fx.window_effects_active());
    assert!(!fx.text_effects_active());
    assert!(!fx.scroll_effects_active());
    assert!(!fx.decorative_effects_active());
    assert!(!fx.transient_effects_active());
}

#[test]
fn each_category_drives_needs_redraw() {
    let now = observe_platform_now();

    // Cursor: wake.
    let mut fx = RendererFrameEffects::default();
    fx.fx.cursor_wake.started = Some(now);
    assert!(fx.cursor_effects_active() && fx.needs_redraw());

    // Window: animated borders.
    let mut fx = RendererFrameEffects::default();
    fx.fx.has_animated_borders = true;
    assert!(fx.window_effects_active() && fx.needs_redraw());

    // Text: typing ripple.
    let mut fx = RendererFrameEffects::default();
    fx.fx.typing_ripple.active.push((0.0, 0.0, now));
    assert!(fx.text_effects_active() && fx.needs_redraw());

    // Scroll: velocity fade.
    let mut fx = RendererFrameEffects::default();
    fx.fx.scroll_velocity.fades.push(ScrollVelocityFadeEntry {
        window_id: 1,
        bounds: neomacs_display_protocol::types::Rect {
            x: 0.0,
            y: 0.0,
            width: 1.0,
            height: 1.0,
        },
        intensity: 1.0,
        started: now,
        duration: Duration::from_millis(100),
    });
    assert!(fx.scroll_effects_active() && fx.needs_redraw());

    // Decorative: ripple ring.
    let mut fx = RendererFrameEffects::default();
    fx.fx.ripple_ring.start = Some(now);
    assert!(fx.decorative_effects_active() && fx.needs_redraw());

    // Transient: error pulse.
    let mut fx = RendererFrameEffects::default();
    fx.fx.error_pulse.started = Some(now);
    assert!(fx.transient_effects_active() && fx.needs_redraw());
}
