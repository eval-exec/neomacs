use super::*;

fn budget(limit_bytes: u64) -> GpuBudget {
    GpuBudget::with_limit_bytes(NonZeroU64::new(limit_bytes).expect("non-zero test limit"))
}

fn window(frame_id: u64) -> GpuBudgetOwner {
    GpuBudgetOwner::FrameWindow(frame_id)
}

#[test]
fn the_budget_refuses_a_pooled_charge_that_would_cross_the_ceiling_rather_than_taking_it() {
    // If this fails, `GpuBudget` is decoration: the render thread can keep
    // allocating full-frame textures until the GPU runs out.
    let mut budget = budget(100);
    assert!(budget.try_charge_pooled(60).is_ok());
    let refused = budget
        .try_charge_pooled(60)
        .expect_err("60 more bytes do not fit under a 100-byte ceiling with 60 in use");
    assert_eq!(refused.requested_bytes, 60);
    assert_eq!(refused.in_use_bytes, 60);
    assert_eq!(budget.pooled_bytes(), 60, "a refusal must charge nothing");
}

#[test]
fn glyph_atlas_bytes_count_against_the_same_ceiling_as_pool_slots() {
    // If this fails the budget is fictional: one window's atlas can reach
    // 288 MiB against about 59 MiB for every full-frame texture combined, so
    // a ceiling that ignores it constrains the wrong thing entirely.
    let mut budget = budget(100);
    budget.record_unpooled(window(1), UnpooledTexture::GlyphAtlas, 80);
    assert_eq!(budget.in_use_bytes(), 80);
    budget
        .try_charge_pooled(40)
        .expect_err("the atlas leaves only 20 bytes of headroom");
}

#[test]
fn the_retained_static_scene_counts_against_the_ceiling_even_though_it_is_not_pooled() {
    // If this fails, a full-frame texture the render thread genuinely owns is
    // invisible to the only place that decides whether more memory may be
    // taken.
    let mut budget = budget(100);
    budget.record_unpooled(window(1), UnpooledTexture::RetainedStaticScene, 70);
    budget
        .try_charge_pooled(40)
        .expect_err("the retained scene leaves only 30 bytes of headroom");
}

#[test]
fn re_reporting_one_unpooled_texture_replaces_its_figure_instead_of_adding_to_it() {
    // If this fails, the per-frame census inflates without bound and the
    // budget starts refusing leases for memory nothing holds — the exact
    // failure mode a census has to be designed against.
    let mut budget = budget(1000);
    for _ in 0..10 {
        budget.record_unpooled(window(1), UnpooledTexture::GlyphAtlas, 50);
    }
    assert_eq!(budget.unpooled_bytes(), 50);
    budget.record_unpooled(window(1), UnpooledTexture::GlyphAtlas, 0);
    assert_eq!(budget.unpooled_bytes(), 0);
}

#[test]
fn two_frame_windows_add_to_the_census_rather_than_overwriting_each_other() {
    // If this fails, a second window's atlas and retained scene are free as
    // far as the budget is concerned, which is how a multi-window session
    // would exhaust GPU memory with the ceiling reporting plenty of room.
    let mut budget = budget(1000);
    budget.record_unpooled(window(1), UnpooledTexture::GlyphAtlas, 50);
    budget.record_unpooled(window(2), UnpooledTexture::GlyphAtlas, 70);
    assert_eq!(budget.unpooled_bytes(), 120);
}

#[test]
fn a_destroyed_frame_window_stops_being_charged_for_the_textures_it_took_with_it() {
    // If this fails, closing windows ratchets the census upward for the life
    // of the process and eventually every transition degrades.
    let mut budget = budget(1000);
    budget.record_unpooled(window(7), UnpooledTexture::GlyphAtlas, 50);
    budget.record_unpooled(window(7), UnpooledTexture::RetainedStaticScene, 30);
    budget.record_unpooled(window(8), UnpooledTexture::GlyphAtlas, 11);
    budget.forget_frame_window(7);
    assert_eq!(budget.unpooled_bytes(), 11);
}

#[test]
fn refunding_a_pooled_texture_returns_exactly_what_it_cost() {
    // If this fails, the pool's side of the budget drifts upward across
    // eviction cycles until it refuses every lease.
    let mut budget = budget(100);
    budget.try_charge_pooled(40).expect("40 fits under 100");
    budget.refund_pooled(40);
    assert_eq!(budget.pooled_bytes(), 0);
    budget
        .try_charge_pooled(100)
        .expect("the whole ceiling is free again");
}

#[test]
fn the_shared_stencil_target_stays_charged_when_the_window_that_last_sized_it_closes() {
    // The stencil clip target is one texture for the whole renderer, resized
    // to whichever window is being drawn. If it were charged to that window,
    // closing the window would refund megabytes that are still allocated and
    // about to be resized for the next window — and the budget would hand out
    // pool leases against memory the GPU has already given away.
    let mut budget = budget(1000);
    budget.record_unpooled(window(7), UnpooledTexture::GlyphAtlas, 50);
    budget.record_unpooled(GpuBudgetOwner::Renderer, UnpooledTexture::StencilClip, 30);

    budget.forget_frame_window(7);

    assert_eq!(budget.unpooled_bytes(), 30);
}

#[test]
fn a_stencil_clip_target_is_charged_one_byte_per_texel_rather_than_a_colour_texels_four() {
    // Charging it as if it were an RGBA attachment would put four times the
    // real figure under the ceiling, which is the same kind of untrustworthy
    // as leaving it out: a budget is only worth consulting when its numbers
    // are the ones the GPU actually holds.
    let (width, height) = (2560u32, 1440u32);

    assert_eq!(
        crate::renderer::snapshot_pool::texture_bytes(
            crate::renderer::resources::stencil_clip_size(width, height),
            crate::renderer::resources::STENCIL_FORMAT,
        ),
        u64::from(width) * u64::from(height),
    );
}

#[test]
fn a_stencil_clip_target_is_charged_for_the_one_texel_a_degenerate_size_still_allocates() {
    // `StencilTargets` clamps each dimension to at least one because wgpu
    // rejects a zero-sized texture, so a zero-sized window still costs a
    // texel. Reporting zero here would be a figure no texture matches.
    assert_eq!(
        crate::renderer::snapshot_pool::texture_bytes(
            crate::renderer::resources::stencil_clip_size(0, 0),
            crate::renderer::resources::STENCIL_FORMAT,
        ),
        1
    );
}
