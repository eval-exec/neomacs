// Several render entry points carry the recurring `bg_gradient` RGB-pair tuple
// parameter, which mirrors the renderer-wgpu API surface; a local type alias
// would not be reused, so the type-complexity lint is allowed module-wide.
#![allow(clippy::type_complexity)]

pub(in crate::render_thread) mod chrome;
mod composition_targets;
mod present;
mod retained_static;
mod scene;
mod surface;

use self::surface::FrameRenderFailure;
use super::RenderApp;
use super::frame_windows::{
    FrameLifecycle, GuiFrameNativeWindowState, GuiFrameRenderState, GuiFrameWindowState,
};
use super::state::{ChildFrameStyle, ToolbarResources};
use super::transitions::{detect_frame_transitions, render_frame_transitions};
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

#[allow(clippy::too_many_arguments)]
fn render_frame_window_contents(
    renderer: &mut WgpuRenderer,
    native: &GuiFrameNativeWindowState,
    render: &mut GuiFrameRenderState,
    surface_view: &wgpu::TextureView,
    frame: &crate::core::frame_glyphs::FrameGlyphBuffer,
    present_mapping: neomacs_display_protocol::PresentMapping,
    cursor_visible: bool,
    root_animated_cursor: Option<crate::core::types::AnimatedCursor>,
    animated_cursor: Option<crate::core::types::AnimatedCursor>,
    bg_gradient: Option<((f32, f32, f32), (f32, f32, f32))>,
    include_overlays: bool,
    child_frame_style: &ChildFrameStyle,
    scroll_indicators_enabled: bool,
    toolbar: &ToolbarResources,
) {
    scene::render_frame_root_glyphs(
        renderer,
        render,
        surface_view,
        frame,
        present_mapping,
        cursor_visible,
        root_animated_cursor,
        bg_gradient,
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
        animated_cursor,
        child_frame_style,
        scroll_indicators_enabled,
        toolbar,
    );
    if renderer_effects_still_active {
        render.mark_dirty();
    }
}

impl RenderApp {
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
        render_policy: &super::render_quality::RenderQualityPolicy,
        device_lost: &mut super::device_loss::DeviceLossDetector,
    ) -> Result<RenderedFrameSurface, FrameRenderFailure> {
        Self::render_frame_window_contents_to_acquired_surface(
            renderer,
            window_state,
            bg_gradient,
            child_frame_style,
            scroll_indicators_enabled,
            toolbar,
            extra_line_spacing,
            extra_letter_spacing,
            None,
            compositor_only_hint,
            render_policy,
            device_lost,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn render_frame_window_contents_to_acquired_surface(
        renderer: &mut WgpuRenderer,
        window_state: &mut GuiFrameWindowState,
        bg_gradient: Option<((f32, f32, f32), (f32, f32, f32))>,
        child_frame_style: &ChildFrameStyle,
        scroll_indicators_enabled: bool,
        toolbar: &ToolbarResources,
        extra_line_spacing: f32,
        extra_letter_spacing: f32,
        output: Option<wgpu::SurfaceTexture>,
        compositor_only_hint: bool,
        render_policy: &super::render_quality::RenderQualityPolicy,
        device_lost: &mut super::device_loss::DeviceLossDetector,
    ) -> Result<RenderedFrameSurface, FrameRenderFailure> {
        let render = &mut window_state.render;
        let native = match &mut window_state.lifecycle {
            FrameLifecycle::Active { native, .. } => native,
            _ => return Err(FrameRenderFailure::WindowNotReady),
        };
        Self::begin_fps_cpu_span(&mut render.overlays.fps);
        Self::update_fps_counter(&mut render.overlays.fps, renderer.frame_sample());
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

        let output = match output {
            Some(output) => output,
            None => surface::acquire_current_texture(
                &native.surface,
                device_lost,
                render.emacs_frame_id,
            )?,
        };

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
        let composition = render.sample_pane_layout(renderer.frame_sample());
        let pane_projection = composition.projection.clone();
        let pane_blits = composition.blits;
        if !pane_blits.is_empty() {
            render.mark_dirty();
        }
        // A morph draws the composed frame once and then places it a pane at a
        // time, which needs the frame in a texture rather than straight on the
        // surface.
        let need_offscreen = feature_plan.use_transition_offscreen || !pane_blits.is_empty();

        let present_mapping = render
            .present_mapping()
            .ok_or(FrameRenderFailure::AwaitingContent)?;
        let mut frame = render
            .take_current_frame_for_render()
            .ok_or(FrameRenderFailure::AwaitingContent)?;
        feature_plan.prepare_frame(&mut frame);
        render.begin_presentable_render();
        if extra_line_spacing != 0.0 || extra_letter_spacing != 0.0 {
            Self::apply_extra_spacing(
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
                surface_size.and_then(|size| {
                    composition_targets::ensure_frame_post_src(renderer, render, size)
                })
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

        // The retained-static fast path is a whole composition strategy; it
        // owns its own eligibility rule and its own draw. What the draw order
        // keeps is the decision to take it and the tail every strategy shares.
        let mouse_pos = render.mouse_pos;
        if retained_static::is_eligible(compositor_only_hint, &pane_blits, render) {
            retained_static::draw(
                renderer,
                native,
                render,
                &composition_view,
                &frame,
                present_mapping,
                cursor_visible,
                root_animated_cursor,
                animated_cursor,
                bg_gradient,
                child_frame_style,
                scroll_indicators_enabled,
                toolbar,
                mouse_pos,
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
            // No projection: eligibility required that no pane is moving, so
            // this frame places nothing that hit testing has to be told about.
            return Ok(RenderedFrameSurface {
                output,
                frame,
                projection: None,
            });
        }

        // Rotated here rather than at the top of the frame: every path above
        // returns without composing anything, and advancing for a frame that
        // is never drawn would retire the picture a transition still needs.
        let composition = need_offscreen
            .then(|| composition_targets::advance_frame_composition(renderer, render, surface_size))
            .flatten();
        if let Some(composition) = composition.as_ref() {
            render_frame_window_contents(
                renderer,
                native,
                render,
                composition.view(),
                &frame,
                present_mapping,
                cursor_visible,
                root_animated_cursor,
                animated_cursor,
                bg_gradient,
                false,
                child_frame_style,
                scroll_indicators_enabled,
                toolbar,
            );

            // Drained here, not earlier: the surface-acquisition paths above can
            // return, and observations dropped on one of those would lose the
            // scroll measured at install with no later chance to plan it.
            let pending_continuity =
                render.take_pending_continuity(feature_plan.accept_derived_effects);
            renderer.with_frame_effects(&mut render.compositor.renderer_effects, |renderer| {
                detect_frame_transitions(
                    renderer,
                    &mut render.compositor.transitions,
                    &renderer.effects.clone(),
                    &mut frame,
                    pending_continuity,
                    &mut render.compositor.dirty,
                );
            });
            if render.compositor.renderer_effects.needs_redraw() {
                render.mark_dirty();
            }

            if pane_blits.is_empty() {
                renderer.blit_texture_to_view(
                    composition.bind_group(),
                    &composition_view,
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
                    &composition_view,
                    (
                        native.width as f32 / native.scale_factor as f32,
                        native.height as f32 / native.scale_factor as f32,
                    ),
                    &pane_blits,
                );
            }
            render_frame_transitions(
                renderer,
                &mut render.compositor.transitions,
                &composition_view,
                native.width,
                native.height,
            );
            if render.compositor.transitions.has_active() {
                render.mark_dirty();
            }
            chrome::render_frame_window_overlays_with_toolbar_resources(
                renderer,
                native,
                render,
                &composition_view,
                &frame,
                cursor_visible,
                animated_cursor,
                child_frame_style,
                scroll_indicators_enabled,
                toolbar,
            );
        } else {
            render_frame_window_contents(
                renderer,
                native,
                render,
                &composition_view,
                &frame,
                present_mapping,
                cursor_visible,
                root_animated_cursor,
                animated_cursor,
                bg_gradient,
                true,
                child_frame_style,
                scroll_indicators_enabled,
                toolbar,
            );
            // Drained here, not earlier: the surface-acquisition paths above can
            // return, and observations dropped on one of those would lose the
            // scroll measured at install with no later chance to plan it.
            let pending_continuity =
                render.take_pending_continuity(feature_plan.accept_derived_effects);
            renderer.with_frame_effects(&mut render.compositor.renderer_effects, |renderer| {
                detect_frame_transitions(
                    renderer,
                    &mut render.compositor.transitions,
                    &renderer.effects.clone(),
                    &mut frame,
                    pending_continuity,
                    &mut render.compositor.dirty,
                );
            });
            render.mark_active_visuals_dirty();
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
}
