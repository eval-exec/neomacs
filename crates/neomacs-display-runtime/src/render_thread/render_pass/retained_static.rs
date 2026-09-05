//! The retained-static composition strategy: the fast path a compositor-only
//! frame takes when nothing but the cursor has moved.
//!
//! Owns: the rule deciding whether a frame is eligible, the retained
//! cursorless scene texture and its per-generation validity, the filled-box
//! cursor mini-frames retained alongside it, and the composite that puts the
//! two back together.
//!
//! The composite is proven bit-identical to a full render
//! (`offscreen_frame::composite_matches_full_render`, and
//! `offscreen_frame::filled_box_composite_matches_full_render` for the
//! inverse-video cells), which is what lets the fast path exist at all.
//!
//! Must not: acquire or present a surface, start or advance a transition, or
//! publish a projection. It exists precisely for frames where none of those
//! happen — the caller has already established that, and the eligibility rule
//! is where that establishment is written down.

use super::FrameDrawInputs;
use crate::render_thread::frame_stats;
use crate::render_thread::frame_windows::{
    GuiFrameNativeWindowState, GuiFrameRenderState, RetainedCursorCell, RetainedStatic,
};
use crate::render_thread::state::PointerAppearanceState;
use neomacs_renderer_wgpu::{FullFrameTexture, SnapshotSize, UnpooledTexture, WgpuRenderer};

/// Whether this frame may be composed from the retained static scene:
/// the coordinator asked for a compositor-only cursor frame, and the scene is
/// eligible — no transition, no dynamic overlay, no pane motion, and every
/// cursor a clean top-layer style. An ineligible frame falls through to the
/// ordinary full render. `NEOMACS_DISABLE_RETAINED_STATIC` force-disables it.
///
/// The transition test asks whether one is *active*, not whether the frame
/// needs an offscreen: the latter is true whenever crossfade/scroll
/// transitions are merely enabled (the default), because the offscreen
/// snapshot only has to be kept current across scene commits. A
/// compositor-only frame changes no editor content, so it cannot start a
/// transition, and the "before" snapshot captured at the last scene-commit
/// full render stays correct. Gating on "needs an offscreen" would disable
/// this path entirely under the default transition policy.
///
/// A pane morph disqualifies the frame for the same reason an active
/// transition does: the retained scene is a picture of the panes where the
/// *presentation* puts them, and a moving frame needs them where the motion
/// puts them. Reproducing it would snap every pane to its destination for
/// that frame and back on the next — a blink landing mid-motion is enough to
/// show it.
pub(super) fn is_eligible(
    compositor_only_hint: bool,
    pane_placements: &[neomacs_renderer_wgpu::PaneBlit],
    render: &GuiFrameRenderState,
) -> bool {
    compositor_only_hint
        && pane_placements.is_empty()
        && !render.compositor.transitions.has_active()
        && !window_has_active_overlays(render)
        && pointer_appearance_allowed(&render.pointer_appearance)
        && !render.has_pointer_paint_damage()
        && std::env::var_os("NEOMACS_DISABLE_RETAINED_STATIC").is_none()
}

/// Compose one frame from the retained cursorless scene: blit it, draw the
/// cursor over it, and redraw each filled-box cell.
///
/// Rebuilds the retained texture first when the scene generation or the
/// window size has moved on — that rebuild is the one place this path runs
/// the glyph pipeline, once per generation rather than once per frame.
///
/// Draws onto `composition_view`, which is the frame-post source when the
/// post shader is on and the swapchain view otherwise; the caller owns that
/// choice and the frame-post step that follows.
#[allow(clippy::too_many_arguments)]
pub(super) fn draw(
    renderer: &mut WgpuRenderer,
    native: &GuiFrameNativeWindowState,
    render: &mut GuiFrameRenderState,
    composition_view: &wgpu::TextureView,
    frame: &crate::core::frame_glyphs::FrameGlyphBuffer,
    inputs: &FrameDrawInputs<'_>,
    cursor_visible: bool,
    hovered_scroll_bar: Option<neomacs_display_protocol::ScrollBarIdentity>,
) {
    let Some(native_size) = SnapshotSize::new(native.width, native.height) else {
        // wgpu rejects a zero extent, so there is no scene to retain and
        // nothing to composite from. Reaching this would mean the window was
        // asked to draw at a size `resize` already refuses to configure.
        return;
    };
    let generation = render.compositor.current_scene_generation;
    let retained_valid = matches!(
        &render.compositor.retained_static,
        Some(rs) if rs.generation == generation && rs.texture.size() == native_size
    );
    if !retained_valid {
        ensure_retained_static_texture(renderer, render, native_size);
        let retained_view = render
            .compositor
            .retained_static
            .as_ref()
            .expect("retained texture just ensured")
            .texture
            .view()
            .clone();
        // Render the full cursorless static scene into the retained
        // texture (this runs the glyph pipeline once per generation).
        super::render_frame_window_contents(
            renderer,
            native,
            render,
            &retained_view,
            frame,
            inputs,
            false,
            true,
        );
        let cells = build_filled_box_cursor_cells(
            frame,
            native.scale_factor as f32,
            &render.pointer_appearance,
        );
        if let Some(rs) = render.compositor.retained_static.as_mut() {
            rs.generation = generation;
            rs.cursor_cells = cells;
        }
        frame_stats::count(&frame_stats::RETAINED_STATIC_BUILDS);
    }
    if let Some(rs) = render.compositor.retained_static.as_ref() {
        renderer.blit_texture_to_view(
            &rs.bind_group,
            composition_view,
            native.width,
            native.height,
        );
    }
    renderer.render_cursor_only(
        composition_view,
        frame,
        inputs.present_mapping,
        cursor_visible,
        inputs.animated_cursor,
        hovered_scroll_bar,
    );
    // Filled-box cursors are inverse-video: the retained scene has the
    // character in its normal color, so each filled-box cell (box plus
    // the character in cursor_fg) is redrawn over the composite from a
    // single-glyph mini-frame, scissored to the cell. Bit-identical to
    // the full render (offscreen_frame::filled_box_composite_matches_
    // full_render).
    if cursor_visible {
        composite_filled_box_cursor_cells(
            renderer,
            render,
            composition_view,
            inputs.present_mapping,
            inputs.animated_cursor,
        );
    }
    frame_stats::count(&frame_stats::COMPOSITE_ONLY_FRAMES);
}

/// Build a single-glyph mini-frame for each filled-box cursor in the frame
/// (only the glyphs in that cursor's slot, with the frame's font tables),
/// paired with the physical-pixel scissor rect for its cell. Called once
/// per scene generation when the retained static scene is rebuilt; the
/// results are reused across cursor-only frames so the font tables are not
/// cloned every frame.
pub(super) fn build_filled_box_cursor_cells(
    frame: &crate::core::frame_glyphs::FrameGlyphBuffer,
    scale: f32,
    pointer_appearance: &PointerAppearanceState,
) -> Vec<RetainedCursorCell> {
    use crate::core::frame_glyphs::{CursorStyle, FrameGlyphBuffer};
    if !pointer_appearance_allowed(pointer_appearance) {
        return Vec::new();
    }
    let mut cells = Vec::new();
    for cursor in &frame.window_cursors {
        if !matches!(cursor.style, CursorStyle::FilledBox) {
            continue;
        }
        let mut mini = FrameGlyphBuffer::with_size(frame.width, frame.height);
        mini.presentation_id = frame.presentation_id;
        mini.clone_font_bindings_from(frame);
        mini.background = frame.background;
        mini.background_alpha = frame.background_alpha;
        // Carry the parent's default glyph metrics: a metric-less frame
        // makes render_frame_glyphs fall back to invented defaults, and a
        // default-metric change clears the whole glyph atlas — so a bare
        // mini-frame silently evicted every cached glyph on each
        // filled-box cursor composite (defeating this path's "the glyph
        // is warm in the atlas" premise) and again on the next real frame.
        mini.font_pixel_size = frame.font_pixel_size;
        mini.char_height = frame.char_height;
        for glyph in &frame.glyphs {
            if glyph.slot_id() == Some(cursor.slot_id) {
                mini.glyphs.push(glyph.clone());
            }
        }
        let mut only_cursor = cursor.clone();
        only_cursor.active = true;
        mini.window_cursors = vec![only_cursor];
        // The retained cursor cell is rendered independently from its
        // source frame. Preserve the source window's local profile so the
        // cursor-only and full-render paths resolve configuration through
        // the same local-over-global rule.
        if let Some(effects) = frame.window_cursor_effects(cursor.window_id) {
            mini.set_window_cursor_effects(cursor.window_id, effects.clone());
        }
        // The box covers the cell = the cursor rect; scissor to it in
        // physical pixels (glyph positions are logical, scaled by the
        // uniform, so the scissor rect is logical * scale).
        let scissor = (
            (cursor.x * scale).floor().max(0.0) as u32,
            (cursor.y * scale).floor().max(0.0) as u32,
            (cursor.width * scale).ceil().max(1.0) as u32,
            (cursor.height * scale).ceil().max(1.0) as u32,
        );
        cells.push(RetainedCursorCell { mini, scissor });
    }
    cells
}

/// Redraw each filled-box cursor's inverse-video cell over the composited
/// scene, from the mini-frames retained for this generation. The retained
/// static scene has the character in its normal color; a filled-box cursor
/// covers that cell with a box in the cursor color and redraws the
/// character in `cursor_fg`. Each cell renders scissored with
/// `LoadOp::Load`, so no full-frame glyph work runs and the rest of the
/// composite is preserved. The glyph is warm in the atlas from the retained
/// build, so it is a cache hit; the box color is recomputed from the frame
/// sample time, so it still cycles.
fn composite_filled_box_cursor_cells(
    renderer: &mut WgpuRenderer,
    render: &mut GuiFrameRenderState,
    surface_view: &wgpu::TextureView,
    present_mapping: neomacs_display_protocol::PresentMapping,
    animated_cursor: Option<crate::core::types::AnimatedCursor>,
) {
    let Some(atlas) = render.compositor.glyph_atlas.as_mut() else {
        return;
    };
    let Some(retained) = render.compositor.retained_static.as_ref() else {
        return;
    };
    for cell in &retained.cursor_cells {
        atlas.set_current_frame_fonts(cell.mini.font_bindings());
        renderer.render_frame_cell_loaded(
            surface_view,
            &cell.mini,
            atlas,
            present_mapping,
            true,
            animated_cursor,
            cell.scissor,
        );
    }
}

/// Whether any dynamic overlay is active. Overlays are not part of the
/// retained static scene, so their presence forces the full render path.
///
/// Idle dimming is included: it is a post-content overlay drawn *after* the
/// cursor (glyphs.rs draw_post_content_effects, after draw_cursor_layer), so
/// in a full render the cursor is dimmed too. The composite fast path draws
/// the cursor over the already-dimmed retained scene, which would leave the
/// cursor undimmed — and the retained scene's validity is not keyed on dim
/// alpha. Falling back to the full render whenever dimming is active keeps
/// both correct.
pub(super) fn window_has_active_overlays(render: &GuiFrameRenderState) -> bool {
    render.overlays.popup_menu.is_some()
        || render.overlays.tooltip.is_some()
        || render.overlays.visual_bell_start.is_some()
        || render.has_ime_preedit()
        || render.overlays.idle_dim.active
        // The FPS counter is redrawn from a live timer every frame; the
        // retained scene would freeze it, so a full render is required
        // while it is shown.
        || render.overlays.fps.enabled
}

pub(super) fn pointer_appearance_allowed(pointer_appearance: &PointerAppearanceState) -> bool {
    pointer_appearance.active().is_none()
}

/// Ensure the window's retained-static texture exists at `size` in the
/// surface format, recreating it on a size change. Leaves the generation
/// stamp untouched (the caller sets it after rendering).
///
/// Not a pool lease: `RetainedStatic` outlives the frame that drew it, so it
/// owns a [`FullFrameTexture`] and is counted through the
/// [`UnpooledTexture::RetainedStaticScene`] census instead.
///
/// [`UnpooledTexture::RetainedStaticScene`]:
///     neomacs_renderer_wgpu::UnpooledTexture::RetainedStaticScene
fn ensure_retained_static_texture(
    renderer: &WgpuRenderer,
    render: &mut GuiFrameRenderState,
    size: SnapshotSize,
) {
    let needs_new = match &render.compositor.retained_static {
        Some(rs) => rs.texture.size() != size,
        None => true,
    };
    if !needs_new {
        return;
    }
    let texture = FullFrameTexture::allocate(
        renderer.device(),
        UnpooledTexture::RetainedStaticScene,
        "retained-static-scene",
        size,
        renderer.surface_format(),
        wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
    );
    let bind_group = renderer.create_texture_bind_group(texture.view());
    render.compositor.retained_static = Some(RetainedStatic::new(texture, bind_group));
}

#[cfg(test)]
#[path = "retained_static_test.rs"]
mod tests;
