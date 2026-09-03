//! Shared types and utilities for visual effect modules.
//!
//! Contains the EffectCtx struct and helper functions used by
//! cursor_effects, window_effects, and pattern_effects.

use super::super::vertex::RectVertex;
use crate::effect_config::EffectsConfig;
use neomacs_display_protocol::frame_glyphs::FrameGlyphBuffer;
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
    pub aurora_start: neomacs_host_runtime::time::Instant,
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
