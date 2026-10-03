//! Final native pane placement and native-size retirement in production order.
use super::super::super::composition_targets::{native_content_target, prepare_frame_targets};
use super::super::super::full_render::place_pane_composition;
use super::native_boundary::full_plan;
use super::*;
use crate::render_thread::frame_compositor::continuity::pane_layout::PixelGrid;
use neomacs_display_protocol::types::Rect;
use neomacs_renderer_wgpu::renderer::{NativeContentPlacement, RenderTarget};

pub(super) fn pane_frame() -> FrameGlyphBuffer {
    let mut root = picture(42, 0, W as f32);
    root.height = H as f32;
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
    root
}
fn pane_specs(render: &mut GuiFrameRenderState) {
    render.compositor.pane_motion = crate::render_thread::render_quality::WindowAnimationSpecs {
        resize: linear_second(),
        movement: linear_second(),
        open: linear_second(),
        close: linear_second(),
    };
}
pub(super) fn install_pane_change(
    render: &mut GuiFrameRenderState,
    origin: neomacs_display_protocol::frame_time::EventTime,
    kind: usize,
) -> (FrameGlyphBuffer, FrameGlyphBuffer) {
    pane_specs(render);
    let single = vec![window(1, Rect::new(0.0, 0.0, W as f32, 48.0))];
    let split = vec![
        window(1, Rect::new(0.0, 0.0, 48.0, 48.0)),
        window(2, Rect::new(48.0, 0.0, 48.0, 48.0)),
    ];
    let mut before = pane_frame();
    before.background_alpha = 0.5;
    before.window_infos = if kind == 0 {
        single.clone()
    } else {
        split.clone()
    };
    let mut after = before.clone();
    after.background_alpha = 1.0; // RGB, buffers and window starts are unchanged.
    after.window_infos = match kind {
        0 => split,
        1 => single,
        _ => vec![
            window(1, Rect::new(0.0, 0.0, 64.0, 48.0)),
            window(2, Rect::new(64.0, 0.0, 32.0, 48.0)),
        ],
    };
    install_layout(render, &before, origin);
    install_layout(render, &after, origin);
    assert!(
        render
            .compositor
            .current_frame
            .as_ref()
            .unwrap()
            .transition_hints
            .is_empty()
    );
    assert!(!render.compositor.transitions.has_active());
    (before, after)
}

#[test]
fn fractional_outgoing_panes_use_final_native_conversion_and_settle_direct() {
    let mut renderer = WgpuRenderer::new(None, W, H).expect("real GPU required");
    let size = SnapshotSize::new(W, H).unwrap();
    for kind in 0..3 {
        let origin = observe_platform_now();
        let mut render = GuiFrameRenderState::new(42, renderer.device(), 1.0, false, origin);
        let (before, after) = install_pane_change(&mut render, origin, kind);
        let surface = mapping(&after, W, H).surface();
        let old = render
            .compositor
            .transitions
            .advance_compositions(&mut renderer, size)
            .unwrap();
        draw_root(&mut renderer, &mut render, old.view(), &before);
        drop(old); // The composition ring, not the test, retains this picture.
        let out = target(&renderer);
        let view = out.create_view(&Default::default());
        let current = render.compositor.current_frame.clone();
        let interaction = format!("{:?}", render.compositor.interaction);
        let mut witnessed_fractional = false;
        for millis in [630, 794] {
            renderer.set_frame_sample(FrameSample::new(
                origin.plus(Duration::from_millis(millis)),
                Duration::ZERO,
            ));
            // Pending and active ticks enter the same actual preflight. Neither
            // may drain the payload or sample/publish an interactive placement.
            let native = prepare_frame_targets(&mut renderer, &mut render, surface, &full_plan())
                .unwrap()
                .native;
            assert_eq!(render.compositor.current_frame, current);
            assert_eq!(format!("{:?}", render.compositor.interaction), interaction);
            assert!(render.compositor.layout.wants_frames());
            let destination = native.as_ref().map_or(&view, |p| p.view());
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
            place_pane_composition(
                &mut renderer,
                &mut render,
                &new,
                destination,
                (W as f32, H as f32),
                &composition.blits,
            );
            if let Some(native) = &native {
                renderer.place_native_content_with_opacity(
                    NativeContentPlacement::new(RenderTarget::new(&view, surface), native).unwrap(),
                    after.background,
                    1.0,
                );
            }
            let data = pixels(&renderer, &out);
            let weight = composition
                .blits
                .iter()
                .find(|pane| {
                    pane.source == neomacs_renderer_wgpu::PaneSource::Previous
                        && pane.content_origin == (0.0, 0.0)
                })
                .unwrap()
                .opacity;
            let expected_alpha = (255.0 * (1.0 - 0.5 * weight)).round() as u8;
            assert!(
                px(&data, 20, 40)[3].abs_diff(expected_alpha) <= 2,
                "actual complementary picture weight {weight}"
            );
            for x in 2..W - 2 {
                let bg = px(&data, x, 40);
                witnessed_fractional |= bg[3] < 250;
                assert!(
                    bg[1].abs_diff(bg[3]) <= 2,
                    "native pane kind{kind} {millis} x{x}: {bg:?}; conversion={}",
                    native.is_some()
                );
            }
            assert_eq!(
                px(&data, 80, 60),
                [0, 255, 0, 255],
                "outside panes remains newly opaque"
            );
            assert_eq!(
                px(&data, 20, 20),
                [255; 4],
                "complete opaque foreground survives picture interpolation"
            );
            eprintln!(
                "native panes kind{kind} {millis}: center {:?} blits {:?}",
                px(&data, 20, 40),
                composition.blits
            );
            assert!(
                native.is_some(),
                "outgoing retained picture requires charged conversion"
            );
            drop(new);
        }
        assert!(witnessed_fractional);
        renderer.set_frame_sample(FrameSample::new(
            origin.plus(Duration::from_secs(2)),
            Duration::ZERO,
        ));
        assert!(
            prepare_frame_targets(&mut renderer, &mut render, surface, &full_plan())
                .unwrap()
                .native
                .is_none(),
            "terminal zero-weight patches need no native conversion before settlement"
        );
        let settled = render.sample_pane_layout(
            &super::super::super::surface::SurfaceAcquired::for_test(),
            renderer.frame_sample(),
            PixelGrid::new(1.0),
        );
        assert!(
            settled.projection.is_some(),
            "settlement still carries interactive geometry"
        );
        assert!(!render.compositor.layout.wants_frames());
        assert!(
            native_content_target(&mut renderer, &mut render, surface, &full_plan())
                .unwrap()
                .is_none()
        );
        draw_root(&mut renderer, &mut render, &view, &after);
        assert_eq!(px(&pixels(&renderer, &out), 20, 40), [0, 255, 0, 255]);
    }
}

#[test]
fn pane_conversion_refuses_before_motion_and_recovers_under_held_pressure() {
    if std::env::var("NEOMACS_GPU_BUDGET_MB").as_deref() != Ok("1") {
        let status=std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact","render_thread::render_pass::scene::tests::pane_size_boundary::pane_conversion_refuses_before_motion_and_recovers_under_held_pressure","--nocapture"]).env("NEOMACS_GPU_BUDGET_MB","1").status().unwrap();
        assert!(status.success());
        return;
    }
    for active in [false, true] {
        let mut renderer = WgpuRenderer::new(None, W, H).expect("real GPU required");
        let origin = observe_platform_now();
        let mut render = GuiFrameRenderState::new(42, renderer.device(), 1.0, false, origin);
        let (before, after) = install_pane_change(&mut render, origin, 1);
        let surface = mapping(&after, W, H).surface();
        let old = render
            .compositor
            .transitions
            .advance_compositions(&mut renderer, SnapshotSize::new(W, H).unwrap())
            .unwrap();
        draw_root(&mut renderer, &mut render, old.view(), &before);
        drop(old);
        // Seed the genuinely required native owner while the old background is applied.
        render.compositor.current_frame = Some(before.clone());
        let native = native_content_target(&mut renderer, &mut render, surface, &full_plan())
            .unwrap()
            .unwrap();
        let id = native.id();
        drop(native);
        render.compositor.current_frame = Some(after.clone());
        render.set_current_frame(
            Some(after.clone()),
            None,
            Default::default(),
            Default::default(),
        );
        if active {
            renderer.set_frame_sample(FrameSample::new(
                origin.plus(Duration::from_millis(250)),
                Duration::ZERO,
            ));
            let native = native_content_target(&mut renderer, &mut render, surface, &full_plan())
                .unwrap()
                .unwrap();
            let composition = render.sample_pane_layout(
                &super::super::super::surface::SurfaceAcquired::for_test(),
                renderer.frame_sample(),
                PixelGrid::new(1.0),
            );
            let new = render
                .compositor
                .transitions
                .advance_compositions(&mut renderer, SnapshotSize::new(W, H).unwrap())
                .unwrap();
            draw_root(&mut renderer, &mut render, new.view(), &after);
            place_pane_composition(
                &mut renderer,
                &mut render,
                &new,
                native.view(),
                (W as f32, H as f32),
                &composition.blits,
            );
            drop(new);
            drop(native);
        }
        let child = picture(43, 42, 64.0);
        assert!(render.compositor.child_frames.update_frame(child));
        let mut controls = crate::thread_comm::FrameOpacityState::default();
        controls.accept(43, [0.5; 2], 0.0);
        render.apply_frame_opacity(&controls);
        let pressure = renderer
            .acquire_snapshot(SnapshotSize::new(512, if active { 471 } else { 483 }).unwrap())
            .unwrap();
        let current = render.compositor.current_frame.clone();
        let interaction = format!("{:?}", render.compositor.interaction);
        for millis in [if active { 500 } else { 0 }, 630] {
            renderer.set_frame_sample(FrameSample::new(
                origin.plus(Duration::from_millis(millis)),
                Duration::ZERO,
            ));
            eprintln!(
                "pressure at{millis}: layout={} child={} native={:?} pooled={} unpooled={}",
                render.compositor.layout.wants_frames(),
                render.compositor.child_frames.frames[&43].applied_frame_alpha,
                render.native_content_src.as_ref().map(|p| p.id()),
                renderer.gpu_budget().pooled_bytes(),
                renderer.gpu_budget().unpooled_bytes()
            );
            assert!(
                matches!(
                    prepare_frame_targets(&mut renderer, &mut render, surface, &full_plan()),
                    Err(super::super::super::surface::FrameRenderFailure::WindowNotReady)
                ),
                "pane's genuine native owner cannot be retired to admit child"
            );
            assert_eq!(render.native_content_src.as_ref().unwrap().id(), id);
            assert_eq!(render.compositor.current_frame, current);
            assert_eq!(format!("{:?}", render.compositor.interaction), interaction);
            assert!(render.compositor.layout.wants_frames());
            assert!(render.child_opacity_src.is_none() && render.child_resize_src.is_none());
        }
        renderer.set_frame_sample(FrameSample::new(
            origin.plus(Duration::from_secs(2)),
            Duration::ZERO,
        ));
        let targets = prepare_frame_targets(&mut renderer, &mut render, surface, &full_plan())
            .expect(
                "terminal pane must not strand obsolete native ownership under unchanged pressure",
            );
        assert!(targets.native.is_none());
        assert!(targets.picture.is_some());
        assert!(
            render.compositor.layout.wants_frames(),
            "preflight must not itself publish settlement"
        );
        assert_eq!(format!("{:?}", render.compositor.interaction), interaction);
        let composition = render.sample_pane_layout(
            &super::super::super::surface::SurfaceAcquired::for_test(),
            renderer.frame_sample(),
            PixelGrid::new(1.0),
        );
        assert!(composition.projection.is_some());
        assert!(!render.compositor.layout.wants_frames());
        let out = target(&renderer);
        let view = out.create_view(&Default::default());
        render.child_opacity_src = targets.picture;
        draw_root(&mut renderer, &mut render, &view, &after);
        let style = ChildFrameStyle {
            corner_radius: 0.0,
            shadow_enabled: false,
            shadow_layers: 0,
            shadow_offset: 0.0,
            shadow_opacity: 0.0,
        };
        render_frame_content_overlays(
            &mut renderer,
            (W, H),
            &mut render,
            &view,
            &after,
            false,
            None,
            &style,
            false,
        );
        assert_eq!(
            px(&pixels(&renderer, &out), 48, 40)[3],
            255,
            "required child remains rendered over settled opaque root"
        );
        render.child_opacity_src = None;
        drop(pressure);
    }
}

#[test]
fn wrong_size_required_native_owner_retires_before_child_under_same_pressure() {
    if std::env::var("NEOMACS_GPU_BUDGET_MB").as_deref() != Ok("1") {
        let status=std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact","render_thread::render_pass::scene::tests::pane_size_boundary::wrong_size_required_native_owner_retires_before_child_under_same_pressure","--nocapture"]).env("NEOMACS_GPU_BUDGET_MB","1").status().unwrap();
        assert!(status.success());
        return;
    }
    let mut renderer = WgpuRenderer::new(None, W, H).expect("real GPU required");
    let origin = observe_platform_now();
    let mut render = GuiFrameRenderState::new(42, renderer.device(), 1.0, false, origin);
    let mut root = pane_frame();
    root.background_alpha = 0.5;
    render.set_current_frame(
        Some(root.clone()),
        None,
        Default::default(),
        Default::default(),
    );
    let old_surface = mapping(&root, 128, H).surface();
    let old = prepare_frame_targets(&mut renderer, &mut render, old_surface, &full_plan())
        .unwrap()
        .native
        .unwrap();
    draw_root(&mut renderer, &mut render, old.view(), &root);
    let old_id = old.id();
    assert_eq!(old.size(), SnapshotSize::new(128, H).unwrap());
    drop(old);
    let child = picture(43, 42, 64.0);
    render.compositor.child_frames.update_frame(child);
    let mut controls = crate::thread_comm::FrameOpacityState::default();
    controls.accept(43, [0.5; 2], 0.0);
    render.apply_frame_opacity(&controls);
    // Held ownership is pressure 989184 plus the renderer's charged stencil
    // 6144: 995328 held + 32768 old + 24576 child exceeds 1048576;
    // 995328 held + two 24576 replacement owners fits, continuously.
    let pressure = renderer
        .acquire_snapshot(SnapshotSize::new(512, 483).unwrap())
        .unwrap();
    let current = render.compositor.current_frame.clone();
    let interaction = format!("{:?}", render.compositor.interaction);
    assert!(
        matches!(
            prepare_frame_targets(&mut renderer, &mut render, old_surface, &full_plan()),
            Err(super::super::super::surface::FrameRenderFailure::WindowNotReady)
        ),
        "correctly sized required root must not be surrendered to child"
    );
    assert_eq!(render.native_content_src.as_ref().unwrap().id(), old_id);
    let surface = mapping(&root, W, H).surface();
    let targets = prepare_frame_targets(&mut renderer, &mut render, surface, &full_plan())
        .expect("wrong-size native owner must retire BEFORE mandatory child admission");
    assert_eq!(
        targets.native.as_ref().unwrap().size(),
        SnapshotSize::new(W, H).unwrap()
    );
    assert_eq!(
        targets.picture.as_ref().unwrap().size(),
        SnapshotSize::new(W, H).unwrap()
    );
    assert_ne!(
        targets.picture.as_ref().unwrap().id(),
        targets.native.as_ref().unwrap().id()
    );
    assert_eq!(render.compositor.current_frame, current);
    assert_eq!(format!("{:?}", render.compositor.interaction), interaction);
    render.child_opacity_src = targets.picture;
    let native = targets.native.unwrap();
    draw_root(&mut renderer, &mut render, native.view(), &root);
    let style = ChildFrameStyle {
        corner_radius: 0.0,
        shadow_enabled: false,
        shadow_layers: 0,
        shadow_offset: 0.0,
        shadow_opacity: 0.0,
    };
    render_frame_content_overlays(
        &mut renderer,
        (W, H),
        &mut render,
        native.view(),
        &root,
        false,
        None,
        &style,
        false,
    );
    let out = target(&renderer);
    let view = out.create_view(&Default::default());
    renderer.place_native_content_with_opacity(
        NativeContentPlacement::new(RenderTarget::new(&view, surface), &native).unwrap(),
        root.background,
        1.0,
    );
    let bg = px(&pixels(&renderer, &out), 48, 40);
    assert!(
        bg[1].abs_diff(bg[3]) <= 2 && (190..=192).contains(&bg[3]),
        "root and required child actually drawn after shrink: {bg:?}"
    );
    render.child_opacity_src = None;
    drop(native);
    let retry = prepare_frame_targets(&mut renderer, &mut render, surface, &full_plan()).unwrap();
    assert!(retry.native.is_some() && retry.picture.is_some());
    drop(retry);
    drop(pressure);
}
