//! Actual production admission, transition dispatch and final native storage.
use super::super::super::composition_targets::{native_content_target, prepare_frame_targets};
use super::*;
use crate::render_thread::transitions::{detect_frame_transitions, render_frame_transitions};
use neomacs_display_protocol::frame_glyphs::{
    BufferTransitionTarget, ContentTransitionHint, PresentedWindowRegions,
};
use neomacs_display_protocol::{ContentTransitionIntent, TransitionEffect};
use neomacs_renderer_wgpu::renderer::{NativeContentPlacement, RenderTarget};
pub(super) fn full_plan() -> crate::render_thread::render_quality::RenderFeaturePlan {
    crate::render_thread::render_quality::RenderFeaturePlan {
        compose_offscreen: true,
        accept_transition_hints: true,
        accept_derived_effects: true,
        accept_cursor_effects: true,
        apply_frame_post: false,
    }
}

#[test]
fn obsolete_native_owner_retires_before_required_child_under_same_pressure() {
    if !in_gpu_budget_child(
        "render_thread::render_pass::scene::tests::native_boundary::obsolete_native_owner_retires_before_required_child_under_same_pressure",
    ) {
        return;
    }
    let mut renderer = WgpuRenderer::new(None, W, H).expect("GPU required");
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
    root.background = Color::BLACK;
    root.background_alpha = 1.0;
    render.set_current_frame(
        Some(root.clone()),
        None,
        Default::default(),
        Default::default(),
    );
    let child = picture(43, 42, 64.0);
    assert!(render.compositor.child_frames.update_frame(child.clone()));
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
    seed_pending_layout(&mut render, &root, origin);
    let surface = mapping(&root, W, H).surface();
    let mut controls = crate::thread_comm::FrameOpacityState::default();
    controls.accept(42, [0.5; 2], 0.0);
    controls.accept(43, [1.0; 2], 0.0);
    controls.focus(42, true);
    render.apply_frame_opacity(&controls);
    let out = target(&renderer);
    let view = out.create_view(&Default::default());
    let style = ChildFrameStyle {
        corner_radius: 0.0,
        shadow_enabled: false,
        shadow_layers: 0,
        shadow_offset: 0.0,
        shadow_opacity: 0.0,
    };
    let retained = native_content_target(&mut renderer, &mut render, surface, &full_plan())
        .unwrap()
        .unwrap();
    draw_root(&mut renderer, &mut render, retained.view(), &root);
    render_frame_content_overlays(
        &mut renderer,
        (W, H),
        &mut render,
        retained.view(),
        &root,
        false,
        None,
        &style,
        false,
    );
    renderer.place_native_content_with_opacity(
        NativeContentPlacement::new(RenderTarget::new(&view, surface), &retained).unwrap(),
        root.background,
        render.applied_frame_alpha,
    );
    let seeded = px(&pixels(&renderer, &out), 48, 40);
    assert!(
        seeded
            .iter()
            .zip([0_u8, 128, 0, 128])
            .all(|(a, b)| a.abs_diff(b) <= 1),
        "real fractional-root presentation seeds the retained native owner: {seeded:?}"
    );
    let old_id = retained.id();
    drop(retained); // Only the real frame owner remains, not an external holder.
    let pressure = renderer
        .acquire_snapshot(SnapshotSize::new(512, 496).unwrap())
        .unwrap();
    let interaction = format!("{:?}", render.compositor.interaction);
    let current = render.compositor.current_frame.clone();
    let generation = render.compositor.current_scene_generation;
    // Both native root and child really need the one available slot: typed retry.
    controls.accept(43, [0.5; 2], 0.0);
    render.apply_frame_opacity(&controls);
    eprintln!(
        "native owner control root={} child={} budget={:?} held={:?} native={:?}",
        render.applied_frame_alpha,
        render.compositor.child_frames.frames[&43].applied_frame_alpha,
        renderer.gpu_budget().pooled_bytes(),
        pressure.size(),
        render.native_content_src.as_ref().map(|p| p.id())
    );
    assert!(matches!(
        prepare_frame_targets(&mut renderer, &mut render, surface, &full_plan()),
        Err(super::super::super::surface::FrameRenderFailure::WindowNotReady)
    ));
    assert_eq!(render.native_content_src.as_ref().unwrap().id(), old_id);
    controls.accept(42, [1.0; 2], 0.0);
    render.apply_frame_opacity(&controls);
    for _ in 0..2 {
        let before = pixels(&renderer, &out);
        let super::super::super::composition_targets::FrameTargets {
            picture,
            resize,
            native,
        } = prepare_frame_targets(&mut renderer, &mut render, surface, &full_plan())
            .expect("obsolete native owner must retire BEFORE mandatory child reservation");
        assert!(native.is_none() && render.native_content_src.is_none() && resize.is_none());
        let picture = picture.expect("fractional child must never be omitted");
        assert_eq!(
            picture.id(),
            old_id,
            "reuse the returned owner slot under unchanged pressure"
        );
        assert_eq!(render.compositor.current_frame, current);
        assert_eq!(format!("{:?}", render.compositor.interaction), interaction);
        assert!(
            render.compositor.layout.wants_frames(),
            "preflight cannot settle undrawn motion"
        );
        assert_eq!(render.compositor.child_frames.frames[&43].frame, child);
        assert_eq!(render.compositor.child_frames.frames[&44].frame, sibling);
        assert_eq!(pixels(&renderer, &out), before, "preflight cannot draw");
        render.child_opacity_src = Some(picture);
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
        );
        let data = pixels(&renderer, &out);
        assert_eq!(px(&data, 60, 40), [0, 0, 255, 255], "sibling order");
        assert!(
            (186..=189).contains(&px(&data, 48, 40)[1]),
            "fractional required child actually drawn"
        );
        assert_eq!(px(&data, 2, 60), [0, 0, 0, 255]);
        render.child_opacity_src = None;
    }
    assert!(render.compositor.current_scene_generation >= generation);
    drop(pressure);
}

#[test]
fn dispatched_scale_zoom_strips_use_final_native_conversion_with_opaque_controls() {
    let mut renderer = WgpuRenderer::new(None, W, H).expect("real GPU required");
    let size = SnapshotSize::new(W, H).unwrap();
    for (background_alpha, whole_alpha) in [(1.0, 1.0), (0.5, 0.5)] {
        let origin = observe_platform_now();
        let mut render = GuiFrameRenderState::new(42, renderer.device(), 1.0, false, origin);
        let mut old_frame = FrameGlyphBuffer::with_size(W as f32, H as f32);
        old_frame.background = Color::GREEN;
        old_frame.background_alpha = background_alpha;
        let mut new_frame = old_frame.clone();
        new_frame.background = Color::BLUE;
        let surface = mapping(&new_frame, W, H).surface();
        render.set_current_frame(
            Some(new_frame.clone()),
            None,
            Default::default(),
            Default::default(),
        );
        render.applied_frame_alpha = whole_alpha;
        render.compositor.transitions.policy.buffer.enabled = true;
        render.compositor.transitions.policy.buffer.duration = Duration::from_secs(1);
        render.compositor.transitions.policy.buffer.easing = TransitionEasing::Linear;
        render.compositor.transitions.policy.buffer.effect = TransitionEffect::ScaleZoom;
        let old = render
            .compositor
            .transitions
            .advance_compositions(&mut renderer, size)
            .unwrap();
        draw_root(&mut renderer, &mut render, old.view(), &old_frame);
        drop(old);
        if background_alpha == 1.0 {
            assert!(
                native_content_target(&mut renderer, &mut render, surface, &full_plan())
                    .unwrap()
                    .is_none(),
                "enabled transition policy and an idle ring must not force a native lease"
            );
        }
        let region = PresentedWindowRegions {
            text_body: neomacs_display_protocol::Rect::new(8.0, 8.0, 80.0, 48.0),
            ..Default::default()
        }
        .buffer_viewport()
        .unwrap();
        new_frame
            .transition_hints
            .push(ContentTransitionHint::BufferReplaced {
                target: BufferTransitionTarget::Frame {
                    regions: vec![region],
                },
                intent: ContentTransitionIntent::Replace,
            });
        // This pending hint is the real path that begins a transition AFTER native
        // preflight. Admission must neither drain it nor ignore its future pixels.
        render.compositor.current_frame = Some(new_frame.clone());
        let native = prepare_frame_targets(&mut renderer, &mut render, surface, &full_plan())
            .unwrap()
            .native;
        assert_eq!(
            render
                .compositor
                .current_frame
                .as_ref()
                .unwrap()
                .transition_hints,
            new_frame.transition_hints
        );
        let out = target(&renderer);
        let view = out.create_view(&Default::default());
        let destination = native.as_ref().map_or(&view, |p| p.view());
        let new = render
            .compositor
            .transitions
            .advance_compositions(&mut renderer, size)
            .unwrap();
        draw_root(&mut renderer, &mut render, new.view(), &new_frame);
        renderer.set_frame_sample(FrameSample::new(origin, Duration::ZERO));
        detect_frame_transitions(
            &mut renderer,
            &mut render.compositor.transitions,
            &Default::default(),
            &mut new_frame,
            Default::default(),
            &mut render.compositor.dirty,
        );
        assert!(render.compositor.transitions.has_active());
        render.compositor.current_frame = Some(new_frame.clone());
        let retained = prepare_frame_targets(&mut renderer, &mut render, surface, &full_plan())
            .unwrap()
            .native;
        assert_eq!(
            retained.as_ref().map(|p| p.id()),
            native.as_ref().map(|p| p.id()),
            "active transition still requires conversion after hints drain"
        );
        drop(retained);
        for millis in [250, 750] {
            renderer.set_frame_sample(FrameSample::new(
                origin.plus(Duration::from_millis(millis)),
                Duration::ZERO,
            ));
            renderer
                .begin_draw(RenderTarget::new(destination, surface))
                .blit_retained(new.bind_group());
            render_frame_transitions(
                &mut renderer,
                &mut render.compositor.transitions,
                destination,
                W,
                H,
                &new_frame,
            );
            if let Some(native) = &native {
                let placement =
                    NativeContentPlacement::new(RenderTarget::new(&view, surface), native).unwrap();
                renderer.place_native_content_with_opacity(
                    placement,
                    new_frame.background,
                    whole_alpha,
                );
            }
            let data = pixels(&renderer, &out);
            let strip = px(&data, 9, 24);
            let overlap = px(&data, 48, 32);
            let outside = px(&data, 2, 2);
            eprintln!(
                "native ScaleZoom {background_alpha}/{whole_alpha} {millis}: strip {strip:?} overlap {overlap:?} outside {outside:?} conversion={}",
                native.is_some()
            );
            // Missing geometric weight is the current (blue) background,
            // not a transparent strip or a third layer under both snapshots.
            let a = background_alpha * whole_alpha;
            let expected_alpha = (255.0_f32 * a).round() as u8;
            assert!(strip[3].abs_diff(expected_alpha) <= 2);
            let strip_color = if millis == 250 {
                Color::new(0.0, 0.75, 0.25, 1.0).linear_to_srgb()
            } else {
                Color::BLUE
            };
            assert!(strip[1].abs_diff((255.0 * a * strip_color.g).round() as u8) <= 2);
            assert!(strip[2].abs_diff((255.0 * a * strip_color.b).round() as u8) <= 2);
            assert!(strip[..3].iter().all(|c| *c <= strip[3].saturating_add(1)));
            let t = millis as f32 / 1000.0;
            let straight = Color::new(0.0, 1.0 - t, t, 1.0).linear_to_srgb();
            let a = background_alpha * whole_alpha;
            assert!(overlap[3].abs_diff((255.0 * a).round() as u8) <= 1);
            assert!(overlap[1].abs_diff((255.0 * a * straight.g).round() as u8) <= 2);
            assert!(overlap[2].abs_diff((255.0 * a * straight.b).round() as u8) <= 2);
            assert!(
                outside[2].abs_diff((255.0 * a).round() as u8) <= 1
                    && outside[3].abs_diff((255.0 * a).round() as u8) <= 1
            );
        }
    }
}
