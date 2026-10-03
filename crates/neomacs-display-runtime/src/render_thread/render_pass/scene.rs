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
use crate::render_thread::frame_windows::GuiFrameRenderState;

#[cfg(test)]
#[path = "scene/tests.rs"]
mod tests;
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
    retain_scroll_body: bool,
) {
    frame_stats::count(&frame_stats::ROOT_GLYPH_PASSES);
    let raster = super::retained_scroll::prepare(
        renderer,
        render,
        frame,
        present_mapping,
        bg_gradient.is_some() || !retain_scroll_body,
    );
    let frame = raster.as_ref().map_or(frame, |raster| &raster.frame);
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
            if raster.is_some() {
                None
            } else {
                render.compositor.current_row_damage.as_ref()
            },
        );
    });
    if let Some(raster) = raster {
        let region = neomacs_renderer_wgpu::renderer::SnapshotRegion::new(
            &raster.texture,
            raster.source_pixels,
            raster.destination,
        )
        .expect("crop was validated before drawing the base");
        renderer
            .begin_draw(neomacs_renderer_wgpu::renderer::RenderTarget::new(
                surface_view,
                present_mapping.surface(),
            ))
            .blit_snapshot_region(region);
        frame_stats::count(&frame_stats::SCROLL_RASTER_BLITS);
        tracing::debug!(target: "neomacs_display_runtime::retained_scroll", "composited retained scroll raster");
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn render_frame_content_overlays(
    renderer: &mut WgpuRenderer,
    content_size: (u32, u32),
    render: &mut GuiFrameRenderState,
    surface_view: &wgpu::TextureView,
    frame: &crate::core::frame_glyphs::FrameGlyphBuffer,
    cursor_visible: bool,
    animated_cursor: Option<crate::core::types::AnimatedCursor>,
    child_frame_style: &ChildFrameStyle,
    scroll_indicators_enabled: bool,
) {
    let pointer_appearance = render.pointer_appearance;
    // One sample drives every child-frame lifecycle animation on this
    // surface. Sampling -- not stepping -- is what makes a frame redrawn at
    // the same instant identical, so coalescing and dropped frames cannot
    // change where a fade lands.
    let sample = renderer.frame_sample();
    let mut child_animation_active = false;
    // Finished open animations are cleared after the pass, so the retained
    // path's activity check does not pin the surface to full renders
    // forever after the first popup.
    let mut finished_animations = Vec::new();
    // Positions each drifting frame was painted at, written back after the
    // pass so a re-anchor arriving before the drift finishes can retarget
    // from where the user actually saw the popup.
    let mut drift_drawn: Vec<(u64, f32, f32, f32)> = Vec::new();
    // Whether a drift finished this pass: the popup reached its placed
    // position, and the retained texture -- frozen mid-drift at an
    // interpolated origin -- must rebuild before it is blitted again.
    let mut drift_finished = false;
    // Whether the corpse-clearing (or crossfade-closing) repaint was
    // requested this pass.
    let mut crossfade_finished = false;
    renderer.with_frame_effects(&mut render.compositor.renderer_effects, |renderer| {
        // One merged draw order: living frames and dying corpses
        // interleaved in z-path order, a corpse drawing before a living
        // frame at equal z -- a dismissed popup is normally being replaced
        // by the popup that took its place at the same z, and the old
        // picture receding *beneath* the new one reads as the new one
        // arriving.
        let merged = render.compositor.child_frames.merged_render_order();
        for (child_id, dying) in merged {
            let (
                child_frame,
                base_x,
                base_y,
                clip_in_root,
                alpha,
                offset_y,
                crossfade_layer,
                scale,
            ) = if dying {
                let Some(dying_entry) = render.compositor.child_frames.dying_entry(child_id) else {
                    continue;
                };
                // A corpse's payload is frozen by definition -- nothing will
                // ever re-ingest it -- so the living-draw stale-catalog
                // guard below does not apply to it. The atlas is
                // content-addressed and retains its entries across
                // face-table updates, and the corpse is transient and
                // uninteractive; refusing its pixels here would make every
                // dismissal instantly vanish instead of fading.
                let animation = dying_entry.animation;
                let progress = animation.motion.sample(sample);
                if progress.finished {
                    continue;
                }
                child_animation_active = true;
                // The close path runs progress toward "gone": opacity
                // falls, and any slide distance carries the frame further
                // down.
                let alpha = 1.0 - progress.content_mix.get();
                if alpha <= 0.0 {
                    continue;
                }
                let offset_y = animation.slide_pixels * progress.progress;
                // The departing frame shrinks toward its configured start
                // scale on the slot's own curve, unclamped: a spring's
                // overshoot below the target scale is the point.
                let scale = 1.0 - (1.0 - animation.scale_from) * progress.progress;
                let neomacs_display_protocol::PresentedClip::Rect(clip_in_root) =
                    dying_entry.entry.clip_in_root
                else {
                    continue;
                };
                (
                    &dying_entry.entry.frame,
                    dying_entry.entry.abs_x,
                    dying_entry.entry.abs_y,
                    clip_in_root,
                    alpha * dying_entry.entry.applied_frame_alpha,
                    offset_y,
                    None,
                    scale,
                )
            } else {
                let Some(child_entry) = render.compositor.child_frames.frames.get(&child_id) else {
                    continue;
                };
                if child_entry.frame.font_catalog_generation != frame.font_catalog_generation {
                    tracing::debug!(
                        frame_id = child_id,
                        child_generation = child_entry.frame.font_catalog_generation.get(),
                        root_generation = frame.font_catalog_generation.get(),
                        "skipping retained child frame from a stale font catalog generation"
                    );
                    continue;
                }
                // An entry without an animation draws settled, at full
                // opacity, at its placed position -- byte-identical to the
                // pre-animation path, which is what keeps this feature free
                // when it is off.
                let (alpha, offset_y, scale, finished) =
                    child_entry
                        .animation
                        .as_ref()
                        .map_or((1.0, 0.0, 1.0, true), |animation| {
                            let progress = animation.motion.sample(sample);
                            if progress.finished {
                                (1.0, 0.0, 1.0, true)
                            } else {
                                child_animation_active = true;
                                // The scale rides the slot's own curve,
                                // unclamped like the slide: a spring's
                                // overshoot past the settled size is the
                                // point of the pop.
                                let scale = if animation.closing {
                                    1.0 - (1.0 - animation.scale_from) * progress.progress
                                } else {
                                    animation.scale_from
                                        + (1.0 - animation.scale_from) * progress.progress
                                };
                                if animation.closing {
                                    (
                                        1.0 - progress.content_mix.get(),
                                        animation.slide_pixels * progress.progress,
                                        scale,
                                        false,
                                    )
                                } else {
                                    // The open path rises: the frame starts
                                    // `slide_pixels` below its placement and
                                    // settles onto it, with the same curve as
                                    // its opacity.
                                    (
                                        progress.content_mix.get(),
                                        animation.slide_pixels * (1.0 - progress.progress),
                                        scale,
                                        false,
                                    )
                                }
                            }
                        });
                if finished && child_entry.animation.is_some() {
                    finished_animations.push(child_id);
                }
                // Resize mixes complete unscaled old/new pictures in reserved
                // scratch before applying accepted/lifecycle opacity once. At
                // completion the next frame uses the new payload alone.
                let mut crossfade_layer: Option<(
                    &neomacs_renderer_wgpu::SnapshotLease,
                    f32,
                    f32,
                    f32,
                )> = None;
                if let Some(crossfade) = child_entry.crossfade.as_ref() {
                    let progress = crossfade.motion.sample(sample);
                    if progress.finished {
                        crossfade_finished = true;
                    } else {
                        child_animation_active = true;
                        let mix = progress.content_mix.get();
                        crossfade_layer = Some((
                            &crossfade.old,
                            crossfade.old_width,
                            crossfade.old_height,
                            mix,
                        ));
                    }
                }
                // A placement drift glides the frame from where the last
                // pass painted it to its placed position. While it runs,
                // the drawn origin is the interpolation; the placed
                // position is where it settles, reached with the speed the
                // popup had when the re-anchor interrupted it.
                let mut drawn_x = child_entry.abs_x;
                let mut drawn_y = child_entry.abs_y;
                if let Some(drift) = child_entry.drift.as_ref() {
                    let progress = drift.motion.sample(sample);
                    if progress.finished {
                        drift_finished = true;
                    } else {
                        child_animation_active = true;
                        drawn_x = drift.from_x + (drawn_x - drift.from_x) * progress.progress;
                        drawn_y = drift.from_y + (drawn_y - drift.from_y) * progress.progress;
                        drift_drawn.push((child_id, drawn_x, drawn_y, progress.rate));
                    }
                }
                // Accepted/lifecycle opacity belongs to the completed resize
                // picture, not the independently weighted new content.
                let effective_alpha = alpha * child_entry.applied_frame_alpha;
                if effective_alpha <= 0.0 {
                    continue;
                }
                let neomacs_display_protocol::PresentedClip::Rect(clip_in_root) =
                    child_entry.clip_in_root
                else {
                    continue;
                };
                (
                    &child_entry.frame,
                    drawn_x,
                    drawn_y,
                    clip_in_root,
                    effective_alpha,
                    offset_y,
                    crossfade_layer,
                    scale,
                )
            };
            let pointer_selection = pointer_appearance.selection_for(child_frame);
            if let Some(atlas) = render.compositor.glyph_atlas.as_mut() {
                atlas.set_current_frame_fonts(child_frame.font_bindings());
            }
            if dying {
                tracing::debug!(
                    parent_frame_id = render.emacs_frame_id,
                    frame_id = child_id,
                    alpha,
                    scale,
                    "child_frame_lifecycle: render_dying_child_frame"
                );
            } else {
                tracing::debug!(
                    parent_frame_id = render.emacs_frame_id,
                    frame_id = child_id,
                    x = base_x,
                    y = base_y + offset_y,
                    width = child_frame.width,
                    height = child_frame.height,
                    alpha,
                    scale,
                    glyphs = child_frame.glyphs.len(),
                    "child_frame_lifecycle: render_child_frame_start"
                );
            }
            renderer.render_child_frame_prepared(
                surface_view,
                child_frame,
                base_x,
                base_y + offset_y,
                clip_in_root,
                render.compositor.glyph_atlas.as_mut().unwrap(),
                content_size.0,
                content_size.1,
                cursor_visible,
                animated_cursor.filter(|ac| ac.frame_id == DisplayFrameId::new(child_id)),
                child_frame_style.corner_radius,
                child_frame_style.shadow_enabled,
                child_frame_style.shadow_layers,
                child_frame_style.shadow_offset,
                child_frame_style.shadow_opacity,
                pointer_selection,
                alpha,
                scale,
                // The scale anchors at the frame's drawn top-left, so the
                // picture grows outward from the point that anchored it.
                [base_x, base_y + offset_y],
                render.child_opacity_src.as_ref(),
                crossfade_layer.map(|(old, old_width, old_height, mix)| {
                    frame_stats::count(&frame_stats::CHILD_FRAME_CROSSFADE_QUADS);
                    neomacs_renderer_wgpu::renderer::ChildResizePicture {
                        old,
                        old_width,
                        old_height,
                        mix,
                        composition: render
                            .child_resize_src
                            .as_ref()
                            .expect("resize composition was preflighted"),
                    }
                }),
            );
            if !dying {
                tracing::debug!(
                    parent_frame_id = render.emacs_frame_id,
                    frame_id = child_id,
                    "child_frame_lifecycle: render_child_frame_done"
                );
            }
        }
    });
    // The pass reports continued animation the only way it may: by marking
    // the frame dirty, so the scheduler asks for another one. Finished dying
    // frames are pruned afterwards, on the same sample they were drawn with.
    if child_animation_active || render.compositor.renderer_effects.needs_redraw() {
        render.mark_dirty();
    }
    for frame_id in finished_animations {
        render
            .compositor
            .child_frames
            .clear_finished_animation(frame_id);
    }
    if crossfade_finished && render.compositor.child_frames.prune_crossfades(sample) {
        // The mix ended: one more repaint draws the new payload alone.
        render.mark_dirty();
    }
    for (frame_id, x, y, rate) in drift_drawn {
        render
            .compositor
            .child_frames
            .record_drift_drawn(frame_id, x, y, rate);
    }
    if drift_finished && render.compositor.child_frames.clear_finished_drifts(sample) {
        // The popup reached its anchor. The retained texture was frozen
        // mid-drift at an interpolated origin -- bump the generation so the
        // next eligible frame rebuilds it with the popup settled, and one
        // more repaint actually shows that.
        render.compositor.current_scene_generation =
            crate::render_thread::frame_state::next_scene_generation();
        render.mark_dirty();
    }
    if render.compositor.child_frames.has_dying()
        && render.compositor.child_frames.prune_dying(sample)
    {
        // The corpse just left the list, but its last drawn pixels are on
        // screen and the standing demand retracts with it. One more repaint
        // is what actually clears them; without it the dismissed popup's
        // area freezes at whatever the final fade frame showed.
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
            content_size.0,
            content_size.1,
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
