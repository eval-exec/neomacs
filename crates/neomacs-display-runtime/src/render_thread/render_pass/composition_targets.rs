//! The offscreen textures a frame composes *through*, and the accounting for
//! the ones the pool does not own.
//!
//! Owns: the frame-post source lease, the composition-ring rotation that hands
//! a transition or a pane morph somewhere to draw before it is placed, the
//! single place a refused GPU-budget lease is counted and logged, and the
//! per-frame census of this window's unpooled full-frame textures.
//!
//! Must not: draw, consume observations or publish presentation state. Optional
//! effects may degrade on budget pressure; required native/child composition
//! instead returns the typed retry without presenting incomplete pixels.

use crate::render_thread::frame_stats;
use crate::render_thread::frame_windows::GuiFrameRenderState;
use neomacs_renderer_wgpu::{
    BudgetExceeded, GpuBudgetOwner, SnapshotLease, SnapshotSize, UnpooledTexture, WgpuGlyphAtlas,
    WgpuRenderer,
};

/// Only provably opaque native output may bypass encoded-premultiplied
/// conversion. This same pure proof governs early obsolete-owner retirement.
fn native_content_required(
    renderer: &WgpuRenderer,
    render: &GuiFrameRenderState,
    surface: neomacs_display_protocol::DrawableSurface,
    plan: &crate::render_thread::render_quality::RenderFeaturePlan,
) -> bool {
    surface.content_insets() != Default::default()
        || render.applied_frame_alpha != 1.0
        || render
            .compositor
            .current_frame
            .as_ref()
            .is_some_and(|frame| {
                frame.background_alpha != 1.0
                    || render.compositor.layout.needs_native_conversion(
                        renderer.frame_sample(),
                        crate::render_thread::frame_compositor::continuity::pane_layout::PixelGrid::new(
                            surface.device_scale().get() as f64,
                        ),
                        render.compositor.transitions.compositions.as_ref()
                            .map(|ring| ring.current()),
                    )
                    || render.compositor.transitions.needs_native_conversion(
                        frame,
                        renderer.frame_sample(),
                        &render.compositor.pending,
                        &renderer.effects,
                        plan.accept_transition_hints,
                        plan.accept_derived_effects,
                    )
            })
}

/// Native conversion and overlay insets require a charged content target;
/// allocation failure must not silently bypass their final boundary.
pub(super) fn native_content_target(
    renderer: &mut WgpuRenderer,
    render: &mut GuiFrameRenderState,
    surface: neomacs_display_protocol::DrawableSurface,
    plan: &crate::render_thread::render_quality::RenderFeaturePlan,
) -> Result<Option<SnapshotLease>, super::surface::FrameRenderFailure> {
    if !native_content_required(renderer, render, surface, plan) {
        render.native_content_src = None;
        return Ok(None);
    }
    let content = surface
        .content_surface()
        .ok_or(super::surface::FrameRenderFailure::WindowNotReady)?;
    let size =
        SnapshotSize::new(content.device_width().get(), content.device_height().get()).unwrap();
    if !render
        .native_content_src
        .as_ref()
        .is_some_and(|lease| lease.size() == size)
    {
        render.native_content_src = None;
        render.native_content_src = Some(renderer.acquire_snapshot(size).map_err(|exceeded| {
            note_refused_full_frame_texture(&exceeded, "native content viewport");
            super::surface::FrameRenderFailure::WindowNotReady
        })?);
    }
    Ok(render.native_content_src.clone())
}

/// Lease the intermediate composition texture for the full-frame post
/// shader at the window's physical size; returns its view.
///
/// `None` means the budget refused the lease, and the caller composes
/// straight to the swapchain without the post shader: one unshaded frame
/// is a better answer than a dropped one.
pub(super) fn ensure_frame_post_src(
    renderer: &mut WgpuRenderer,
    render: &mut GuiFrameRenderState,
    size: SnapshotSize,
) -> Option<wgpu::TextureView> {
    if !render
        .frame_post_src
        .as_ref()
        .is_some_and(|lease| lease.size() == size)
    {
        // Dropped before the acquire so the pool can re-cut the old-size
        // slot rather than allocate beside it.
        render.frame_post_src = None;
        match renderer.acquire_snapshot(size) {
            Ok(lease) => render.frame_post_src = Some(lease),
            Err(exceeded) => {
                note_refused_full_frame_texture(&exceeded, "frame post source");
                return None;
            }
        }
    }
    render
        .frame_post_src
        .as_ref()
        .map(|lease| lease.view().clone())
}

/// Rotate the frame window's composition ring and hand back the slot this
/// frame composes into.
///
/// `None` degrades the frame to composing straight on the surface, which
/// costs the transitions and pane motion for that frame and nothing else.
/// GPU pressure is a real state, not a masked bug, so it is counted.
pub(super) fn advance_frame_composition(
    renderer: &mut WgpuRenderer,
    render: &mut GuiFrameRenderState,
    surface_size: Option<SnapshotSize>,
) -> Option<SnapshotLease> {
    let size = surface_size?;
    match render
        .compositor
        .transitions
        .advance_compositions(renderer, size)
    {
        Ok(lease) => Some(lease),
        Err(exceeded) => {
            note_refused_full_frame_texture(&exceeded, "frame composition");
            None
        }
    }
}

fn note_refused_full_frame_texture(exceeded: &BudgetExceeded, what: &'static str) {
    frame_stats::count(&frame_stats::FULL_FRAME_TEXTURE_REFUSALS);
    tracing::debug!(
        %exceeded,
        what,
        "GPU budget refused a full-frame texture; composing without it"
    );
}

/// Re-report every full-frame GPU texture this window owns that the
/// snapshot pool does not hand out.
///
/// Derived from live state once per frame rather than registered at
/// creation: a census that is re-stated every frame cannot drift, whereas
/// a charge/refund pair drifts the first time a release site is added
/// without a matching refund.
pub(super) fn report_unpooled_gpu_textures(
    renderer: &mut WgpuRenderer,
    render: &GuiFrameRenderState,
) {
    let owner = GpuBudgetOwner::FrameWindow(render.emacs_frame_id);
    match render.compositor.retained_static.as_ref() {
        Some(retained) => renderer.record_full_frame_texture(owner, &retained.texture),
        None => renderer.retire_full_frame_texture(owner, UnpooledTexture::RetainedStaticScene),
    }
    let atlas_bytes = render
        .compositor
        .glyph_atlas
        .as_ref()
        .map_or(0, WgpuGlyphAtlas::resident_bytes);
    renderer.record_glyph_atlas_bytes(owner, atlas_bytes);
    let budget = renderer.gpu_budget();
    tracing::trace!(
        ?owner,
        pooled_bytes = budget.pooled_bytes(),
        unpooled_bytes = budget.unpooled_bytes(),
        limit_bytes = budget.limit_bytes().get(),
        "full-frame GPU texture accounting"
    );
}

/// Native and child targets are admitted before acquisition or any presentation
/// sampling. Keep this order in one production entry point.
pub(super) struct FrameTargets {
    pub(super) picture: Option<SnapshotLease>,
    pub(super) resize: Option<SnapshotLease>,
    pub(super) native: Option<SnapshotLease>,
}

#[cfg(test)]
pub(super) fn prepare_frame_targets(
    renderer: &mut WgpuRenderer,
    render: &mut GuiFrameRenderState,
    surface: neomacs_display_protocol::DrawableSurface,
    plan: &crate::render_thread::render_quality::RenderFeaturePlan,
) -> Result<FrameTargets, super::surface::FrameRenderFailure> {
    prepare_frame_targets_for_scene(renderer, render, surface, plan, true, false)
}

pub(super) fn prepare_frame_targets_for_scene(
    renderer: &mut WgpuRenderer,
    render: &mut GuiFrameRenderState,
    surface: neomacs_display_protocol::DrawableSurface,
    plan: &crate::render_thread::render_quality::RenderFeaturePlan,
    compositor_only_hint: bool,
    has_gradient: bool,
) -> Result<FrameTargets, super::surface::FrameRenderFailure> {
    let content = surface
        .content_surface()
        .ok_or(super::surface::FrameRenderFailure::WindowNotReady)?;
    let size =
        SnapshotSize::new(content.device_width().get(), content.device_height().get()).unwrap();
    let sample = renderer.frame_sample();
    reclaim_finished_child_composition(render, sample);
    let grid = crate::render_thread::frame_compositor::continuity::pane_layout::PixelGrid::new(
        surface.device_scale().get() as f64,
    );
    render.compositor.transitions.reclaim_finished(sample);
    render
        .compositor
        .layout
        .reclaim_finished_outgoing(sample, grid);
    if !plan.apply_frame_post
        || render
            .frame_post_src
            .as_ref()
            .is_some_and(|lease| lease.size() != size)
    {
        render.frame_post_src = None;
    }
    let panes = render
        .compositor
        .layout
        .planned_sample(sample, grid)
        .map_or_else(Vec::new, |sample| sample.pane_blits());
    if !super::retained_static::is_valid(render, size)
        || !super::retained_static::is_eligible(compositor_only_hint, &panes, render)
    {
        render.compositor.retained_static = None;
    }
    super::retained_scroll::retire_unusable(renderer, render, has_gradient, surface.device_scale());
    // A dropped unpooled picture must stop charging admission in this retry,
    // not in a successful draw which that stale census could prevent.
    report_unpooled_gpu_textures(renderer, render);
    // Reclaim only ownership proven obsolete, before mandatory child admission.
    // No surface, hints, motion sample or interactive projection is consumed.
    if !native_content_required(renderer, render, surface, plan)
        || render
            .native_content_src
            .as_ref()
            .is_some_and(|lease| lease.size() != size)
    {
        render.native_content_src = None;
    }
    let (picture, resize) = prepare_child_targets(renderer, render, size)?;
    let native = native_content_target(renderer, render, surface, plan)?;
    Ok(FrameTargets {
        picture,
        resize,
        native,
    })
}

/// Required child pictures are not optional visual decoration. Reserve this
/// before surface acquisition and return the existing retry outcome on pressure.
pub(super) fn mandatory_child_target(
    renderer: &mut WgpuRenderer,
    required: bool,
    size: SnapshotSize,
) -> Result<Option<SnapshotLease>, super::surface::FrameRenderFailure> {
    if !required {
        return Ok(None);
    }
    renderer.acquire_snapshot(size).map(Some).map_err(|exceeded| {
        frame_stats::count(&frame_stats::FULL_FRAME_TEXTURE_REFUSALS);
        tracing::debug!(%exceeded, "mandatory child composition not ready; retrying without present");
        super::surface::FrameRenderFailure::WindowNotReady
    })
}

#[cfg(test)]
#[path = "composition_targets/tests/composition_targets_test.rs"]
mod tests;

/// Preflight child composition before acquiring a surface or advancing any
/// presentation/projection state. Both leases remain local until admitted.
pub(super) fn prepare_child_targets(
    renderer: &mut WgpuRenderer,
    render: &mut GuiFrameRenderState,
    size: SnapshotSize,
) -> Result<(Option<SnapshotLease>, Option<SnapshotLease>), super::surface::FrameRenderFailure> {
    reclaim_finished_child_composition(render, renderer.frame_sample());
    let children = &render.compositor.child_frames;
    let needs_picture = children.has_dying()
        || children.frames.values().any(|entry| {
            entry.frame.background_alpha != 1.0
                || entry.applied_frame_alpha != 1.0
                // Dormant policy components are not applied picture opacity.
                || entry.animation.is_some()
                || entry.crossfade.is_some()
        });
    mandatory_child_targets(
        renderer,
        needs_picture,
        children
            .frames
            .values()
            .any(|entry| entry.crossfade.is_some()),
        size,
    )
}

fn reclaim_finished_child_composition(
    render: &mut GuiFrameRenderState,
    sample: neomacs_display_protocol::frame_time::FrameSample,
) {
    if render
        .compositor
        .child_frames
        .reclaim_finished_composition(sample)
    {
        // Settled state may become retained-static eligible; retire any texture
        // of the old animated scene as well as requesting its final repaint.
        render.compositor.current_scene_generation =
            crate::render_thread::frame_state::next_scene_generation();
        render.compositor.current_row_damage = None;
        render.mark_dirty();
    }
}

/// Both mandatory scratch leases are accepted atomically by the caller.
/// A second refusal releases the first before returning the typed retry.
pub(super) fn mandatory_child_targets(
    renderer: &mut WgpuRenderer,
    picture: bool,
    resize: bool,
    size: SnapshotSize,
) -> Result<(Option<SnapshotLease>, Option<SnapshotLease>), super::surface::FrameRenderFailure> {
    let picture = mandatory_child_target(renderer, picture, size)?;
    let mixed = mandatory_child_target(renderer, resize, size)?;
    Ok((picture, mixed))
}
