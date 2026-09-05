//! Shared types and utilities for visual effect modules.
//!
//! Contains the EffectCtx struct and helper functions used by
//! cursor_effects, window_effects, and pattern_effects.

use super::super::vertex::RectVertex;
use crate::effect_config::EffectsConfig;
use neomacs_display_protocol::frame_glyphs::FrameGlyphBuffer;
use neomacs_display_protocol::frame_time::{EventTime, FrameSample};
use neomacs_display_protocol::types::{AnimatedCursor, Color};

/// Shared context for effect vertex computation.
/// Holds immutable references to data needed by most effects.
pub(super) struct EffectCtx<'a> {
    pub effects: &'a EffectsConfig,
    pub frame_glyphs: &'a FrameGlyphBuffer,
    pub animated_cursor: &'a Option<AnimatedCursor>,
    pub cursor_visible: bool,
    #[allow(dead_code)]
    pub mouse_pos: (f32, f32),
    pub surface_width: u32,
    pub surface_height: u32,
    pub aurora_start: EventTime,
    /// The one time sample every effect in this frame dates itself to.
    ///
    /// Effects age their anchors against this instead of reading the clock,
    /// so two effects drawn in the same frame agree on how old everything is.
    /// Phase-bearing effects use `since_at_presentation`, because the phase a
    /// viewer perceives is the one on screen, not the one at build time.
    pub frame_sample: FrameSample,
    /// Monotonic per-frame counter, the entropy input for
    /// [`effect_entity_seed`]. Effects that need pseudo-random per-entity
    /// values mix this with an entity index rather than reading a clock.
    pub frame_seq: u64,
    pub scale_factor: f32,
    /// Logical frame width (frame_glyphs.width or surface_width/scale_factor)
    pub logical_w: f32,
    /// Logical frame height
    pub logical_h: f32,
    /// Renderer physical width as float
    pub renderer_width: f32,
    /// Renderer physical height as float
    pub renderer_height: f32,
}

/// Deterministic pseudo-random seed for one entity within one frame.
///
/// Effects that scatter particles, rain columns, or sparkles need a different
/// value per entity that also changes from frame to frame. They used to get it
/// by reading the clock (`Instant::now().elapsed().subsec_nanos()`), which is
/// entropy dressed up as time: replacing that read with the frame's time
/// sample would hand every entity in the frame the same constant and collapse
/// the scatter to a single value.
///
/// Mixing the frame counter with the entity index instead keeps both
/// properties and adds a third: the result is reproducible, so a frame can be
/// replayed. `frame_seq` comes from [`EffectCtx::frame_seq`].
#[must_use]
pub(super) fn effect_entity_seed(frame_seq: u64, entity_index: u64) -> u64 {
    // splitmix64 finalizer over the two mixed inputs: cheap, and it avalanches
    // the low bits so callers taking `(h >> 16) & 0xFFFF` still see variation
    // between adjacent entity indices.
    let mut z = frame_seq
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add(entity_index.wrapping_mul(0xBF58_476D_1CE4_E5B9));
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Free-running animation phase in `[0.0, 1.0)` for an ambient effect.
///
/// A pulse or a breath has no trigger event to date itself to, so instead of
/// ageing an entry's `started` it ages the session-start anchor already on the
/// context — [`EffectCtx::aurora_start`], the render epoch minted once at
/// renderer construction — against this frame's sample. Reading it at
/// presentation rather than at frame time is the same rule every other
/// phase-bearing effect follows: the phase a viewer perceives is the one that
/// is on screen when the pixels land.
///
/// `cycle_ms` is the effect's configured period, so one full cycle of the
/// returned phase takes exactly that long. A zero period names no cycle at
/// all, so it holds the phase still rather than dividing by zero and painting
/// a NaN alpha.
#[must_use]
pub(super) fn ambient_cycle_phase(ctx: &EffectCtx, cycle_ms: u32) -> f64 {
    let cycle = f64::from(cycle_ms) / 1000.0;
    if cycle <= 0.0 {
        return 0.0;
    }
    let elapsed = ctx
        .frame_sample
        .since_at_presentation(ctx.aurora_start)
        .as_secs_f64();
    (elapsed % cycle) / cycle
}

/// Push a rectangle (6 vertices = 2 triangles) into a vertex buffer.
/// Free function equivalent of WgpuRenderer::add_rect.
pub(super) fn push_rect(
    vertices: &mut Vec<RectVertex>,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    color: &Color,
) {
    let color_arr = [color.r, color.g, color.b, color.a];
    let x0 = x;
    let y0 = y;
    let x1 = x + width;
    let y1 = y + height;

    vertices.push(RectVertex {
        position: [x0, y0],
        color: color_arr,
    });
    vertices.push(RectVertex {
        position: [x1, y0],
        color: color_arr,
    });
    vertices.push(RectVertex {
        position: [x0, y1],
        color: color_arr,
    });
    vertices.push(RectVertex {
        position: [x1, y0],
        color: color_arr,
    });
    vertices.push(RectVertex {
        position: [x1, y1],
        color: color_arr,
    });
    vertices.push(RectVertex {
        position: [x0, y1],
        color: color_arr,
    });
}

/// Find the active cursor position from animated cursor or frame glyphs.
/// Returns (x, y, width, height) of the active (non-hollow) cursor.
pub(super) fn find_cursor_pos(
    animated_cursor: &Option<AnimatedCursor>,
    frame_glyphs: &FrameGlyphBuffer,
) -> Option<(f32, f32, f32, f32)> {
    if let Some(anim) = animated_cursor {
        return Some((anim.x, anim.y, anim.width, anim.height));
    }
    frame_glyphs
        .active_cursor()
        .map(|cursor| (cursor.x, cursor.y, cursor.width, cursor.height))
}

#[cfg(test)]
#[path = "effect_common_test.rs"]
mod tests;
