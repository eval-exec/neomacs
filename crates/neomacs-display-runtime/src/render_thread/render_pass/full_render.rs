//! The two strategies that actually run the glyph pipeline for a frame: draw
//! it into a composition-ring slot and place that slot, or draw it straight
//! onto the target view.
//!
//! Owns: the choice of *what happens to the composed picture* once it exists —
//! a whole-frame blit, a per-pane placement driven by the motion, the
//! transition draw layered over it — and the transition detection both
//! strategies run.
//!
//! Must not: decide which strategy applies, lease the slot, acquire or present
//! a surface. The draw order does all of that: a ring slot was already
//! requested and either granted or refused, and `None` means the frame is
//! degrading to the direct strategy — one frame without transitions or pane
//! motion, which is a real GPU-pressure outcome and not an error path.
//!
//! The two strategies differ in more than their target. The ring strategy
//! draws content *without* overlays, because the chrome has to land on the
//! placed result rather than inside the picture being placed; the direct
//! strategy draws content and overlays in one go. That asymmetry is the whole
//! reason `include_overlays` exists.

use super::{FrameDrawInputs, NativeDrawContext};
use crate::render_thread::frame_windows::GuiFrameRenderState;
use crate::render_thread::transitions::{detect_frame_transitions, render_frame_transitions};
use neomacs_renderer_wgpu::{PaneBlit, SnapshotLease, WgpuRenderer};

/// Compose the frame into `composition`, then place it on `composition_view`:
/// whole, or one pane at a time when a motion says the panes are not where the
/// presentation puts them. Chrome is drawn last, onto the placed result.
#[allow(clippy::too_many_arguments)]
pub(super) fn through_composition_ring(
    acquired: &super::surface::SurfaceAcquired,
    renderer: &mut WgpuRenderer,
    native: NativeDrawContext<'_>,
    render: &mut GuiFrameRenderState,
    composition: &SnapshotLease,
    composition_view: &wgpu::TextureView,
    frame: &mut crate::core::frame_glyphs::FrameGlyphBuffer,
    inputs: &FrameDrawInputs<'_>,
    cursor_visible: bool,
    accept_derived_effects: bool,
    pane_blits: &[PaneBlit],
) -> Result<(), super::surface::FrameRenderFailure> {
    let seed_retained = super::retained_static::can_seed_on_commit(
        renderer,
        pane_blits,
        render,
        frame,
        accept_derived_effects,
    );
    super::render_frame_window_contents(
        renderer,
        native,
        render,
        composition.view(),
        frame,
        inputs,
        if seed_retained { false } else { cursor_visible },
        false,
    )?;

    detect_transitions(acquired, renderer, render, frame, accept_derived_effects);
    if render.compositor.renderer_effects.needs_redraw() {
        render.mark_dirty();
    }

    if seed_retained && pane_blits.is_empty() && !render.compositor.transitions.has_active() {
        let cursor_cells = super::retained_static::build_filled_box_cursor_cells(
            frame,
            native.scale_factor as f32,
            &render.pointer_appearance,
        );
        let hovered_scroll_bar = render.hovered_scroll_bar(frame);
        super::retained_static::composite_cursor_over_retained(
            renderer,
            composition.bind_group(),
            &cursor_cells,
            composition_view,
            frame,
            inputs.present_mapping,
            cursor_visible,
            inputs.root_animated_cursor,
            hovered_scroll_bar,
            render.compositor.glyph_atlas.as_mut(),
        );
        super::chrome::render_frame_window_overlays_with_toolbar_resources(
            renderer,
            native,
            render,
            composition_view,
            frame,
            cursor_visible,
            inputs.animated_cursor,
            inputs.child_frame_style,
            inputs.scroll_indicators_enabled,
            inputs.toolbar,
        )?;
        if !render.compositor.renderer_effects.needs_redraw()
            && !render.compositor.transitions.has_active()
        {
            let includes_chrome = !super::retained_static::frame_has_static_chrome(
                renderer,
                native,
                frame,
                inputs.scroll_indicators_enabled,
            );
            render.compositor.retained_static = Some(
                crate::render_thread::frame_windows::RetainedStatic::from_shared_root(
                    composition.clone(),
                    includes_chrome,
                    render.compositor.current_scene_generation,
                    cursor_cells,
                ),
            );
        }
        return Ok(());
    }

    if pane_blits.is_empty() {
        if let Ok(mut draw) =
            renderer.begin_draw(neomacs_renderer_wgpu::renderer::RenderTarget::new(
                composition_view,
                inputs.present_mapping.surface(),
            ))
        {
            draw.blit_retained(composition.bind_group());
        }
    } else {
        // Same picture, placed rather than copied whole: each pane
        // reads the region of the composed frame it owns and draws
        // it where the motion currently puts it.
        // The picture this motion fades from, pinned by the morph on its first
        // frame. Read straight from the ring instead, it would be the previous
        // frame *of the motion* — the destination fading into the destination,
        // which renders as a completely static frame.
        place_pane_composition(
            renderer,
            render,
            composition,
            composition_view,
            (
                native.content_size.0 as f32 / native.scale_factor as f32,
                native.content_size.1 as f32 / native.scale_factor as f32,
            ),
            pane_blits,
        );
    }
    render_frame_transitions(
        renderer,
        &mut render.compositor.transitions,
        composition_view,
        native.content_size.0,
        native.content_size.1,
        frame,
    );
    if seed_retained {
        // Fallback if a transition unexpectedly started inside `detect_transitions`
        // after rendering `composition` cursorless: paint the cursor layer over the
        // transitioned surface so the frame never misses its cursor.
        let cursor_cells = super::retained_static::build_filled_box_cursor_cells(
            frame,
            native.scale_factor as f32,
            &render.pointer_appearance,
        );
        let hovered_scroll_bar = render.hovered_scroll_bar(frame);
        renderer.render_cursor_only(
            composition_view,
            frame,
            inputs.present_mapping,
            cursor_visible,
            inputs.root_animated_cursor,
            hovered_scroll_bar,
        );
        if cursor_visible && let Some(atlas) = render.compositor.glyph_atlas.as_mut() {
            for cell in &cursor_cells {
                atlas.set_current_frame_fonts(cell.mini.font_bindings());
                renderer.render_frame_cell_loaded(
                    composition_view,
                    &cell.mini,
                    atlas,
                    inputs.present_mapping,
                    true,
                    inputs.root_animated_cursor,
                    cell.scissor,
                );
            }
        }
    }
    if render.compositor.transitions.has_active() {
        render.mark_dirty();
    }
    super::chrome::render_frame_window_overlays_with_toolbar_resources(
        renderer,
        native,
        render,
        composition_view,
        frame,
        cursor_visible,
        inputs.animated_cursor,
        inputs.child_frame_style,
        inputs.scroll_indicators_enabled,
        inputs.toolbar,
    )?;
    Ok(())
}

/// Place and pin a pane picture using the content-local geometry supplied by
/// the native window. Admission and motion sampling remain the caller's work.
pub(super) fn place_pane_composition(
    renderer: &mut WgpuRenderer,
    render: &mut GuiFrameRenderState,
    composition: &SnapshotLease,
    destination: &wgpu::TextureView,
    logical_size: (f32, f32),
    pane_blits: &[PaneBlit],
) {
    let candidate = render
        .compositor
        .transitions
        .previous_composition()
        .cloned();
    let previous = render
        .compositor
        .layout
        .pin_outgoing(candidate)
        .map(|lease| lease.bind_group().clone());
    renderer.render_pane_layout(
        composition.bind_group(),
        previous.as_ref(),
        destination,
        logical_size,
        pane_blits,
    );
}

/// Draw the frame — content and overlays together — straight onto
/// `composition_view`.
///
/// This is the strategy for a frame with nothing to place and nothing to
/// transition, and also the degraded strategy when the composition ring
/// refused a slot.
#[allow(clippy::too_many_arguments)]
pub(super) fn onto_target(
    acquired: &super::surface::SurfaceAcquired,
    renderer: &mut WgpuRenderer,
    native: NativeDrawContext<'_>,
    render: &mut GuiFrameRenderState,
    composition_view: &wgpu::TextureView,
    frame: &mut crate::core::frame_glyphs::FrameGlyphBuffer,
    inputs: &FrameDrawInputs<'_>,
    cursor_visible: bool,
    accept_derived_effects: bool,
) -> Result<(), super::surface::FrameRenderFailure> {
    if render.cursor.is_animating()
        && super::retained_static::can_seed_on_commit(
            renderer,
            &[],
            render,
            frame,
            accept_derived_effects,
        )
        && let Some(native_size) =
            neomacs_renderer_wgpu::SnapshotSize::new(native.content_size.0, native.content_size.1)
    {
        // When offscreen composition is disabled (e.g., transitions and window
        // animations off) but cursor motion is active, build `RetainedStatic`
        // once on the commit frame so follow-up `CompositeOnly` ticks reuse it
        // without a second root glyph pass (#607 Stage 2B).
        super::retained_static::ensure_retained_static_texture(renderer, render, native_size);
        let retained_view = render
            .compositor
            .retained_static
            .as_ref()
            .expect("retained texture just ensured")
            .view()
            .clone();
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
        let cells = super::retained_static::build_filled_box_cursor_cells(
            frame,
            native.scale_factor as f32,
            &render.pointer_appearance,
        );
        if let Some(rs) = render.compositor.retained_static.as_mut() {
            rs.generation = render.compositor.current_scene_generation;
            rs.cursor_cells = cells;
        }
        detect_transitions(acquired, renderer, render, frame, accept_derived_effects);
        let hovered_scroll_bar = render.hovered_scroll_bar(frame);
        if let Some(rs) = render.compositor.retained_static.as_ref() {
            super::retained_static::composite_cursor_over_retained(
                renderer,
                rs.bind_group(),
                &rs.cursor_cells,
                composition_view,
                frame,
                inputs.present_mapping,
                cursor_visible,
                inputs.root_animated_cursor,
                hovered_scroll_bar,
                render.compositor.glyph_atlas.as_mut(),
            );
        }
        render.mark_active_visuals_dirty();
        return Ok(());
    }

    super::render_frame_window_contents(
        renderer,
        native,
        render,
        composition_view,
        frame,
        inputs,
        cursor_visible,
        true,
    )?;
    detect_transitions(acquired, renderer, render, frame, accept_derived_effects);
    render.mark_active_visuals_dirty();
    Ok(())
}

/// Measure this frame against the last presented one and start whatever motion
/// that implies.
///
/// Called from both strategies at the same point: after the content is drawn
/// and before anything is placed. The observations are drained here, not
/// earlier, because the surface-acquisition paths in the draw order can return
/// before this point — and an observation dropped on one of those would lose
/// the scroll measured at install, with no later chance to plan it.
/// Drain the observations measured at install and plan what they imply.
///
/// Takes the acquisition proof because the drain is one-shot: running it above
/// a surface-loss return would consume the scroll, reflow or text replacement
/// measured at install for a frame that is then abandoned, and no later frame
/// would get another chance to animate it.
fn detect_transitions(
    acquired: &super::surface::SurfaceAcquired,
    renderer: &mut WgpuRenderer,
    render: &mut GuiFrameRenderState,
    frame: &mut crate::core::frame_glyphs::FrameGlyphBuffer,
    accept_derived_effects: bool,
) {
    let pending_continuity = render.take_pending_continuity(acquired, accept_derived_effects);
    renderer.with_frame_effects(&mut render.compositor.renderer_effects, |renderer| {
        detect_frame_transitions(
            renderer,
            &mut render.compositor.transitions,
            &renderer.effects.clone(),
            frame,
            pending_continuity,
            &mut render.compositor.dirty,
        );
    });
}
