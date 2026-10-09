//! Real GPU regression through the runtime child-resize composition branch.

#[path = "native_boundary_test.rs"]
mod native_boundary;
#[path = "pane_size_boundary_test.rs"]
mod pane_size_boundary;
#[path = "pinned_lifecycle_test.rs"]
mod pinned_lifecycle;
#[path = "retained_owner_test.rs"]
mod retained_owner;

fn target(renderer: &WgpuRenderer) -> wgpu::Texture {
    renderer.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("runtime opacity regression target"),
        size: wgpu::Extent3d {
            width: W,
            height: H,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Bgra8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}
fn draw_root(
    renderer: &mut WgpuRenderer,
    render: &mut GuiFrameRenderState,
    view: &wgpu::TextureView,
    root: &FrameGlyphBuffer,
) {
    renderer.render_frame_glyphs(
        view,
        root,
        render.compositor.glyph_atlas.as_mut().unwrap(),
        mapping(root, W, H),
        false,
        None,
        None,
        None,
        None,
        None,
    );
}
fn linear_second() -> MotionSpec {
    MotionSpec::Tween(TweenSpec {
        duration: MotionDuration::new(Duration::from_secs(1)).unwrap(),
        easing: TransitionEasing::Linear,
        bezier: None,
    })
}
fn window(
    id: i64,
    bounds: neomacs_display_protocol::types::Rect,
) -> neomacs_display_protocol::frame_glyphs::WindowInfo {
    neomacs_display_protocol::frame_glyphs::WindowInfo {
        window_id: DisplayWindowId::new(id),
        buffer_id: id as u64 + 100,
        window_start: 1,
        window_end: 100,
        buffer_size: 1000,
        buffer_modiff: neomacs_display_protocol::presentation_origin::BufferModiff::new(1),
        bounds,
        geometry: Default::default(),
        line_number_field: None,
        mode_line_height: 0.0,
        header_line_height: 0.0,
        tab_line_height: 0.0,
        selected: false,
        is_minibuffer: false,
        char_height: 16.0,
        buffer_name: "scratch".into(),
        buffer_file_name: String::new(),
        modified: false,
    }
}
fn install_layout(
    render: &mut GuiFrameRenderState,
    root: &FrameGlyphBuffer,
    origin: neomacs_display_protocol::frame_time::EventTime,
) {
    render.measure_pane_layout(Some(root), origin);
    render.compositor.current_frame = Some(root.clone());
    render.begin_presentable_render();
}
fn seed_pending_layout(
    render: &mut GuiFrameRenderState,
    root: &FrameGlyphBuffer,
    origin: neomacs_display_protocol::frame_time::EventTime,
) {
    use neomacs_display_protocol::types::Rect;
    render.compositor.pane_motion = crate::render_thread::render_quality::WindowAnimationSpecs {
        resize: linear_second(),
        movement: linear_second(),
        open: linear_second(),
        close: linear_second(),
    };
    let mut before = root.clone();
    before.window_infos = vec![window(1, Rect::new(0.0, 0.0, W as f32, 48.0))];
    install_layout(render, &before, origin);
    let mut after = root.clone();
    after.window_infos = vec![window(1, Rect::new(32.0, 0.0, 64.0, 48.0))];
    install_layout(render, &after, origin);
    let composition = render.sample_pane_layout(
        &super::super::surface::SurfaceAcquired::for_test(),
        FrameSample::new(origin.plus(Duration::from_millis(100)), Duration::ZERO),
        crate::render_thread::frame_compositor::continuity::pane_layout::PixelGrid::new(1.0),
    );
    render.publish_submitted_projection(composition.projection);
}

#[test]
fn mixed_policy_child_admission_tracks_applied_highlight_under_held_pressure() {
    if !in_gpu_budget_child(
        "render_thread::render_pass::scene::tests::mixed_policy_child_admission_tracks_applied_highlight_under_held_pressure",
    ) {
        return;
    }
    let mut renderer =
        WgpuRenderer::new(None, W, H).expect("GPU required for applied-alpha admission");
    let origin = observe_platform_now();
    let mut render = GuiFrameRenderState::new(42, renderer.device(), 1.0, false, origin);
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
    root.background_alpha = 0.0;
    render.set_current_frame(
        Some(root.clone()),
        None,
        Default::default(),
        Default::default(),
    );
    let mut child = picture(43, 42, 64.0);
    child.frame_alpha = [1.0, 0.5];
    assert!(render.compositor.child_frames.update_frame(child.clone()));
    // A topmost opaque sibling must remain in its original geometry and z-order.
    let mut sibling = picture(44, 42, 16.0);
    sibling.background = Color::BLUE;
    sibling.set_frame_identity(
        DisplayFrameId::new(44),
        DisplayFrameId::new(42),
        56.0,
        8.0,
        2,
        false,
        0.0,
        Color::BLACK,
        false,
        1.0,
    );
    assert!(render.compositor.child_frames.update_frame(sibling.clone()));
    let mut controls = crate::thread_comm::FrameOpacityState::default();
    controls.accept(42, [1.0; 2], 0.0);
    controls.accept(43, [1.0, 0.5], 0.0);
    controls.set_redirects(vec![(42, Some(43))]);
    controls.focus(42, true);
    render.apply_frame_opacity(&controls);
    seed_pending_layout(&mut render, &root, origin);
    let interaction = format!("{:?}", render.compositor.interaction);
    let baseline = format!(
        "{:?}",
        render.compositor.baseline.as_ref().unwrap().window_infos
    );
    let current_frame = render.compositor.current_frame.clone();
    let placement = render.compositor.child_frames.frames[&43]
        .frame
        .frame_placement;
    let target = target(&renderer);
    let view = target.create_view(&Default::default());
    let style = ChildFrameStyle {
        corner_radius: 0.0,
        shadow_enabled: false,
        shadow_layers: 0,
        shadow_offset: 0.0,
        shadow_opacity: 0.0,
    };
    let pressure = renderer
        .acquire_snapshot(SnapshotSize::new(512, 506).unwrap())
        .unwrap();
    let size = SnapshotSize::new(W, H).unwrap();
    renderer.set_frame_sample(FrameSample::new(
        origin.plus(Duration::from_millis(500)),
        Duration::ZERO,
    ));
    // The same accepted policy and payload survive loss and regain of effective
    // highlight. Neither a replacement scene nor releasing pressure may help.
    for focused in [true, false, true] {
        controls.focus(42, focused);
        render.apply_frame_opacity(&controls);
        assert_eq!(
            render.compositor.child_frames.frames[&43].applied_frame_alpha,
            if focused { 1.0 } else { 0.5 }
        );
        let before = pixels(&renderer, &target);
        let admission = super::super::composition_targets::prepare_child_targets(
            &mut renderer,
            &mut render,
            size,
        );
        assert_eq!(render.compositor.current_frame, current_frame);
        assert_eq!(format!("{:?}", render.compositor.interaction), interaction);
        assert_eq!(
            format!(
                "{:?}",
                render.compositor.baseline.as_ref().unwrap().window_infos
            ),
            baseline
        );
        assert!(
            render.compositor.layout.wants_frames(),
            "admission cannot settle undrawn pane motion"
        );
        assert_eq!(render.compositor.child_frames.frames[&43].frame, child);
        assert_eq!(render.compositor.child_frames.frames[&44].frame, sibling);
        assert_eq!(
            render.compositor.child_frames.frames[&43]
                .frame
                .frame_placement,
            placement
        );
        if !focused {
            assert!(matches!(
                admission,
                Err(super::super::surface::FrameRenderFailure::WindowNotReady)
            ));
            assert!(render.child_opacity_src.is_none() && render.child_resize_src.is_none());
            assert_eq!(pixels(&renderer, &target), before);
            continue;
        }
        let (picture, mixed) = admission
            .expect("currently opaque mixed-policy child needs no picture under held pressure");
        assert!(picture.is_none() && mixed.is_none());
        draw_root(&mut renderer, &mut render, &view, &root);
        render_frame_content_overlays(
            &mut renderer,
            (W, H),
            &mut render,
            &view,
            &root,
            false,
            None,
            &style,
            false,
        )
        .unwrap();
        let data = pixels(&renderer, &target);
        assert_eq!(px(&data, 48, 40), [0, 255, 0, 255]);
        assert_eq!(
            px(&data, 60, 40),
            [0, 0, 255, 255],
            "topmost sibling remains on top"
        );
        assert_eq!(
            px(&data, 2, 60),
            [0; 4],
            "outside children remains untouched"
        );
    }
    // Nil carries no fractional policy, yet must retain the actually applied .5.
    controls.focus(42, false);
    controls.accept(43, [-1.0; 2], 0.0);
    render.apply_frame_opacity(&controls);
    render
        .compositor
        .child_frames
        .frames
        .get_mut(&43)
        .unwrap()
        .frame
        .frame_alpha = [-1.0; 2];
    assert_eq!(
        render.compositor.child_frames.frames[&43].applied_frame_alpha,
        0.5
    );
    assert!(matches!(
        super::super::composition_targets::prepare_child_targets(&mut renderer, &mut render, size),
        Err(super::super::surface::FrameRenderFailure::WindowNotReady)
    ));
    // Current fractional background remains mandatory with whole-frame alpha1.
    controls.accept(43, [1.0; 2], 0.0);
    render.apply_frame_opacity(&controls);
    render
        .compositor
        .child_frames
        .frames
        .get_mut(&43)
        .unwrap()
        .frame
        .background_alpha = 0.5;
    assert_eq!(
        render.compositor.child_frames.frames[&43].applied_frame_alpha,
        1.0
    );
    assert!(matches!(
        super::super::composition_targets::prepare_child_targets(&mut renderer, &mut render, size),
        Err(super::super::surface::FrameRenderFailure::WindowNotReady)
    ));
    assert_eq!(format!("{:?}", render.compositor.interaction), interaction);
    drop(pressure);
    let (picture, mixed) =
        super::super::composition_targets::prepare_child_targets(&mut renderer, &mut render, size)
            .unwrap();
    assert!(picture.is_some() && mixed.is_none());
    render.child_opacity_src = picture;
    draw_root(&mut renderer, &mut render, &view, &root);
    render_frame_content_overlays(
        &mut renderer,
        (W, H),
        &mut render,
        &view,
        &root,
        false,
        None,
        &style,
        false,
    )
    .unwrap();
    let data = pixels(&renderer, &target);
    assert!((127..=129).contains(&px(&data, 48, 40)[3]));
    assert_eq!(px(&data, 25, 25), [255; 4], "foreground remains opaque");
    assert_eq!(px(&data, 60, 40), [0, 0, 255, 255]);
}

#[test]
fn expired_child_resize_recovers_under_unchanged_budget_pressure() {
    if !in_gpu_budget_child(
        "render_thread::render_pass::scene::tests::expired_child_resize_recovers_under_unchanged_budget_pressure",
    ) {
        return;
    }
    let mut renderer = WgpuRenderer::new(None, W, H).expect("GPU required for expiry regression");
    let origin = observe_platform_now();
    let mut render = GuiFrameRenderState::new(42, renderer.device(), 1.0, false, origin);
    let mut root = FrameGlyphBuffer::with_size(W as f32, H as f32);
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
    root.background_alpha = 0.0;
    render.set_current_frame(
        Some(root.clone()),
        None,
        Default::default(),
        Default::default(),
    );
    let old_frame = picture(43, 42, 64.0);
    let old = renderer
        .acquire_snapshot(SnapshotSize::new(64, 48).unwrap())
        .unwrap();
    let old_id = old.id();
    renderer.capture_child_frame_picture(
        &old,
        &old_frame,
        render.compositor.glyph_atlas.as_mut().unwrap(),
        0.0,
        false,
        0,
        0.0,
        0.0,
    );
    let mut new_frame = picture(43, 42, 80.0);
    new_frame.background = Color::BLUE;
    new_frame.frame_alpha = [1.0, 0.5];
    assert!(render.compositor.child_frames.update_frame(new_frame));
    let mut controls = crate::thread_comm::FrameOpacityState::default();
    controls.accept(42, [1.0; 2], 0.0);
    controls.accept(43, [1.0, 0.5], 0.0);
    controls.set_redirects(vec![(42, Some(43))]);
    controls.focus(42, true);
    render.apply_frame_opacity(&controls);
    assert_eq!(
        render.compositor.child_frames.frames[&43].applied_frame_alpha,
        1.0
    );
    render.compositor.child_frames.begin_resize_crossfade(
        43,
        old,
        64.0,
        48.0,
        linear_second(),
        origin,
    );
    // Seed a genuine nonidentity submitted projection and pending motion. Admission
    // may reclaim expired picture ownership, but must not publish undrawn geometry.
    seed_pending_layout(&mut render, &root, origin);
    let surface_point = neomacs_display_protocol::GeometryPoint::<
        neomacs_display_protocol::RootSurfaceSpace,
        LogicalPixels,
    >::from_px(40.0, 20.0)
    .unwrap();
    let mapped = render
        .compositor
        .interaction
        .as_ref()
        .unwrap()
        .map(surface_point)
        .unwrap();
    assert!(
        (mapped.x() - 40.0).abs() > 1.0,
        "guard must start with nonidentity submitted geometry"
    );
    render.compositor.pending.theme = Some(
        crate::render_thread::frame_compositor::continuity::theme::ThemeChange {
            bounds: neomacs_display_protocol::types::Rect::new(0.0, 0.0, W as f32, H as f32),
        },
    );
    let pending_theme = render.compositor.pending.theme;
    let current_frame = render.compositor.current_frame.clone();
    assert!(
        render
            .compositor
            .child_frames
            .begin_drift(43, 4.0, 4.0, linear_second(), origin)
    );
    render
        .compositor
        .child_frames
        .record_drift_drawn(43, 4.0, 4.0, 0.25);
    let drift = render.compositor.child_frames.frames[&43]
        .drift
        .as_ref()
        .unwrap()
        .last_drawn;
    let clip = render.compositor.child_frames.frames[&43].clip_in_root;
    let interaction = format!("{:?}", render.compositor.interaction);
    let baseline = format!(
        "{:?}",
        render.compositor.baseline.as_ref().unwrap().window_infos
    );
    let placement = render.compositor.child_frames.frames[&43]
        .frame
        .frame_placement;
    let target = target(&renderer);
    let view = target.create_view(&Default::default());
    draw_root(&mut renderer, &mut render, &view, &root);
    let before = pixels(&renderer, &target);
    let pressure = renderer
        .acquire_snapshot(SnapshotSize::new(512, 500).unwrap())
        .unwrap();
    let size = SnapshotSize::new(W, H).unwrap();
    for millis in [250, 750, 999] {
        renderer.set_frame_sample(FrameSample::new(
            origin.plus(Duration::from_millis(millis)),
            Duration::ZERO,
        ));
        assert!(matches!(
            super::super::composition_targets::prepare_child_targets(
                &mut renderer,
                &mut render,
                size
            ),
            Err(super::super::surface::FrameRenderFailure::WindowNotReady)
        ));
        assert!(
            render.compositor.child_frames.frames[&43]
                .crossfade
                .is_some()
        );
        assert!(render.child_opacity_src.is_none() && render.child_resize_src.is_none());
        assert_eq!(pixels(&renderer, &target), before);
        assert_eq!(render.compositor.current_frame, current_frame);
        assert_eq!(render.compositor.pending.theme, pending_theme);
        assert_eq!(
            render.compositor.child_frames.frames[&43]
                .drift
                .as_ref()
                .unwrap()
                .last_drawn,
            drift
        );
        assert_eq!(
            render.compositor.child_frames.frames[&43].clip_in_root,
            clip
        );
        assert_eq!(format!("{:?}", render.compositor.interaction), interaction);
    }
    let generation = render.compositor.current_scene_generation;
    let style = ChildFrameStyle {
        corner_radius: 0.0,
        shadow_enabled: false,
        shadow_layers: 0,
        shadow_offset: 0.0,
        shadow_opacity: 0.0,
    };
    for millis in [1000, 1250] {
        renderer.set_frame_sample(FrameSample::new(
            origin.plus(Duration::from_millis(millis)),
            Duration::ZERO,
        ));
        let (picture, mixed) = super::super::composition_targets::prepare_child_targets(
            &mut renderer,
            &mut render,
            size,
        )
        .expect("settled opaque child must resume without pressure release or replacement payload");
        assert!(picture.is_none() && mixed.is_none());
        assert!(
            render.compositor.child_frames.frames[&43]
                .crossfade
                .is_none()
        );
        assert_ne!(render.compositor.current_scene_generation, generation);
        assert_eq!(render.compositor.current_frame, current_frame);
        assert_eq!(render.compositor.pending.theme, pending_theme);
        assert_eq!(
            render.compositor.child_frames.frames[&43].clip_in_root,
            clip
        );
        if millis == 1000 {
            assert_eq!(
                render.compositor.child_frames.frames[&43]
                    .drift
                    .as_ref()
                    .unwrap()
                    .last_drawn,
                drift
            );
        }
        assert_eq!(format!("{:?}", render.compositor.interaction), interaction);
        assert_eq!(
            format!(
                "{:?}",
                render.compositor.baseline.as_ref().unwrap().window_infos
            ),
            baseline
        );
        assert!(
            render.compositor.layout.wants_frames(),
            "preflight must not settle unpublished pane motion"
        );
        assert_eq!(
            render.compositor.child_frames.frames[&43]
                .frame
                .frame_placement,
            placement
        );
        draw_root(&mut renderer, &mut render, &view, &root);
        render_frame_content_overlays(
            &mut renderer,
            (W, H),
            &mut render,
            &view,
            &root,
            false,
            None,
            &style,
            false,
        )
        .unwrap();
        let data = pixels(&renderer, &target);
        assert_eq!(px(&data, 48, 40), [0, 0, 255, 255]);
        assert_eq!(
            px(&data, 80, 40),
            [0, 0, 255, 255],
            "settled new-only extent"
        );
        assert_eq!(px(&data, 2, 2), [0; 4]);
    }
    let released = renderer
        .acquire_snapshot(SnapshotSize::new(64, 48).unwrap())
        .unwrap();
    assert_eq!(
        released.id(),
        old_id,
        "old lease was returned to the production pool"
    );
    drop(released);
    let mut controls = crate::thread_comm::FrameOpacityState::default();
    controls.accept(43, [0.5; 2], 0.0);
    render.apply_frame_opacity(&controls);
    let before = pixels(&renderer, &target);
    assert!(matches!(
        super::super::composition_targets::prepare_child_targets(&mut renderer, &mut render, size),
        Err(super::super::surface::FrameRenderFailure::WindowNotReady)
    ));
    assert_eq!(pixels(&renderer, &target), before);
    assert_eq!(format!("{:?}", render.compositor.interaction), interaction);
    drop(pressure);
    let (picture, mixed) =
        super::super::composition_targets::prepare_child_targets(&mut renderer, &mut render, size)
            .unwrap();
    assert!(picture.is_some() && mixed.is_none());
    render.child_opacity_src = picture;
    draw_root(&mut renderer, &mut render, &view, &root);
    render_frame_content_overlays(
        &mut renderer,
        (W, H),
        &mut render,
        &view,
        &root,
        false,
        None,
        &style,
        false,
    )
    .unwrap();
    assert!((127..=129).contains(&px(&pixels(&renderer, &target), 48, 40)[3]));
}

use super::*;

#[test]
fn production_split_delete_placements_keep_background_opacity() {
    use crate::render_thread::frame_compositor::continuity::pane_layout::PixelGrid;
    use neomacs_display_protocol::types::Rect;
    use neomacs_renderer_wgpu::PaneSource;
    let mut renderer =
        WgpuRenderer::new(None, W, H).expect("GPU required for runtime pane regression");
    let size = SnapshotSize::new(W, H).unwrap();
    let old = renderer.acquire_snapshot(size).unwrap();
    let new = renderer.acquire_snapshot(size).unwrap();
    for splitting in [true, false] {
        let origin = observe_platform_now();
        let mut render = GuiFrameRenderState::new(42, renderer.device(), 1.0, false, origin);
        render.compositor.pane_motion =
            crate::render_thread::render_quality::WindowAnimationSpecs {
                resize: linear_second(),
                movement: linear_second(),
                open: linear_second(),
                close: linear_second(),
            };
        let single = vec![window(1, Rect::new(0.0, 0.0, W as f32, 48.0))];
        let split = vec![
            window(1, Rect::new(0.0, 0.0, 48.0, 48.0)),
            window(2, Rect::new(48.0, 0.0, 48.0, 48.0)),
        ];
        let mut before = picture(42, 0, W as f32);
        before.height = H as f32;
        before.background_alpha = 0.5;
        before.window_infos = if splitting {
            single.clone()
        } else {
            split.clone()
        };
        let mut after = before.clone();
        after.background = Color::BLUE;
        after.window_infos = if splitting { split } else { single };
        draw_root(&mut renderer, &mut render, old.view(), &before);
        draw_root(&mut renderer, &mut render, new.view(), &after);
        install_layout(&mut render, &before, origin);
        install_layout(&mut render, &after, origin);
        let target = target(&renderer);
        let view = target.create_view(&Default::default());
        for millis in [0, 500, 1000] {
            let sample =
                FrameSample::new(origin.plus(Duration::from_millis(millis)), Duration::ZERO);
            let composition = render.sample_pane_layout(
                &super::super::surface::SurfaceAcquired::for_test(),
                sample,
                PixelGrid::new(1.0),
            );
            assert!(!composition.blits.is_empty());
            if millis < 1000 {
                assert!(
                    composition
                        .blits
                        .iter()
                        .any(|p| p.source == PaneSource::Previous)
                );
            }
            eprintln!(
                "runtime pane split={splitting} {millis}: {:?}",
                composition.blits
            );
            renderer.render_pane_layout(
                new.bind_group(),
                Some(old.bind_group()),
                &view,
                (W as f32, H as f32),
                &composition.blits,
            );
            let data = pixels(&renderer, &target);
            for x in 2..W - 2 {
                let bg = px(&data, x, 40);
                assert!(
                    (127..=129).contains(&bg[3]),
                    "runtime pane split={splitting} {millis} x{x}: {bg:?}"
                );
            }
            let outside = px(&data, 80, 60);
            assert_eq!(outside, [0, 0, 188, 128], "outside actual pane placements");
            assert_eq!(
                px(&data, 20, 20),
                [255; 4],
                "opaque foreground inside surviving pane"
            );
        }
    }
}

#[test]
fn expired_child_lifecycle_needs_no_scratch_under_unchanged_pressure() {
    if !in_gpu_budget_child(
        "render_thread::render_pass::scene::tests::expired_child_lifecycle_needs_no_scratch_under_unchanged_pressure",
    ) {
        return;
    }
    for closing in [false, true] {
        let mut renderer =
            WgpuRenderer::new(None, W, H).expect("GPU required for lifecycle expiry regression");
        let origin = observe_platform_now();
        let mut render = GuiFrameRenderState::new(42, renderer.device(), 1.0, false, origin);
        let mut root = FrameGlyphBuffer::with_size(W as f32, H as f32);
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
        render.set_current_frame(Some(root), None, Default::default(), Default::default());
        assert!(
            render
                .compositor
                .child_frames
                .update_frame(picture(43, 42, 64.0))
        );
        if closing {
            assert!(render.compositor.child_frames.retire_frame(
                43,
                linear_second(),
                origin,
                0.0,
                1.0
            ));
        } else {
            render.compositor.child_frames.begin_open_animation(
                43,
                linear_second(),
                origin,
                0.0,
                1.0,
            );
        }
        let pressure = renderer
            .acquire_snapshot(SnapshotSize::new(512, 506).unwrap())
            .unwrap();
        let size = SnapshotSize::new(W, H).unwrap();
        renderer.set_frame_sample(FrameSample::new(
            origin.plus(Duration::from_millis(999)),
            Duration::ZERO,
        ));
        assert!(matches!(
            super::super::composition_targets::prepare_child_targets(
                &mut renderer,
                &mut render,
                size
            ),
            Err(super::super::surface::FrameRenderFailure::WindowNotReady)
        ));
        renderer.set_frame_sample(FrameSample::new(
            origin.plus(Duration::from_millis(1000)),
            Duration::ZERO,
        ));
        let (picture, mixed) = super::super::composition_targets::prepare_child_targets(
            &mut renderer,
            &mut render,
            size,
        )
        .unwrap();
        assert!(picture.is_none() && mixed.is_none());
        assert!(!render.compositor.child_frames.has_animation_activity());
        assert_eq!(
            render.compositor.child_frames.frames.contains_key(&43),
            !closing
        );
        drop(pressure);
    }
}

use super::super::tests::in_gpu_budget_child;
use neomacs_display_protocol::TransitionEasing;
use neomacs_display_protocol::frame_time::{FrameSample, observe_platform_now};
use neomacs_display_protocol::motion_spec::{MotionDuration, MotionSpec, TweenSpec};
use neomacs_display_protocol::{
    Color, DeviceScale, DisplayWindowId, FaceId, FrameGlyphBuffer, GeometrySize, GlyphRowRole,
    LogicalPixels, PresentMapping, PresentationExtent, StipplePattern, SurfaceState,
};
use neomacs_renderer_wgpu::SnapshotSize;
use std::time::Duration;
const W: u32 = 96;
const H: u32 = 64;
fn mapping(frame: &FrameGlyphBuffer, w: u32, h: u32) -> PresentMapping {
    let SurfaceState::Drawable(surface) =
        SurfaceState::from_device_size(w, h, DeviceScale::new(1.0).unwrap()).unwrap()
    else {
        unreachable!()
    };
    PresentMapping::top_left_clip(
        surface,
        PresentationExtent::new(
            frame.presentation_id,
            GeometrySize::<LogicalPixels>::from_px(frame.width, frame.height).unwrap(),
        ),
    )
}
fn picture(id: u64, parent: u64, width: f32) -> FrameGlyphBuffer {
    let mut frame = FrameGlyphBuffer::with_size(width, 48.0);
    frame.set_frame_identity(
        DisplayFrameId::new(id),
        DisplayFrameId::new(parent),
        8.0,
        8.0,
        1,
        false,
        0.0,
        Color::BLACK,
        false,
        1.0,
    );
    frame.background = Color::GREEN;
    frame.set_face(
        FaceId::new(1),
        Color::WHITE,
        Some(Color::GREEN),
        400,
        false,
        0,
        None,
        0,
        None,
        0,
        None,
    );
    frame.faces.get_mut(&FaceId::new(1)).unwrap().stipple = Some(Box::new(StipplePattern {
        width: 1,
        height: 1,
        bits: vec![1],
    }));
    frame.set_draw_context(DisplayWindowId::new(1), GlyphRowRole::Text, None);
    frame.add_stretch(12.0, 12.0, 16.0, 16.0, Color::GREEN, FaceId::new(1), false);
    frame
}
fn pixels(renderer: &WgpuRenderer, target: &wgpu::Texture) -> Vec<u8> {
    let padded = (W * 4).div_ceil(256) * 256;
    let buffer = renderer.device().create_buffer(&wgpu::BufferDescriptor {
        label: Some("runtime resize readback"),
        size: (padded * H) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = renderer
        .device()
        .create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: target,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded),
                rows_per_image: Some(H),
            },
        },
        wgpu::Extent3d {
            width: W,
            height: H,
            depth_or_array_layers: 1,
        },
    );
    renderer.queue().submit([encoder.finish()]);
    let slice = buffer.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    renderer
        .device()
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(Duration::from_secs(3)),
        })
        .unwrap();
    let data = slice.get_mapped_range().unwrap();
    let mut result = Vec::new();
    for row in 0..H {
        result.extend_from_slice(&data[(row * padded) as usize..(row * padded + W * 4) as usize]);
    }
    result
}
fn px(data: &[u8], x: u32, y: u32) -> [u8; 4] {
    let i = ((y * W + x) * 4) as usize;
    [data[i + 2], data[i + 1], data[i], data[i + 3]]
}
#[test]
fn production_child_resize_applies_complete_picture_opacity_once() {
    let mut renderer =
        WgpuRenderer::new(None, W, H).expect("GPU required for production resize regression");
    for (background_alpha, radius, lifecycle) in
        [(1.0, 0.0, false), (0.5, 8.0, false), (0.5, 8.0, true)]
    {
        let origin = observe_platform_now();
        let mut render = GuiFrameRenderState::new(42, renderer.device(), 1.0, false, origin);
        let mut root = FrameGlyphBuffer::with_size(W as f32, H as f32);
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
        root.background_alpha = 0.0;
        render.set_current_frame(
            Some(root.clone()),
            None,
            Default::default(),
            Default::default(),
        );
        let mut old_frame = picture(43, 42, 64.0);
        old_frame.background_alpha = background_alpha;
        let old = renderer
            .acquire_snapshot(SnapshotSize::new(64, 48).unwrap())
            .unwrap();
        // Poison the reusable capture lease with opaque pixels first. The same
        // complete-picture capture used by frame_ingest must clear uncovered corners.
        let mut poison = old_frame.clone();
        poison.background_alpha = 1.0;
        renderer.render_frame_glyphs(
            old.view(),
            &poison,
            render.compositor.glyph_atlas.as_mut().unwrap(),
            mapping(&poison, 64, 48),
            false,
            None,
            None,
            None,
            None,
            None,
        );
        renderer.capture_child_frame_picture(
            &old,
            &old_frame,
            render.compositor.glyph_atlas.as_mut().unwrap(),
            radius,
            false,
            0,
            0.0,
            0.0,
        );
        let mut new_frame = picture(43, 42, 80.0);
        new_frame.background = Color::BLUE;
        new_frame.background_alpha = background_alpha;
        new_frame.frame_alpha = [0.5, 0.25];
        assert!(render.compositor.child_frames.update_frame(new_frame));
        let spec = MotionSpec::Tween(TweenSpec {
            duration: MotionDuration::new(Duration::from_secs(1)).unwrap(),
            easing: TransitionEasing::Linear,
            bezier: None,
        });
        render
            .compositor
            .child_frames
            .begin_resize_crossfade(43, old, 64.0, 48.0, spec, origin);
        if lifecycle {
            render
                .compositor
                .child_frames
                .begin_open_animation(43, spec, origin, 0.0, 1.0);
        }
        (render.child_opacity_src, render.child_resize_src) =
            super::super::composition_targets::mandatory_child_targets(
                &mut renderer,
                true,
                true,
                SnapshotSize::new(W, H).unwrap(),
            )
            .unwrap();
        let target = renderer.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("runtime child resize target"),
            size: wgpu::Extent3d {
                width: W,
                height: H,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Bgra8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = target.create_view(&Default::default());
        let style = ChildFrameStyle {
            corner_radius: radius,
            shadow_enabled: false,
            shadow_layers: 0,
            shadow_offset: 0.0,
            shadow_opacity: 0.0,
        };
        let mut controls = crate::thread_comm::FrameOpacityState::default();
        controls.accept(42, [-1.0; 2], 0.0);
        controls.accept(43, [0.5, 0.25], 0.0);
        controls.set_redirects(vec![(42, Some(43))]);
        // Focus changes at the identical intermediate sample must update both pictures,
        // not only the freshly ingested foreground. No sleeps or display/input access.
        for millis in [0, 250, 500, 750, 1000] {
            renderer.set_frame_sample(FrameSample::new(
                origin.plus(Duration::from_millis(millis)),
                Duration::ZERO,
            ));
            for accepted in [[0.5, 0.25], [0.75, 0.375]] {
                controls.accept(43, accepted, 0.0);
                for focused in [true, false] {
                    controls.focus(42, focused);
                    render.apply_frame_opacity(&controls);
                    renderer.render_frame_glyphs(
                        &view,
                        &root,
                        render.compositor.glyph_atlas.as_mut().unwrap(),
                        mapping(&root, W, H),
                        false,
                        None,
                        None,
                        None,
                        None,
                        None,
                    );
                    render_frame_content_overlays(
                        &mut renderer,
                        (W, H),
                        &mut render,
                        &view,
                        &root,
                        false,
                        None,
                        &style,
                        false,
                    )
                    .unwrap();
                    let data = pixels(&renderer, &target);
                    let bg = px(&data, 48, 40);
                    let fg = px(&data, 25, 25);
                    let mix = millis as f32 / 1000.0;
                    let alpha =
                        accepted[if focused { 0 } else { 1 }] * if lifecycle { mix } else { 1.0 };
                    let expected = (255.0 * alpha).round() as u8;
                    eprintln!(
                        "runtime resize {millis}ms focus {focused}: background {bg:?}, foreground {fg:?}"
                    );
                    assert!(
                        bg[3].abs_diff((255.0 * alpha * background_alpha).round() as u8) <= 2,
                        "complete child background alpha: {millis}/{focused}: {bg:?}"
                    );
                    assert!(
                        fg[3].abs_diff(expected) <= 1,
                        "complete child foreground alpha: {millis}/{focused}: {fg:?}"
                    );
                    let weighted = Color::new(
                        0.0,
                        alpha * background_alpha * (1.0 - mix),
                        alpha * background_alpha * mix,
                        alpha * background_alpha,
                    )
                    .linear_to_srgb();
                    assert!(
                        bg[0] == 0
                            && bg[1].abs_diff((255.0 * weighted.g).round() as u8) <= 2
                            && bg[2].abs_diff((255.0 * weighted.b).round() as u8) <= 2,
                        "weighted pictures: {bg:?}"
                    );
                    assert!(
                        fg[0] == fg[1] && fg[1] == fg[2] && (alpha == 0.0 || fg[0] > 0),
                        "foreground preserved: {fg:?}"
                    );
                    let new_only = px(&data, 80, 40);
                    assert!(
                        new_only[3]
                            .abs_diff((255.0 * alpha * background_alpha * mix).round() as u8)
                            <= 2,
                        "new-only resize extent: {new_only:?}"
                    );
                    assert_eq!(px(&data, 2, 2), [0; 4], "parent outside child is untouched");
                    if radius > 0.0 {
                        assert_eq!(px(&data, 8, 8), [0; 4], "capture clears rounded corner");
                    }
                }
            }
        }
    }
}

#[test]
fn missing_child_scratch_returns_retry_without_drawing_and_recovers() {
    let mut renderer =
        WgpuRenderer::new(None, W, H).expect("GPU required for preparation regression");
    let origin = observe_platform_now();
    let mut render = GuiFrameRenderState::new(42, renderer.device(), 1.0, false, origin);
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
    root.background = Color::BLUE;
    render.set_current_frame(
        Some(root.clone()),
        None,
        Default::default(),
        Default::default(),
    );
    let mut child = picture(43, 42, 40.0);
    child.background_alpha = 0.5;
    assert!(render.compositor.child_frames.update_frame(child));
    let texture = target(&renderer);
    let view = texture.create_view(&Default::default());
    draw_root(&mut renderer, &mut render, &view, &root);
    let before = pixels(&renderer, &texture);
    let interaction = format!("{:?}", render.compositor.interaction);
    let style = ChildFrameStyle::default();
    assert!(matches!(
        render_frame_content_overlays(
            &mut renderer,
            (W, H),
            &mut render,
            &view,
            &root,
            false,
            None,
            &style,
            false
        ),
        Err(super::super::surface::FrameRenderFailure::WindowNotReady)
    ));
    assert_eq!(pixels(&renderer, &texture), before);
    assert_eq!(format!("{:?}", render.compositor.interaction), interaction);
    (render.child_opacity_src, render.child_resize_src) =
        super::super::composition_targets::prepare_child_targets(
            &mut renderer,
            &mut render,
            SnapshotSize::new(W, H).unwrap(),
        )
        .unwrap();
    render_frame_content_overlays(
        &mut renderer,
        (W, H),
        &mut render,
        &view,
        &root,
        false,
        None,
        &style,
        false,
    )
    .unwrap();
    assert_ne!(pixels(&renderer, &texture), before);
}
