//! The offscreen textures a frame composes *through*, and the accounting for
//! the ones the pool does not own.
//!
//! Owns: the frame-post source lease, the composition-ring rotation that hands
//! a transition or a pane morph somewhere to draw before it is placed, the
//! single place a refused GPU-budget lease is counted and logged, and the
//! per-frame census of this window's unpooled full-frame textures.
//!
//! Must not: draw into any of them, or decide that a frame needs one. Every
//! function here answers "may I have a texture this size?", and every `None`
//! it returns is a real GPU-pressure state the caller degrades gracefully
//! around — one unshaded or un-transitioned frame, never a dropped one and
//! never a masked failure, which is why refusals are counted rather than
//! swallowed.

use crate::render_thread::frame_stats;
use crate::render_thread::frame_windows::GuiFrameRenderState;
use neomacs_renderer_wgpu::{
    BudgetExceeded, GpuBudgetOwner, SnapshotLease, SnapshotSize, UnpooledTexture, WgpuGlyphAtlas,
    WgpuRenderer, texture_bytes,
};

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
    let retained_static_bytes = render
        .compositor
        .retained_static
        .as_ref()
        .and_then(|retained| SnapshotSize::new(retained.width, retained.height))
        .map_or(0, |size| texture_bytes(size, renderer.surface_format()));
    renderer.record_unpooled_texture(
        owner,
        UnpooledTexture::RetainedStaticScene,
        retained_static_bytes,
    );
    let atlas_bytes = render
        .compositor
        .glyph_atlas
        .as_ref()
        .map_or(0, WgpuGlyphAtlas::resident_bytes);
    renderer.record_unpooled_texture(owner, UnpooledTexture::GlyphAtlas, atlas_bytes);
    let budget = renderer.gpu_budget();
    tracing::trace!(
        ?owner,
        pooled_bytes = budget.pooled_bytes(),
        unpooled_bytes = budget.unpooled_bytes(),
        limit_bytes = budget.limit_bytes().get(),
        "full-frame GPU texture accounting"
    );
}
