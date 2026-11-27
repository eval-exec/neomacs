//! Regression rows for #857: disabling a visual effect mid-animation must
//! release that effect's accumulated state.
//!
//! The frame scheduler polls effect *state* through `needs_redraw` and the six
//! per-family predicates, and every stateful `emit_*` returns early when its
//! config is disabled -- before it would have drained anything. So a disable
//! that lands while a state is populated freezes it, the poll keeps reporting
//! the family as animating, and the render loop keeps submitting frames at the
//! animation rate (the display rate) with nothing left to draw. That is
//! #857: ~120 presents/second on an idle session whose effects were switched
//! off.

use super::super::effect_common::EffectCtx;
use super::*;
use crate::effect_config::EffectsConfig;
use crate::renderer::cursor_effects::emit_cursor_comet;
use neomacs_display_protocol::frame_glyphs::FrameGlyphBuffer;
use neomacs_display_protocol::frame_time::{FrameSample, observe_platform_now};
use neomacs_display_protocol::types::{AnimatedCursor, DisplayFrameId, DisplayWindowId};
use std::time::Duration;

fn test_cursor(x: f32, y: f32) -> AnimatedCursor {
    AnimatedCursor {
        window_id: DisplayWindowId::new(1),
        x,
        y,
        width: 10.0,
        height: 20.0,
        corners: None,
        frame_id: DisplayFrameId::new(0),
    }
}

fn test_ctx<'a>(
    effects: &'a EffectsConfig,
    frame_glyphs: &'a FrameGlyphBuffer,
    animated_cursor: &'a Option<AnimatedCursor>,
) -> EffectCtx<'a> {
    EffectCtx {
        effects,
        frame_glyphs,
        animated_cursor,
        cursor_visible: true,
        surface_width: 800,
        surface_height: 600,
        aurora_start: observe_platform_now(),
        frame_sample: FrameSample::new(observe_platform_now(), Duration::from_millis(16)),
        frame_seq: 1,
        scale_factor: 1.0,
        logical_w: 800.0,
        logical_h: 600.0,
        renderer_width: 800.0,
        renderer_height: 600.0,
    }
}

/// The reported shape: `cursor-comet` accumulates trail positions and the
/// user disables it before the fade completes.
#[test]
fn disabled_cursor_comet_stops_demanding_frames() {
    let mut config = EffectsConfig::default();
    config.cursor_comet.enabled = true;
    config.cursor_comet.trail_length = 5;
    config.cursor_comet.fade_ms = 250;

    let frame_glyphs = FrameGlyphBuffer::default();
    let cursor = Some(test_cursor(100.0, 100.0));
    let ctx = test_ctx(&config, &frame_glyphs, &cursor);

    let mut effects = RendererFrameEffects::default();
    let _ = emit_cursor_comet(&ctx, &mut effects.fx.cursor_comet.positions);
    assert!(
        effects.cursor_effects_active(),
        "an enabled comet frame must record a trail position"
    );
    assert!(effects.needs_redraw());

    // The user disables the effect mid-fade. The next presented frame runs the
    // same sequence the draw pass does: release disabled state, then emit.
    config.cursor_comet.enabled = false;
    let ctx = test_ctx(&config, &frame_glyphs, &cursor);
    let _ = emit_cursor_comet(&ctx, &mut effects.fx.cursor_comet.positions);

    assert!(
        !effects.cursor_effects_active(),
        "a disabled effect must not keep its accumulated trail state"
    );
    assert!(
        !effects.needs_redraw(),
        "a disabled effect must not keep demanding frames (#857)"
    );
}
