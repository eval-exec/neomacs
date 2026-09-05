//! The editor's own picture for one frame: the glyphs, the child frames
//! stacked over them, and the overlays that live in the editor's coordinate
//! space rather than the window's.
//!
//! Owns: the root glyph pass (including the pointer selection and the idle-dim
//! alpha handed to it), the child-frame stack in its render order, and the
//! content overlays — breadcrumbs, scroll indicators, window watermarks.
//!
//! Must not: draw window chrome (that is `chrome`), pick a target view, or
//! decide whether the frame happens. It draws onto the view it is handed and
//! reports continued animation the only way this pass may: by marking the
//! frame dirty, so the scheduler asks for another one.
//!
//! A child frame from a stale font-catalog generation is skipped rather than
//! drawn: its glyph indices name fonts the current atlas no longer has, and
//! drawing it would resolve them against the wrong table.

use crate::core::types::DisplayFrameId;
use crate::render_thread::frame_stats;
use crate::render_thread::frame_windows::{GuiFrameNativeWindowState, GuiFrameRenderState};
use crate::render_thread::state::ChildFrameStyle;
use neomacs_renderer_wgpu::{WgpuGlyphAtlas, WgpuRenderer};

#[allow(clippy::too_many_arguments)]
pub(super) fn render_frame_root_glyphs(
    renderer: &mut WgpuRenderer,
    render: &mut GuiFrameRenderState,
    surface_view: &wgpu::TextureView,
    frame: &crate::core::frame_glyphs::FrameGlyphBuffer,
    present_mapping: neomacs_display_protocol::PresentMapping,
    cursor_visible: bool,
    root_animated_cursor: Option<crate::core::types::AnimatedCursor>,
    bg_gradient: Option<((f32, f32, f32), (f32, f32, f32))>,
) {
    frame_stats::count(&frame_stats::ROOT_GLYPH_PASSES);
    let pointer_selection = render.pointer_selection_for(frame);
    let hovered_scroll_bar = render.hovered_scroll_bar(frame);
    if let Some(atlas) = render.compositor.glyph_atlas.as_mut() {
        atlas.set_current_frame_fonts(frame.font_bindings());
    }
    renderer.with_frame_effects(&mut render.compositor.renderer_effects, |renderer| {
        renderer.set_idle_dim_alpha(render.overlays.idle_dim.current_alpha);
        renderer.render_frame_glyphs(
            surface_view,
            frame,
            render.compositor.glyph_atlas.as_mut().unwrap(),
            present_mapping,
            cursor_visible,
            root_animated_cursor,
            hovered_scroll_bar,
            bg_gradient,
            pointer_selection,
            render.compositor.current_row_damage.as_ref(),
        );
    });
}

#[allow(clippy::too_many_arguments)]
pub(super) fn render_frame_content_overlays(
    renderer: &mut WgpuRenderer,
    native: &GuiFrameNativeWindowState,
    render: &mut GuiFrameRenderState,
    surface_view: &wgpu::TextureView,
    frame: &crate::core::frame_glyphs::FrameGlyphBuffer,
    cursor_visible: bool,
    animated_cursor: Option<crate::core::types::AnimatedCursor>,
    child_frame_style: &ChildFrameStyle,
    scroll_indicators_enabled: bool,
) {
    let pointer_appearance = render.pointer_appearance;
    renderer.with_frame_effects(&mut render.compositor.renderer_effects, |renderer| {
        for &child_id in render.compositor.child_frames.sorted_for_rendering() {
            if let Some(child_entry) = render.compositor.child_frames.frames.get(&child_id) {
                if child_entry.frame.font_catalog_generation != frame.font_catalog_generation {
                    tracing::debug!(
                        frame_id = child_id,
                        child_generation = child_entry.frame.font_catalog_generation.get(),
                        root_generation = frame.font_catalog_generation.get(),
                        "skipping retained child frame from a stale font catalog generation"
                    );
                    continue;
                }
                let neomacs_display_protocol::PresentedClip::Rect(clip_in_root) =
                    child_entry.clip_in_root
                else {
                    continue;
                };
                let pointer_selection = pointer_appearance.selection_for(&child_entry.frame);
                if let Some(atlas) = render.compositor.glyph_atlas.as_mut() {
                    atlas.set_current_frame_fonts(child_entry.frame.font_bindings());
                }
                tracing::debug!(
                    parent_frame_id = render.emacs_frame_id,
                    frame_id = child_id,
                    x = child_entry.abs_x,
                    y = child_entry.abs_y,
                    width = child_entry.frame.width,
                    height = child_entry.frame.height,
                    glyphs = child_entry.frame.glyphs.len(),
                    "child_frame_lifecycle: render_child_frame_start"
                );
                renderer.render_child_frame(
                    surface_view,
                    &child_entry.frame,
                    child_entry.abs_x,
                    child_entry.abs_y,
                    clip_in_root,
                    render.compositor.glyph_atlas.as_mut().unwrap(),
                    native.width,
                    native.height,
                    cursor_visible,
                    animated_cursor.filter(|ac| ac.frame_id == DisplayFrameId::new(child_id)),
                    child_frame_style.corner_radius,
                    child_frame_style.shadow_enabled,
                    child_frame_style.shadow_layers,
                    child_frame_style.shadow_offset,
                    child_frame_style.shadow_opacity,
                    pointer_selection,
                );
                tracing::debug!(
                    parent_frame_id = render.emacs_frame_id,
                    frame_id = child_id,
                    "child_frame_lifecycle: render_child_frame_done"
                );
            }
        }
    });
    if render.compositor.renderer_effects.needs_redraw() {
        render.mark_dirty();
    }

    if let Some(atlas) = render.compositor.glyph_atlas.as_mut() {
        atlas.set_current_frame_fonts(frame.font_bindings());
    }

    renderer.with_frame_effects(&mut render.compositor.renderer_effects, |renderer| {
        render_frame_common_overlays(
            renderer,
            surface_view,
            frame,
            render.compositor.glyph_atlas.as_mut().unwrap(),
            native.width,
            native.height,
            scroll_indicators_enabled,
        );
    });
    if render.compositor.renderer_effects.needs_redraw() {
        render.mark_dirty();
    }
}

fn render_frame_common_overlays(
    renderer: &mut WgpuRenderer,
    surface_view: &wgpu::TextureView,
    frame: &crate::core::frame_glyphs::FrameGlyphBuffer,
    glyph_atlas: &mut WgpuGlyphAtlas,
    width: u32,
    height: u32,
    scroll_indicators_enabled: bool,
) {
    if renderer.effects.breadcrumb.enabled {
        renderer.render_breadcrumbs(surface_view, frame, glyph_atlas);
    }

    if scroll_indicators_enabled {
        renderer.render_scroll_indicators(surface_view, &frame.window_infos, width, height);
    }

    if renderer.effects.window_watermark.enabled {
        renderer.render_window_watermarks(surface_view, frame, glyph_atlas);
    }
}
