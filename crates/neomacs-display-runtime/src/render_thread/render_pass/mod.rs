//! The draw order for one frame.
//!
//! Owns: the *sequence*, and only the sequence — acquire the surface, sample
//! the pane motion, pick a composition strategy, draw through it, and return a
//! [`RenderedFrameSurface`] for `present` to hand to the platform. Each phase
//! lives in a submodule with its own charter:
//!
//! - `surface` — getting the swapchain texture, and naming how that fails.
//! - `composition_targets` — the offscreen textures a frame composes through.
//! - `retained_static`, `full_render` — the three composition strategies: the
//!   compositor-only fast path, and the two that run the glyph pipeline.
//! - `scene` — the editor's own picture: glyphs, child frames, content overlays.
//! - `chrome` — the window-level overlays drawn over it.
//! - `present` — handing the result to the platform and publishing what it
//!   says about the screen.
//!
//! Must not: know how any single phase draws. When a body here grows past
//! "call the phase, check the outcome", it belongs in a phase.
//!
//! Three orderings are load-bearing, and each is commented where it happens
//! because the `?` between them is what makes it matter:
//!
//! 1. The frame itself is materialized *after* the surface is acquired.
//!    Acquisition has several early returns, so taking the frame first is work
//!    thrown away outright on any lost, outdated or occluded surface.
//! 2. The pane-layout sample runs *after* acquisition too, for a stronger
//!    reason: it advances the motion and produces the projection hit testing
//!    will answer from. Sampling above the early returns would leave a
//!    projection describing a frame nobody composed.
//! 3. The continuity drain runs *after* the surface-loss paths — it is in
//!    `full_render::detect_transitions`, below every `?` here. An observation
//!    dropped on one of those returns would lose the scroll measured at
//!    install, with no later chance to plan it.
//!
//! All three are enforced by [`surface::SurfaceAcquired`], which only
//! `surface::acquire_current_texture` can construct and which each of the three
//! phases requires. Moving one of them above acquisition does not compile.

// Several render entry points carry the recurring `bg_gradient` RGB-pair tuple
// parameter, which mirrors the renderer-wgpu API surface; a local type alias
// would not be reused, so the type-complexity lint is allowed module-wide.
#![allow(clippy::type_complexity)]

pub(in crate::render_thread) mod chrome;
mod composition_targets;
mod full_render;
mod present;
mod retained_static;
mod scene;
pub(in crate::render_thread) mod surface;

use self::surface::FrameRenderFailure;
use super::RenderApp;
use super::frame_windows::{
    FrameLifecycle, GuiFrameNativeWindowState, GuiFrameRenderState, GuiFrameWindowState,
};
use super::state::{ChildFrameStyle, ToolbarResources};
use crate::core::types::DisplayFrameId;
use neomacs_renderer_wgpu::{SnapshotSize, WgpuRenderer};

/// A frame drawn and ready to present.
///
/// `projection` rides here rather than being stored when it is computed. It
/// describes where the panes were placed *in this frame*, and it is the answer
/// hit testing must give — so it becomes visible only once the pixels it
/// describes have actually been presented. Publishing it at sample time
/// instead would leave it describing a frame that a later `?` abandoned, and a
/// pointer event arriving before the next successful render would resolve
/// against pixels nobody saw.
struct RenderedFrameSurface {
    output: wgpu::SurfaceTexture,
    frame: crate::core::frame_glyphs::FrameGlyphBuffer,
    projection: Option<neomacs_display_protocol::InteractionProjection>,
}

/// The inputs one frame's content draw needs that are fixed for the whole
/// frame, whichever composition strategy runs.
///
/// Bundled rather than threaded positionally because two of the fields have
/// the same type and different scope — `root_animated_cursor` is
/// `animated_cursor` filtered to the root frame — and a positional list makes
/// swapping them trivially easy to write and impossible to see. The two flags
/// that genuinely differ per call, `cursor_visible` and `include_overlays`,
/// stay explicit arguments for the same reason: they are the choice, not the
/// context.
struct FrameDrawInputs<'a> {
    present_mapping: neomacs_display_protocol::PresentMapping,
    root_animated_cursor: Option<crate::core::types::AnimatedCursor>,
    animated_cursor: Option<crate::core::types::AnimatedCursor>,
    bg_gradient: Option<((f32, f32, f32), (f32, f32, f32))>,
    child_frame_style: &'a ChildFrameStyle,
    scroll_indicators_enabled: bool,
    toolbar: &'a ToolbarResources,
}

#[allow(clippy::too_many_arguments)]
fn render_frame_window_contents(
    renderer: &mut WgpuRenderer,
    native: &GuiFrameNativeWindowState,
    render: &mut GuiFrameRenderState,
    surface_view: &wgpu::TextureView,
    frame: &crate::core::frame_glyphs::FrameGlyphBuffer,
    inputs: &FrameDrawInputs<'_>,
    cursor_visible: bool,
    include_overlays: bool,
) {
    scene::render_frame_root_glyphs(
        renderer,
        render,
        surface_view,
        frame,
        inputs.present_mapping,
        cursor_visible,
        inputs.root_animated_cursor,
        inputs.bg_gradient,
    );
    let renderer_effects_still_active = render.compositor.renderer_effects.needs_redraw();

    if !include_overlays {
        render.set_dirty(renderer_effects_still_active);
        return;
    }

    chrome::render_frame_window_overlays_with_toolbar_resources(
        renderer,
        native,
        render,
        surface_view,
        frame,
        cursor_visible,
        inputs.animated_cursor,
        inputs.child_frame_style,
        inputs.scroll_indicators_enabled,
        inputs.toolbar,
    );
    if renderer_effects_still_active {
        render.mark_dirty();
    }
}

#[allow(clippy::too_many_arguments)]
fn render_frame_window_contents_to_surface(
    renderer: &mut WgpuRenderer,
    window_state: &mut GuiFrameWindowState,
    bg_gradient: Option<((f32, f32, f32), (f32, f32, f32))>,
    child_frame_style: &ChildFrameStyle,
    scroll_indicators_enabled: bool,
    toolbar: &ToolbarResources,
    extra_line_spacing: f32,
    extra_letter_spacing: f32,
    compositor_only_hint: bool,
    render_policy: &crate::render_thread::render_quality::RenderQualityPolicy,
    device_lost: &mut crate::render_thread::device_loss::DeviceLossDetector,
) -> Result<RenderedFrameSurface, FrameRenderFailure> {
    let render = &mut window_state.render;
    let native = match &mut window_state.lifecycle {
        FrameLifecycle::Active { native, .. } => native,
        _ => return Err(FrameRenderFailure::WindowNotReady),
    };
    RenderApp::begin_fps_cpu_span(&mut render.overlays.fps);
    RenderApp::update_fps_counter(&mut render.overlays.fps, renderer.frame_sample());
    // Read the one bit the offscreen decision needs straight out of the
    // retained frame. Missing content has its own typed outcome: the frame
    // channel, rather than an expose retry, is responsible for waking it.
    // Read it here, before `take_current_frame_for_render` below drains
    // the hints. The frame itself is taken once, after the surface is
    // acquired — the acquisition has several early-return paths, so
    // materializing it earlier was work thrown away outright on any
    // lost/outdated/occluded surface.
    // Must be read before `take_pending_continuity` below drains it.
    let frame_has_theme_transition = render
        .pending_theme_change()
        .ok_or(FrameRenderFailure::AwaitingContent)?;
    let animated_cursor = render.cursor.animated_cursor();
    let root_animated_cursor = animated_cursor
        .filter(|cursor| cursor.frame_id == DisplayFrameId::new(render.emacs_frame_id));
    // The slide animation is composed at draw time: emit_cursor_visual reads
    // the interpolated rect from animated_cursor for the active window's
    // cursor. The frame's stored cursor geometry is no longer mutated here,
    // so the materialized frame stays a pure function of the layout snapshot.

    let feature_plan =
        render_policy.plan_frame(frame_has_theme_transition, renderer.has_frame_post());

    let surface::AcquiredSurface { output, acquired } =
        surface::acquire_current_texture(&native.surface, device_lost, render.emacs_frame_id)?;

    // Placed here, after the surface is in hand: `sample_pane_layout`
    // advances the motion and republishes the projection, and every path
    // above returns without drawing. Sampling before them would leave the
    // projection describing a frame that was never composed, so a pointer
    // event arriving before the next successful render would resolve
    // against pixels nobody saw — the one thing the projection exists to
    // prevent.
    //
    // It still runs before anything reads a pane's position, so the
    // transform hit testing uses and the geometry this pass draws come
    // from one sample of one motion rather than from two evaluations that
    // could land on different sides of a frame boundary.
    let composition = render.sample_pane_layout(&acquired, renderer.frame_sample());
    let pane_projection = composition.projection.clone();
    let pane_blits = composition.blits;
    if !pane_blits.is_empty() {
        render.mark_dirty();
    }
    // Re-resolve what the pointer is over, but only while panes are moving.
    //
    // Hover is otherwise resolved when a pointer event arrives, which is
    // correct at that instant and goes stale immediately afterwards if the
    // pointer holds still while a pane slides under it: no event fires, so the
    // shader keeps the `u`/`v` of a pane position that has since moved on. The
    // projection this frame just sampled is the answer, so ask it here.
    //
    // Costs nothing when nothing is moving, which is almost always: an empty
    // placement list skips it, and so does a frame with no shader surface to
    // route to — the search reads every glyph in the frame.
    if !pane_blits.is_empty()
        && renderer.has_shader_surfaces()
        && let Some((glyphs, point)) = render.glyph_hit_target(
            render.emacs_frame_id,
            render.mouse_pos.0,
            render.mouse_pos.1,
        )
        && let Some((surface_id, u, v)) =
            super::pointer_events::surface_glyph_hit_test(glyphs, point)
    {
        renderer.surface_mouse_hover(surface_id, u, v);
    }

    // A morph draws the composed frame once and then places it a pane at a
    // time, which needs the frame in a texture rather than straight on the
    // surface.
    let need_offscreen = feature_plan.use_transition_offscreen || !pane_blits.is_empty();

    let present_mapping = render
        .present_mapping()
        .ok_or(FrameRenderFailure::AwaitingContent)?;
    let mut frame = render
        .take_current_frame_for_render(&acquired)
        .ok_or(FrameRenderFailure::AwaitingContent)?;
    feature_plan.prepare_frame(&mut frame);
    render.begin_presentable_render();
    if extra_line_spacing != 0.0 || extra_letter_spacing != 0.0 {
        RenderApp::apply_extra_spacing(
            &mut frame.glyphs,
            &mut frame.window_cursors,
            extra_line_spacing,
            extra_letter_spacing,
        );
    }

    let surface_view = output
        .texture
        .create_view(&wgpu::TextureViewDescriptor::default());
    // Full-frame post shader: compose the ENTIRE frame (content,
    // transitions, overlays, cursor) into an intermediate texture and
    // shade it into the swapchain as the LAST step, so every present is
    // uniformly post-processed (Ghostty semantics — cursor included) and
    // partial-damage frames cannot mix shaded and unshaded regions.
    let surface_size = SnapshotSize::new(native.width, native.height);
    let frame_post_src = feature_plan
        .apply_frame_post
        .then(|| {
            surface_size
                .and_then(|size| composition_targets::ensure_frame_post_src(renderer, render, size))
        })
        .flatten();
    let frame_post_active = frame_post_src.is_some();
    let composition_view = frame_post_src.unwrap_or_else(|| surface_view.clone());
    let old_scale_factor = renderer.scale_factor();
    let old_width = renderer.width();
    let old_height = renderer.height();
    renderer.set_scale_factor(native.scale_factor as f32);
    renderer.resize(native.width, native.height);
    let cursor_visible = render.cursor.blink_on;
    composition_targets::report_unpooled_gpu_textures(renderer, render);

    let inputs = FrameDrawInputs {
        present_mapping,
        root_animated_cursor,
        animated_cursor,
        bg_gradient,
        child_frame_style,
        scroll_indicators_enabled,
        toolbar,
    };

    // The retained-static fast path is a whole composition strategy; it
    // owns its own eligibility rule and its own draw. What the draw order
    // keeps is the decision to take it and the tail every strategy shares.
    // Read once, so the composite and the frame-post step provably see one
    // value. The scroll-bar highlight takes the *projected* answer rather than
    // this raw position — the two differ whenever a pane is in motion, which is
    // the bug that put the highlight on the wrong thumb.
    let mouse_pos = render.mouse_pos;
    if retained_static::is_eligible(compositor_only_hint, &pane_blits, render) {
        let hovered_scroll_bar = render.hovered_scroll_bar(&frame);
        retained_static::draw(
            renderer,
            native,
            render,
            &composition_view,
            &frame,
            &inputs,
            cursor_visible,
            hovered_scroll_bar,
        );
        if frame_post_active {
            renderer.frame_post_to_view(
                &composition_view,
                &surface_view,
                native.width,
                native.height,
                mouse_pos,
            );
        }
        render.finish_pointer_paint_render();
        renderer.set_scale_factor(old_scale_factor);
        renderer.resize(old_width, old_height);
        // Forwarded, not discarded. Eligibility requires only that this frame
        // *places* nothing — and `sample_pane_layout` has one path that places
        // nothing while still producing a projection: a retarget that leaves
        // the panes already where the new layout wants them ends the motion and
        // publishes the settled transform. Hardcoding `None` here dropped it,
        // so hit testing went on using the morph's last mid-motion transform
        // for every frame afterwards, until some later morph happened to
        // publish again.
        return Ok(RenderedFrameSurface {
            output,
            frame,
            projection: pane_projection,
        });
    }

    // Rotated here rather than at the top of the frame: every path above
    // returns without composing anything, and advancing for a frame that
    // is never drawn would retire the picture a transition still needs.
    let composition = need_offscreen
        .then(|| composition_targets::advance_frame_composition(renderer, render, surface_size))
        .flatten();
    match composition.as_ref() {
        Some(composition) => full_render::through_composition_ring(
            &acquired,
            renderer,
            native,
            render,
            composition,
            &composition_view,
            &mut frame,
            &inputs,
            cursor_visible,
            feature_plan.accept_derived_effects,
            &pane_blits,
        ),
        None => full_render::onto_target(
            &acquired,
            renderer,
            native,
            render,
            &composition_view,
            &mut frame,
            &inputs,
            cursor_visible,
            feature_plan.accept_derived_effects,
        ),
    }

    if frame_post_active {
        renderer.frame_post_to_view(
            &composition_view,
            &surface_view,
            native.width,
            native.height,
            render.mouse_pos,
        );
    }
    render.finish_pointer_paint_render();
    renderer.set_scale_factor(old_scale_factor);
    renderer.resize(old_width, old_height);
    Ok(RenderedFrameSurface {
        output,
        frame,
        projection: pane_projection,
    })
}
