//! Real pooled/unpooled ownership through the production mandatory boundary.
use super::super::super::{composition_targets as targets, retained_scroll, retained_static};
use super::native_boundary::full_plan;
use super::pane_size_boundary::{install_pane_change, pane_frame};
use super::*;
use crate::render_thread::frame_compositor::continuity::pane_layout::PixelGrid;
use crate::render_thread::frame_compositor::layout_driver::{LayoutDriver, OutgoingPicture};
use crate::render_thread::transitions::{detect_frame_transitions, render_frame_transitions};
use neomacs_display_protocol::frame_glyphs::{
    BufferTransitionTarget, ContentTransitionHint, PresentedWindowRegions,
};
use neomacs_display_protocol::{ContentTransitionIntent, TransitionEffect};
use neomacs_renderer_wgpu::SnapshotLease;
use neomacs_renderer_wgpu::renderer::{NativeContentPlacement, RenderTarget};

fn budget_one(name: &str) -> bool {
    if std::env::var("NEOMACS_GPU_BUDGET_MB").as_deref() == Ok("1") {
        return true;
    }
    let exact = format!("render_thread::render_pass::scene::tests::retained_owner::{name}");
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", &exact, "--nocapture"])
        .env("NEOMACS_GPU_BUDGET_MB", "1")
        .status()
        .unwrap();
    assert!(status.success());
    false
}

fn setup() -> (WgpuRenderer, GuiFrameRenderState, FrameGlyphBuffer) {
    assert_eq!(std::env::var("NEOMACS_GPU_BUDGET_MB").as_deref(), Ok("1"));
    let renderer = WgpuRenderer::new(None, W, H).expect("real Vulkan GPU required");
    let mut render =
        GuiFrameRenderState::new(42, renderer.device(), 1.0, false, observe_platform_now());
    let root = pane_frame();
    render.set_current_frame(
        Some(root.clone()),
        None,
        Default::default(),
        Default::default(),
    );
    (renderer, render, root)
}
fn child(render: &mut GuiFrameRenderState, alpha: f32) {
    if !render.compositor.child_frames.frames.contains_key(&43) {
        assert!(
            render
                .compositor
                .child_frames
                .update_frame(picture(43, 42, 64.0))
        );
    }
    let mut controls = crate::thread_comm::FrameOpacityState::default();
    controls.accept(43, [alpha; 2], 0.0);
    render.apply_frame_opacity(&controls);
    assert_eq!(
        render.compositor.child_frames.frames[&43].applied_frame_alpha,
        alpha
    );
}
fn hold(renderer: &mut WgpuRenderer, reserve: u64) -> SnapshotLease {
    let b = renderer.gpu_budget();
    let free = b.limit_bytes().get() - b.pooled_bytes() - b.unpooled_bytes();
    assert!(free > reserve);
    let rows = ((free - reserve) / 1024) as u32;
    let lease = renderer
        .acquire_snapshot(SnapshotSize::new(256, rows).unwrap())
        .expect("qualified pressure fixture");
    let b = renderer.gpu_budget();
    eprintln!(
        "held pressure: pooled={} unpooled={} limit={} pressure={}",
        b.pooled_bytes(),
        b.unpooled_bytes(),
        b.limit_bytes().get(),
        256 * rows * 4
    );
    lease
}
fn refused(
    renderer: &mut WgpuRenderer,
    render: &mut GuiFrameRenderState,
    root: &FrameGlyphBuffer,
    plan: &crate::render_thread::render_quality::RenderFeaturePlan,
) {
    let current = render.compositor.current_frame.clone();
    let interaction = format!("{:?}", render.compositor.interaction);
    let scroll = render
        .compositor
        .input_scroll
        .staged_projection()
        .map(|(s, o)| (s.coverage().epoch, o));
    let pending = (
        render.compositor.pending.scrolls.len(),
        render.compositor.pending.reflows.len(),
        render.compositor.pending.theme.is_some(),
    );
    for _ in 0..2 {
        assert!(
            matches!(
                targets::prepare_frame_targets(
                    renderer,
                    render,
                    mapping(root, W, H).surface(),
                    plan
                ),
                Err(super::super::super::surface::FrameRenderFailure::WindowNotReady)
            ),
            "live needed ownership must refuse typed"
        );
        assert_eq!(render.compositor.current_frame, current);
        assert_eq!(format!("{:?}", render.compositor.interaction), interaction);
        assert_eq!(
            render
                .compositor
                .input_scroll
                .staged_projection()
                .map(|(s, o)| (s.coverage().epoch, o)),
            scroll
        );
        assert_eq!(
            (
                render.compositor.pending.scrolls.len(),
                render.compositor.pending.reflows.len(),
                render.compositor.pending.theme.is_some()
            ),
            pending
        );
    }
}
fn admitted(
    renderer: &mut WgpuRenderer,
    render: &mut GuiFrameRenderState,
    root: &FrameGlyphBuffer,
    plan: &crate::render_thread::render_quality::RenderFeaturePlan,
) -> targets::FrameTargets {
    let current = render.compositor.current_frame.clone();
    let interaction = format!("{:?}", render.compositor.interaction);
    let scroll = render
        .compositor
        .input_scroll
        .staged_projection()
        .map(|(s, o)| (s.coverage().epoch, o));
    let result =
        targets::prepare_frame_targets(renderer, render, mapping(root, W, H).surface(), plan)
            .expect(
                "obsolete owner must retire before mandatory admission under SAME held pressure",
            );
    assert!(result.picture.is_some());
    assert_eq!(
        render
            .compositor
            .input_scroll
            .staged_projection()
            .map(|(s, o)| (s.coverage().epoch, o)),
        scroll
    );
    assert_eq!(render.compositor.current_frame, current);
    assert_eq!(format!("{:?}", render.compositor.interaction), interaction);
    result
}
fn present(
    renderer: &mut WgpuRenderer,
    render: &mut GuiFrameRenderState,
    root: &FrameGlyphBuffer,
    result: targets::FrameTargets,
) {
    let out = target(renderer);
    let view = out.create_view(&Default::default());
    let destination = result.native.as_ref().map_or(&view, |native| native.view());
    draw_root(renderer, render, destination, root);
    render.child_opacity_src = result.picture;
    render.child_resize_src = result.resize;
    let style = ChildFrameStyle {
        corner_radius: 0.0,
        shadow_enabled: false,
        shadow_layers: 0,
        shadow_offset: 0.0,
        shadow_opacity: 0.0,
    };
    render_frame_content_overlays(
        renderer,
        (W, H),
        render,
        destination,
        root,
        false,
        None,
        &style,
        false,
    );
    if let Some(native) = &result.native {
        renderer.place_native_content_with_opacity(
            NativeContentPlacement::new(
                RenderTarget::new(&view, mapping(root, W, H).surface()),
                native,
            )
            .unwrap(),
            root.background,
            render.applied_frame_alpha,
        );
    }
    let p = px(&pixels(renderer, &out), 48, 30);
    eprintln!("recovered required-child production pixels {p:?}");
    let expected = if root.background_alpha == 1.0 {
        255_u8
    } else {
        191_u8
    };
    assert!(
        p[3].abs_diff(expected) <= 2,
        "live required child must really draw: {p:?}"
    );
    render.child_opacity_src = None;
    render.child_resize_src = None;
}
fn post(shrink: bool) {
    let (mut renderer, mut render, root) = setup();
    let old_size = SnapshotSize::new(if shrink { 128 } else { W }, H).unwrap();
    let view = targets::ensure_frame_post_src(&mut renderer, &mut render, old_size).unwrap();
    draw_root(&mut renderer, &mut render, &view, &root);
    drop(view);
    child(&mut render, 0.5);
    let mut plan = full_plan();
    plan.apply_frame_post = true;
    let pressure = hold(&mut renderer, u64::from(W * H * 2));
    if !shrink {
        refused(&mut renderer, &mut render, &root, &plan);
        plan.apply_frame_post = false;
    }
    let result = admitted(&mut renderer, &mut render, &root, &plan);
    assert!(render.frame_post_src.is_none());
    present(&mut renderer, &mut render, &root, result);
    drop(pressure);
}
#[test]
fn disabled_post_retires_under_held_pressure() {
    if !budget_one("disabled_post_retires_under_held_pressure") {
        return;
    }
    post(false)
}
#[test]
fn wrong_size_post_retires_under_held_pressure() {
    if !budget_one("wrong_size_post_retires_under_held_pressure") {
        return;
    }
    post(true)
}

fn static_scene(shrink: bool) {
    let (mut renderer, mut render, root) = setup();
    child(&mut render, 0.5);
    retained_static::ensure_retained_static_texture(
        &renderer,
        &mut render,
        SnapshotSize::new(if shrink { 128 } else { W }, H).unwrap(),
    );
    let view = render
        .compositor
        .retained_static
        .as_ref()
        .unwrap()
        .texture
        .view()
        .clone();
    draw_root(&mut renderer, &mut render, &view, &root);
    drop(view);
    render
        .compositor
        .retained_static
        .as_mut()
        .unwrap()
        .generation = render.compositor.current_scene_generation;
    targets::report_unpooled_gpu_textures(&mut renderer, &render);
    assert!(
        renderer.gpu_budget().unpooled_bytes() >= u64::from(if shrink { 128 } else { W } * H * 4)
    );
    let pressure = hold(&mut renderer, u64::from(W * H * 2));
    if !shrink {
        refused(&mut renderer, &mut render, &root, &full_plan());
        child(&mut render, 0.7);
    }
    let result = admitted(&mut renderer, &mut render, &root, &full_plan());
    assert!(render.compositor.retained_static.is_none());
    assert_eq!(renderer.gpu_budget().unpooled_bytes(), u64::from(W * H)); // live stencil, not stale census
    present(&mut renderer, &mut render, &root, result);
    drop(pressure);
}
#[test]
fn stale_static_generation_and_census_retire_under_held_pressure() {
    if !budget_one("stale_static_generation_and_census_retire_under_held_pressure") {
        return;
    }
    static_scene(false)
}
#[test]
fn stale_static_extent_and_census_retire_under_held_pressure() {
    if !budget_one("stale_static_extent_and_census_retire_under_held_pressure") {
        return;
    }
    static_scene(true)
}

fn snapshot(fractional: bool) {
    let (mut renderer, mut render, mut root) = setup();
    if fractional {
        root.background_alpha = 0.5;
        render.compositor.current_frame = Some(root.clone());
    }
    let origin = observe_platform_now();
    renderer.set_frame_sample(FrameSample::new(origin, Duration::ZERO));
    render.compositor.transitions.policy.buffer.enabled = true;
    render.compositor.transitions.policy.buffer.duration = Duration::from_secs(1);
    render.compositor.transitions.policy.buffer.effect = TransitionEffect::Crossfade;
    let size = SnapshotSize::new(W, H).unwrap();
    for _ in 0..2 {
        let pic = render
            .compositor
            .transitions
            .advance_compositions(&mut renderer, size)
            .unwrap();
        draw_root(&mut renderer, &mut render, pic.view(), &root);
    }
    root.transition_hints
        .push(ContentTransitionHint::BufferReplaced {
            target: BufferTransitionTarget::Frame {
                regions: vec![
                    PresentedWindowRegions {
                        text_body: neomacs_display_protocol::Rect::new(
                            0.0, 0.0, W as f32, H as f32,
                        ),
                        ..Default::default()
                    }
                    .buffer_viewport()
                    .unwrap(),
                ],
            },
            intent: ContentTransitionIntent::Replace,
        });
    detect_frame_transitions(
        &mut renderer,
        &mut render.compositor.transitions,
        &Default::default(),
        &mut root,
        Default::default(),
        &mut render.compositor.dirty,
    );
    assert!(render.compositor.transitions.has_active());
    render.compositor.current_frame = Some(root.clone());
    // Age both ring slots away from the transition's unique old picture.
    for _ in 0..if fractional { 0 } else { 3 } {
        let pic = render
            .compositor
            .transitions
            .advance_compositions(&mut renderer, size)
            .unwrap();
        draw_root(&mut renderer, &mut render, pic.view(), &root);
    }
    let native = targets::native_content_target(
        &mut renderer,
        &mut render,
        mapping(&root, W, H).surface(),
        &full_plan(),
    )
    .unwrap()
    .unwrap();
    render_frame_transitions(
        &mut renderer,
        &mut render.compositor.transitions,
        native.view(),
        W,
        H,
        &root,
    );
    drop(native);
    child(&mut render, 0.5);
    let pressure = hold(&mut renderer, u64::from(W * H * 2));
    refused(&mut renderer, &mut render, &root, &full_plan());
    renderer.set_frame_sample(FrameSample::new(
        origin.plus(Duration::from_secs(2)),
        Duration::ZERO,
    ));
    if fractional {
        // Old is still aliased by the ring, so expiry cannot free its bytes.
        // A genuinely fractional native owner stays required and refuses.
        let id = render.native_content_src.as_ref().unwrap().id();
        refused(&mut renderer, &mut render, &root, &full_plan());
        assert!(!render.compositor.transitions.has_active());
        assert_eq!(render.native_content_src.as_ref().unwrap().id(), id);
    } else {
        let result = admitted(&mut renderer, &mut render, &root, &full_plan());
        assert!(!render.compositor.transitions.has_active());
        assert!(result.native.is_none());
        present(&mut renderer, &mut render, &root, result);
    }
    drop(pressure);
}
#[test]
fn expired_snapshot_old_and_native_retire_under_held_pressure() {
    if !budget_one("expired_snapshot_old_and_native_retire_under_held_pressure") {
        return;
    }
    snapshot(false)
}
#[test]
fn expired_snapshot_keeps_fractional_native_under_true_pressure() {
    if !budget_one("expired_snapshot_keeps_fractional_native_under_true_pressure") {
        return;
    }
    snapshot(true)
}

#[test]
fn terminal_unique_pane_retires_without_surrendering_fractional_native() {
    if !budget_one("terminal_unique_pane_retires_without_surrendering_fractional_native") {
        return;
    }
    let (mut renderer, mut render, _) = setup();
    let origin = observe_platform_now();
    let (before, mut after) = install_pane_change(&mut render, origin, 1);
    after.background_alpha = 0.5;
    render.compositor.current_frame = Some(after.clone());
    render.set_current_frame(
        Some(after.clone()),
        None,
        Default::default(),
        Default::default(),
    );
    let size = SnapshotSize::new(W, H).unwrap();
    let old = render
        .compositor
        .transitions
        .advance_compositions(&mut renderer, size)
        .unwrap();
    draw_root(&mut renderer, &mut render, old.view(), &before);
    drop(old);
    let native = targets::native_content_target(
        &mut renderer,
        &mut render,
        mapping(&after, W, H).surface(),
        &full_plan(),
    )
    .unwrap()
    .unwrap();
    let native_id = native.id();
    for millis in [100, 200, 300] {
        renderer.set_frame_sample(FrameSample::new(
            origin.plus(Duration::from_millis(millis)),
            Duration::ZERO,
        ));
        let composition = render.sample_pane_layout(
            &super::super::super::surface::SurfaceAcquired::for_test(),
            renderer.frame_sample(),
            PixelGrid::new(1.0),
        );
        let new = render
            .compositor
            .transitions
            .advance_compositions(&mut renderer, size)
            .unwrap();
        draw_root(&mut renderer, &mut render, new.view(), &after);
        super::super::super::full_render::place_pane_composition(
            &mut renderer,
            &mut render,
            &new,
            native.view(),
            (W as f32, H as f32),
            &composition.blits,
        );
        drop(new);
    }
    let old_id = match &render.compositor.layout {
        LayoutDriver::Animating {
            outgoing: OutgoingPicture::Pinned(Some(old)),
            ..
        } => old.id(),
        _ => panic!("real production pin required"),
    };
    let ring = render.compositor.transitions.compositions.as_ref().unwrap();
    assert_ne!(old_id, ring.current().id());
    assert_ne!(old_id, ring.previous().unwrap().id());
    drop(native);
    child(&mut render, 0.5);
    let pressure = hold(&mut renderer, u64::from(W * H * 2));
    refused(&mut renderer, &mut render, &after, &full_plan());
    renderer.set_frame_sample(FrameSample::new(
        origin.plus(Duration::from_secs(2)),
        Duration::ZERO,
    ));
    let result = admitted(&mut renderer, &mut render, &after, &full_plan());
    assert_eq!(result.native.as_ref().unwrap().id(), native_id);
    assert!(matches!(
        &render.compositor.layout,
        LayoutDriver::Animating {
            outgoing: OutgoingPicture::Pinned(None),
            ..
        }
    ));
    let settled = render.sample_pane_layout(
        &super::super::super::surface::SurfaceAcquired::for_test(),
        renderer.frame_sample(),
        PixelGrid::new(1.0),
    );
    assert!(settled.projection.is_some());
    present(&mut renderer, &mut render, &after, result);
    drop(pressure);
}

fn scroll(dead: bool) {
    let (mut renderer, mut render, _) = setup();
    let mut root = retained_scroll::tests::fixture();
    root.set_frame_identity(
        DisplayFrameId::new(42),
        DisplayFrameId::new(0),
        0.0,
        0.0,
        0,
        false,
        0.0,
        Color::BLACK,
        false,
        1.0,
    );
    render.set_current_frame(
        Some(root.clone()),
        None,
        Default::default(),
        Default::default(),
    );
    let stream = neomacs_display_protocol::input_progress::InputStream::default();
    let delivery = stream.issue().unwrap();
    assert!(
        render
            .compositor
            .input_scroll
            .push(&root, 11.0, 11.0, 4.0, delivery.receipt(), None)
    );
    render.compositor.input_scroll.paint(&mut root);
    let prepared = retained_scroll::prepare(
        &mut renderer,
        &mut render,
        &root,
        mapping(&root, W, H),
        false,
    )
    .expect("actual warmed scroll raster");
    drop(prepared);
    render.compositor.current_frame = Some(root.clone());
    child(&mut render, 0.5);
    let pressure = hold(&mut renderer, u64::from(W * H * 2));
    refused(&mut renderer, &mut render, &root, &full_plan());
    if dead {
        root.scroll_surfaces.clear();
        render.compositor.input_scroll = Default::default();
    } else {
        render.overlays.idle_dim.active = true;
    }
    render.compositor.current_frame = Some(root.clone());
    let result = admitted(&mut renderer, &mut render, &root, &full_plan());
    assert!(render.compositor.retained_scroll.is_none());
    present(&mut renderer, &mut render, &root, result);
    drop(pressure);
    drop(delivery);
}
#[test]
fn dead_scroll_epoch_retires_under_held_pressure() {
    if !budget_one("dead_scroll_epoch_retires_under_held_pressure") {
        return;
    }
    scroll(true)
}
#[test]
fn ineligible_scroll_overlay_retires_under_held_pressure() {
    if !budget_one("ineligible_scroll_overlay_retires_under_held_pressure") {
        return;
    }
    scroll(false)
}
