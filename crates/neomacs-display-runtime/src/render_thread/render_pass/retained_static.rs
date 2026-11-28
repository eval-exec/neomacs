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

use super::{FrameDrawInputs, NativeDrawContext};
use crate::render_thread::frame_stats;
use crate::render_thread::frame_windows::{
    GuiFrameRenderState, RetainedCursorCell, RetainedStatic,
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
pub(super) fn is_scene_eligible(
    pane_placements: &[neomacs_renderer_wgpu::PaneBlit],
    render: &GuiFrameRenderState,
) -> bool {
    !render.compositor.input_scroll.active()
        && pane_placements.is_empty()
        && !render.compositor.transitions.has_active()
        // Children are painted above root glyphs and the root cursor in a
        // full render. A retained scene flattens them into one picture, so
        // redrawing the cursor afterward cannot preserve that stacking.
        && render.compositor.child_frames.is_empty()
        && !window_has_active_overlays(render)
        && pointer_appearance_allowed(&render.pointer_appearance)
        && !render.has_pointer_paint_damage()
        && std::env::var_os("NEOMACS_DISABLE_RETAINED_STATIC").is_none()
}

pub(super) fn is_eligible(
    compositor_only_hint: bool,
    pane_placements: &[neomacs_renderer_wgpu::PaneBlit],
    render: &GuiFrameRenderState,
) -> bool {
    compositor_only_hint && is_scene_eligible(pane_placements, render)
}

/// Whether a commit (`full_render`) frame can render its root scene cursorless
/// and composite the cursor on top, seeding `RetainedStatic` so subsequent
/// `CompositeOnly` frames reuse the scene without a second root glyph pass
/// (#607 Stage 2B).
pub(super) fn can_seed_on_commit(
    renderer: &WgpuRenderer,
    pane_placements: &[neomacs_renderer_wgpu::PaneBlit],
    render: &GuiFrameRenderState,
    frame: &crate::core::frame_glyphs::FrameGlyphBuffer,
    accept_derived_effects: bool,
) -> bool {
    is_scene_eligible(pane_placements, render)
        && !renderer
            .effects
            .has_enabled_effects_other_than_cursor_color_cycle()
        && !frame.window_cursors.iter().any(|cursor| {
            frame
                .effective_window_cursor_effects(cursor.window_id, &renderer.effects)
                .has_enabled_effects_other_than_cursor_color_cycle()
        })
        && !render.compositor.transitions.needs_native_conversion(
            frame,
            renderer.frame_sample(),
            &render.compositor.pending,
            &renderer.effects,
            true,
            accept_derived_effects,
        )
        && !render.compositor.renderer_effects.needs_redraw()
}

/// Whether the frame has any static chrome/overlay work beyond root glyphs
/// when `is_scene_eligible` holds (menu/tool/compact bar, custom titlebar,
/// rounded corner mask, scroll indicators, breadcrumbs, watermarks, typing
/// speed). Used to decide whether a shared `CompositionRing` root snapshot can
/// composite directly or needs a one-time blit + chrome bake on the first
/// `CompositeOnly` frame (still zero second root glyph pass).
pub(super) fn frame_has_static_chrome(
    renderer: &WgpuRenderer,
    native: NativeDrawContext<'_>,
    frame: &crate::core::frame_glyphs::FrameGlyphBuffer,
    scroll_indicators_enabled: bool,
) -> bool {
    let chrome = native.chrome;
    let has_custom_titlebar =
        !chrome.decorations_enabled && !chrome.is_fullscreen && chrome.titlebar_height > 0.0;
    let has_corner_mask =
        !chrome.decorations_enabled && !chrome.is_fullscreen && chrome.corner_radius > 0.0;
    has_custom_titlebar
        || has_corner_mask
        || !frame.frame_chrome.bands().is_empty()
        || scroll_indicators_enabled
        || renderer.effects.breadcrumb.enabled
        || renderer.effects.window_watermark.enabled
        || renderer.effects.typing_speed.enabled
}

pub(super) fn is_valid(render: &GuiFrameRenderState, size: SnapshotSize) -> bool {
    matches!(&render.compositor.retained_static,
        Some(rs) if rs.generation == render.compositor.current_scene_generation && rs.size() == size)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn composite_cursor_over_retained(
    renderer: &mut WgpuRenderer,
    retained_bind_group: &wgpu::BindGroup,
    cursor_cells: &[RetainedCursorCell],
    composition_view: &wgpu::TextureView,
    frame: &crate::core::frame_glyphs::FrameGlyphBuffer,
    present_mapping: neomacs_display_protocol::PresentMapping,
    cursor_visible: bool,
    animated_cursor: Option<crate::core::types::AnimatedCursor>,
    hovered_scroll_bar: Option<neomacs_display_protocol::ScrollBarIdentity>,
    glyph_atlas: Option<&mut neomacs_renderer_wgpu::WgpuGlyphAtlas>,
) {
    let cell_draws: Vec<neomacs_renderer_wgpu::renderer::RetainedCursorCellDraw<'_>> =
        if cursor_visible {
            cursor_cells
                .iter()
                .map(
                    |cell| neomacs_renderer_wgpu::renderer::RetainedCursorCellDraw {
                        mini: &cell.mini,
                        scissor: cell.scissor,
                    },
                )
                .collect()
        } else {
            Vec::new()
        };
    renderer.composite_retained_scene_with_cursor(
        composition_view,
        retained_bind_group,
        frame,
        present_mapping,
        cursor_visible,
        animated_cursor,
        hovered_scroll_bar,
        &cell_draws,
        glyph_atlas,
    );
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
    native: NativeDrawContext<'_>,
    render: &mut GuiFrameRenderState,
    composition_view: &wgpu::TextureView,
    frame: &crate::core::frame_glyphs::FrameGlyphBuffer,
    inputs: &FrameDrawInputs<'_>,
    cursor_visible: bool,
    hovered_scroll_bar: Option<neomacs_display_protocol::ScrollBarIdentity>,
) -> Result<(), super::surface::FrameRenderFailure> {
    let Some(native_size) = SnapshotSize::new(native.content_size.0, native.content_size.1) else {
        // wgpu rejects a zero extent, so there is no scene to retain and
        // nothing to composite from. Reaching this would mean the window was
        // asked to draw at a size `resize` already refuses to configure.
        return Ok(());
    };
    let generation = render.compositor.current_scene_generation;
    let ready = render
        .compositor
        .retained_static
        .as_ref()
        .is_some_and(|rs| rs.is_ready_for_composite(generation, native_size));
    if !ready {
        let existing = render.compositor.retained_static.take();
        match existing {
            Some(crate::render_thread::frame_compositor::RetainedStatic {
                source:
                    crate::render_thread::frame_compositor::RetainedSceneSource::SharedRoot {
                        lease,
                        includes_chrome: false,
                    },
                generation: cached_gen,
                cursor_cells,
            }) if cached_gen == generation && lease.size() == native_size => {
                // Upgrade the commit frame's cursorless root-glyph snapshot by
                // blitting it and layering static chrome on top — ZERO second
                // root glyph pass (#607 Stage 2B).
                let texture = FullFrameTexture::allocate(
                    renderer.device(),
                    UnpooledTexture::RetainedStaticScene,
                    "retained-static-scene",
                    native_size,
                    renderer.surface_format(),
                    wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                );
                let bind_group = renderer.create_texture_bind_group(texture.view());
                let retained_view = texture.view().clone();
                if let Ok(mut draw) =
                    renderer.begin_draw(neomacs_renderer_wgpu::renderer::RenderTarget::new(
                        &retained_view,
                        inputs.present_mapping.surface(),
                    ))
                {
                    draw.blit_retained(lease.bind_group());
                }
                super::chrome::render_frame_window_overlays_with_toolbar_resources(
                    renderer,
                    native,
                    render,
                    &retained_view,
                    frame,
                    false,
                    inputs.animated_cursor,
                    inputs.child_frame_style,
                    inputs.scroll_indicators_enabled,
                    inputs.toolbar,
                )?;
                let mut upgraded = RetainedStatic::new(texture, bind_group);
                upgraded.generation = generation;
                upgraded.cursor_cells = cursor_cells;
                render.compositor.retained_static = Some(upgraded);
            }
            other => {
                // Restore any existing owned texture so `ensure_retained_static_texture`
                // can reuse its allocation if the extent still matches.
                render.compositor.retained_static = other;
                ensure_retained_static_texture(renderer, render, native_size);
                let retained_view = render
                    .compositor
                    .retained_static
                    .as_ref()
                    .expect("retained texture just ensured")
                    .view()
                    .clone();
                // Render the full cursorless static scene into the retained
                // texture (fallback when not seeded by the commit frame).
                super::render_frame_window_contents(
                    renderer,
                    native,
                    render,
                    &retained_view,
                    frame,
                    inputs,
                    false,
                    true,
                )?;
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
        }
    }
    let submits_before = renderer.queue_submit_count();
    if let Some(rs) = render.compositor.retained_static.as_ref() {
        composite_cursor_over_retained(
            renderer,
            rs.bind_group(),
            &rs.cursor_cells,
            composition_view,
            frame,
            inputs.present_mapping,
            cursor_visible,
            inputs.animated_cursor,
            hovered_scroll_bar,
            render.compositor.glyph_atlas.as_mut(),
        );
    }
    frame_stats::add(
        &frame_stats::RETAINED_COMPOSITE_QUEUE_SUBMITS,
        renderer.queue_submit_count().saturating_sub(submits_before),
    );
    frame_stats::count(&frame_stats::COMPOSITE_ONLY_FRAMES);
    Ok(())
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
        let matching_info = frame
            .window_infos
            .iter()
            .find(|info| info.window_id == cursor.clip_window_id());
        let text_body = matching_info.and_then(|info| match info.geometry {
            neomacs_display_protocol::frame_glyphs::PresentedWindowGeometry::Complete {
                regions,
                ..
            } if regions.text_body.width > 0.0 && regions.text_body.height > 0.0 => {
                Some(regions.text_body)
            }
            _ => None,
        });
        let (draw_x, draw_y, draw_w, draw_h) = frame.cursor_draw_rect(
            cursor.slot_id,
            cursor.style,
            cursor.ascent,
            (cursor.x, cursor.y, cursor.width, cursor.height),
        );
        let (cx, cy, cw, ch) = if let Some(body) = text_body {
            let x0 = draw_x.max(body.x);
            let y0 = draw_y.max(body.y);
            let x1 = (draw_x + draw_w).min(body.x + body.width);
            let y1 = (draw_y + draw_h).min(body.y + body.height);
            if x1 <= x0 || y1 <= y0 {
                continue;
            }
            (x0, y0, x1 - x0, y1 - y0)
        } else {
            (draw_x, draw_y, draw_w, draw_h)
        };
        let x0 = (cx * scale).floor().max(0.0) as u32;
        let y0 = (cy * scale).floor().max(0.0) as u32;
        let x1 = ((cx + cw) * scale).ceil().max(0.0) as u32;
        let y1 = ((cy + ch) * scale).ceil().max(0.0) as u32;
        if x1 <= x0 || y1 <= y0 {
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
        if let Some(info) = matching_info {
            mini.window_infos.push(info.clone());
        }
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
        // The box covers the cell clipped to the owning window's text_body;
        // scissor to it in physical pixels (glyph positions are logical,
        // scaled by the uniform, so the scissor rect is logical * scale).
        let scissor = (x0, y0, x1 - x0, y1 - y0);
        cells.push(RetainedCursorCell { mini, scissor });
    }
    cells
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
    render.overlays.visual_bell_start.is_some()
        || render.has_ime_preedit()
        || render.overlays.idle_dim.active
        // The FPS counter is redrawn from a live timer every frame; the
        // retained scene would freeze it, so a full render is required
        // while it is shown.
        || render.overlays.fps.enabled
        // A child frame mid-lifecycle-animation (or its retained corpse)
        // must not be frozen into the retained texture at whatever alpha
        // the last build saw: the fade draws through full renders until
        // the corpse is gone and the texture rebuilds without it.
        || render.compositor.child_frames.has_animation_activity()
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
pub(super) fn ensure_retained_static_texture(
    renderer: &WgpuRenderer,
    render: &mut GuiFrameRenderState,
    size: SnapshotSize,
) {
    let needs_new = match &render.compositor.retained_static {
        Some(rs) => !rs.is_owned_at(size),
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
#[path = "retained_static/tests/retained_static_test.rs"]
mod tests;
