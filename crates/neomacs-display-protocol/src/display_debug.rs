//! Last-presented render-thread state for on-demand display debug bundles
//! (`M-x neomacs-dump-display-state` / `neomacs--frame-snapshot`
//! `'display-debug`).
//!
//! Ownership boundary (issue #855):
//! - Layout/draw cursor geometry (cursor layout rect, slot, charpos, drawn
//!   rect, row metrics, slot glyph, face/font, draw-time scroll state) is owned
//!   by the `neomacs::cursor_geometry` tracing target and consumed verbatim by
//!   the bundle without re-derivation.
//! - This module owns ONLY the last-presented render-thread state that neither
//!   `neomacs::cursor_geometry` nor `frame-snapshot` carries: monotonic present
//!   sequence, composition strategy (`full_render` / `retained_static` /
//!   `retained_scroll` / `not_yet_presented`), surface scale/dimensions, cursor
//!   blink/visibility/inverse-video state, smooth-cursor animation override,
//!   and active input-scroll prediction offset.
//! - Recording on the hot path uses one inline `Copy` value
//!   ([`LastPresentedFrameSummary`]) with zero per-frame heap allocation.

use crate::frame_chrome::PresentationId;
use crate::frame_glyphs::FrameGlyphBuffer;

/// Schema version for the top-level display debug bundle JSON.
pub const DISPLAY_DEBUG_SCHEMA_VERSION: u32 = 1;

/// Which composition/render strategy painted the last presented frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PresentedRenderPath {
    #[default]
    NotYetPresented,
    FullRender,
    RetainedStatic,
    RetainedScroll,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DisplayDebugRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl DisplayDebugRect {
    pub const fn from_array(rect: [f32; 4]) -> Self {
        Self {
            x: rect[0],
            y: rect[1],
            width: rect[2],
            height: rect[3],
        }
    }
}

/// Inline `Copy` summary of the last-presented render-thread state recorded at
/// `queue().present(...)`. Zero heap allocation per frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LastPresentedFrameSummary {
    pub presentation_id: PresentationId,
    pub frame_seq: u64,
    pub render_path: PresentedRenderPath,
    pub scale_factor: f64,
    pub surface_width: u32,
    pub surface_height: u32,
    pub logical_width: f32,
    pub logical_height: f32,
    pub has_active_cursor: bool,
    pub cursor_visible: bool,
    pub blink_enabled: bool,
    pub inverse_video_active: bool,
    pub is_animating: bool,
    /// Non-`None` only when smooth-cursor animation supplied a GPU override
    /// rectangle distinct from the static cursor placement.
    pub animated_cursor_rect: Option<DisplayDebugRect>,
    pub input_scroll_active: bool,
    pub input_scroll_offset: f32,
}

/// On-demand render-thread snapshot of one live GUI frame window's
/// last-presented state. Joins with `neomacs::cursor_geometry` and
/// `frame_snapshot` via `presentation_id`.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PresentedFrameDebugSnapshot {
    pub frame_id: u64,
    pub is_primary: bool,
    pub presentation_id: u64,
    pub frame_seq: u64,
    pub render_path: PresentedRenderPath,
    pub scale_factor: f64,
    pub surface_px: [u32; 2],
    pub logical_px: [f32; 2],
    pub has_active_cursor: bool,
    pub cursor_visible: bool,
    pub blink_enabled: bool,
    pub inverse_video_active: bool,
    pub is_animating: bool,
    pub animated_cursor_rect: Option<DisplayDebugRect>,
    pub input_scroll_active: bool,
    pub input_scroll_offset: f32,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DisplayDebugVersion {
    pub package_version: String,
    pub git_commit: String,
    pub platform: String,
    pub window_system: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DisplayDebugFlags {
    pub presentation_lagging: bool,
    pub cursor_hidden_or_blinked_off: bool,
    pub cursor_animating: bool,
    pub input_scroll_active: bool,
}

impl DisplayDebugFlags {
    pub fn compute(
        selected_frame_id: Option<u64>,
        last_interaction_presentation: u64,
        presented_frames: &[PresentedFrameDebugSnapshot],
    ) -> Self {
        let selected_frame = selected_frame_id
            .and_then(|id| presented_frames.iter().find(|f| f.frame_id == id))
            .or_else(|| presented_frames.iter().find(|f| f.is_primary))
            .or_else(|| presented_frames.first());
        let Some(frame) = selected_frame else {
            return Self::default();
        };
        let presentation_lagging = last_interaction_presentation > 0
            && frame.presentation_id < last_interaction_presentation;
        let cursor_hidden_or_blinked_off = !frame.has_active_cursor || !frame.cursor_visible;
        Self {
            presentation_lagging,
            cursor_hidden_or_blinked_off,
            cursor_animating: frame.is_animating,
            input_scroll_active: frame.input_scroll_active,
        }
    }
}

impl PresentedFrameDebugSnapshot {
    /// Build a last-presented diagnostic snapshot from a live frame buffer and
    /// the render thread's last-presented summary. Runs only on demand.
    #[allow(clippy::too_many_arguments)]
    pub fn from_frame_buffer(
        frame_id: u64,
        is_primary: bool,
        frame: &FrameGlyphBuffer,
        last_present: Option<LastPresentedFrameSummary>,
        fallback_scale_factor: f64,
        fallback_surface_px: [u32; 2],
        fallback_cursor_visible: bool,
        fallback_blink_enabled: bool,
    ) -> Self {
        if let Some(summary) = last_present {
            Self {
                frame_id,
                is_primary,
                presentation_id: summary.presentation_id.get(),
                frame_seq: summary.frame_seq,
                render_path: summary.render_path,
                scale_factor: summary.scale_factor,
                surface_px: [summary.surface_width, summary.surface_height],
                logical_px: [summary.logical_width, summary.logical_height],
                has_active_cursor: summary.has_active_cursor,
                cursor_visible: summary.cursor_visible,
                blink_enabled: summary.blink_enabled,
                inverse_video_active: summary.inverse_video_active,
                is_animating: summary.is_animating,
                animated_cursor_rect: summary.animated_cursor_rect,
                input_scroll_active: summary.input_scroll_active,
                input_scroll_offset: summary.input_scroll_offset,
            }
        } else {
            let active = frame.active_cursor();
            Self {
                frame_id,
                is_primary,
                presentation_id: frame.presentation_id.get(),
                frame_seq: 0,
                render_path: PresentedRenderPath::NotYetPresented,
                scale_factor: fallback_scale_factor,
                surface_px: fallback_surface_px,
                logical_px: [frame.width, frame.height],
                has_active_cursor: active.is_some(),
                cursor_visible: fallback_cursor_visible,
                blink_enabled: fallback_blink_enabled,
                inverse_video_active: fallback_cursor_visible
                    && active.is_some_and(|c| matches!(c.style, crate::CursorStyle::FilledBox)),
                is_animating: false,
                animated_cursor_rect: None,
                input_scroll_active: false,
                input_scroll_offset: 0.0,
            }
        }
    }
}

#[cfg(test)]
#[path = "display_debug/tests/display_debug_test.rs"]
mod tests;
