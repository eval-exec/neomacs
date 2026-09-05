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

use super::FrameDrawInputs;
use crate::render_thread::frame_windows::{GuiFrameNativeWindowState, GuiFrameRenderState};
use crate::render_thread::transitions::{detect_frame_transitions, render_frame_transitions};
use neomacs_renderer_wgpu::{PaneBlit, SnapshotLease, WgpuRenderer};

/// Compose the frame into `composition`, then place it on `composition_view`:
/// whole, or one pane at a time when a motion says the panes are not where the
/// presentation puts them. Chrome is drawn last, onto the placed result.
#[allow(clippy::too_many_arguments)]
pub(super) fn through_composition_ring(
    renderer: &mut WgpuRenderer,
    native: &GuiFrameNativeWindowState,
    render: &mut GuiFrameRenderState,
    composition: &SnapshotLease,
    composition_view: &wgpu::TextureView,
    frame: &mut crate::core::frame_glyphs::FrameGlyphBuffer,
    inputs: &FrameDrawInputs<'_>,
    cursor_visible: bool,
    accept_derived_effects: bool,
    pane_blits: &[PaneBlit],
) {
    super::render_frame_window_contents(
        renderer,
        native,
        render,
        composition.view(),
        frame,
        inputs,
        cursor_visible,
        false,
    );

    detect_transitions(renderer, render, frame, accept_derived_effects);
    if render.compositor.renderer_effects.needs_redraw() {
        render.mark_dirty();
    }

    if pane_blits.is_empty() {
        renderer.blit_texture_to_view(
            composition.bind_group(),
            composition_view,
            native.width,
            native.height,
        );
    } else {
        // Same picture, placed rather than copied whole: each pane
        // reads the region of the composed frame it owns and draws
        // it where the motion currently puts it.
        let previous = render
            .compositor
            .transitions
            .previous_composition()
            .map(|lease| lease.bind_group().clone());
        renderer.render_pane_layout(
            composition.bind_group(),
            previous.as_ref(),
            composition_view,
            (
                native.width as f32 / native.scale_factor as f32,
                native.height as f32 / native.scale_factor as f32,
            ),
            pane_blits,
        );
    }
    render_frame_transitions(
        renderer,
        &mut render.compositor.transitions,
        composition_view,
        native.width,
        native.height,
    );
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
    renderer: &mut WgpuRenderer,
    native: &GuiFrameNativeWindowState,
    render: &mut GuiFrameRenderState,
    composition_view: &wgpu::TextureView,
    frame: &mut crate::core::frame_glyphs::FrameGlyphBuffer,
    inputs: &FrameDrawInputs<'_>,
    cursor_visible: bool,
    accept_derived_effects: bool,
) {
    super::render_frame_window_contents(
        renderer,
        native,
        render,
        composition_view,
        frame,
        inputs,
        cursor_visible,
        true,
    );
    detect_transitions(renderer, render, frame, accept_derived_effects);
    render.mark_active_visuals_dirty();
}

/// Measure this frame against the last presented one and start whatever motion
/// that implies.
///
/// Called from both strategies at the same point: after the content is drawn
/// and before anything is placed. The observations are drained here, not
/// earlier, because the surface-acquisition paths in the draw order can return
/// before this point — and an observation dropped on one of those would lose
/// the scroll measured at install, with no later chance to plan it.
fn detect_transitions(
    renderer: &mut WgpuRenderer,
    render: &mut GuiFrameRenderState,
    frame: &mut crate::core::frame_glyphs::FrameGlyphBuffer,
    accept_derived_effects: bool,
) {
    let pending_continuity = render.take_pending_continuity(accept_derived_effects);
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
