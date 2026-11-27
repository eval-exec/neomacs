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
    effects.release_disabled(&config);
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

/// Every field a per-family predicate reads must be released by
/// `release_disabled` when the family is disabled, or be listed below with the
/// reason it needs no release.
///
/// This is the class guard, not a second copy of the behavioural row: adding a
/// field to a predicate and forgetting the release re-opens #857 for that one
/// family, and nothing else in the suite would notice, because that family's
/// own emit test never disables it.
#[test]
fn every_predicate_field_is_released_when_disabled() {
    /// Fields the predicates read that are not accumulated animation state.
    const EXEMPT: &[(&str, &str)] = &[(
        "has_animated_borders",
        "frame-derived: set from the borders present in each frame, not accumulated state",
    )];

    const PREDICATES: &[&str] = &[
        "pub fn cursor_effects_active(&self) -> bool {",
        "pub fn window_effects_active(&self) -> bool {",
        "pub fn text_effects_active(&self) -> bool {",
        "pub fn scroll_effects_active(&self) -> bool {",
        "pub fn decorative_effects_active(&self) -> bool {",
        "pub fn has_transient_effects(&self) -> bool {",
    ];
    const RELEASE: &str = "pub fn release_disabled(&mut self, config: &EffectsConfig) {";

    let source = include_str!("../../fx_state.rs");
    let body_of = |needle: &str| -> String {
        let start = source
            .find(needle)
            .unwrap_or_else(|| panic!("`{needle}` not found in fx_state.rs; rename or remove it"));
        let rest = &source[start + needle.len()..];
        let end = rest
            .find("\n    }\n")
            .unwrap_or_else(|| panic!("unterminated body for `{needle}`"));
        rest[..end].to_string()
    };

    /// Split a leading identifier off `s`, returning it and the remainder with
    /// at most one separating `.` removed.
    fn take_ident(s: &str) -> (Option<String>, &str) {
        let end = s
            .find(|c: char| !c.is_alphanumeric() && c != '_')
            .unwrap_or(s.len());
        let ident = s[..end].to_string();
        let after = &s[end..];
        let after = after.strip_prefix('.').unwrap_or(after);
        (if ident.is_empty() { None } else { Some(ident) }, after)
    }

    // `self.fx.<field>` or `self.fx.<field>.<part>` -> the key the release must mention.
    fn read_keys(body: &str) -> Vec<String> {
        let mut keys = Vec::new();
        let mut rest = body;
        while let Some(pos) = rest.find("self.fx.") {
            rest = &rest[pos + "self.fx.".len()..];
            let (field, after_field) = take_ident(rest);
            rest = after_field;
            let Some(field) = field else { continue };
            // A second segment only counts when it is a field access, not a call or comparison.
            let (part, _) = take_ident(rest);
            keys.push(match part {
                Some(part) => format!("{field}.{part}"),
                None => field,
            });
        }
        keys
    }

    let released = read_keys(&body_of(RELEASE));
    for predicate in PREDICATES {
        for key in read_keys(&body_of(predicate)) {
            assert!(
                released.contains(&key) || EXEMPT.iter().any(|(field, _)| *field == key.as_str()),
                "`{key}` is read by `{predicate}` but `release_disabled` never releases it. \
                 Add `release_effect_state_if_disabled!(config.<flag>, self.fx.{key}...)`, or add \
                 it to EXEMPT with the reason it holds no accumulated animation state (#857)."
            );
        }
    }
}
