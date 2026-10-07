//! Pixel regressions for GNU background-only and completed-picture opacity.

#[test]
fn pane_previous_patches_interpolate_complete_colored_pictures() {
    use neomacs_renderer_wgpu::{PaneBlit, PaneSource};
    let mut h = try_harness().expect("GPU required for pane opacity regression");
    let mut old_frame = picture();
    old_frame.background = Color::GREEN;
    let mut new_frame = old_frame.clone();
    new_frame.background = Color::BLUE;
    let size = SnapshotSize::new(W, H).unwrap();
    let old = h.renderer.acquire_snapshot(size).unwrap();
    let new = h.renderer.acquire_snapshot(size).unwrap();
    render(&mut h, &old_frame, old.view());
    render(&mut h, &new_frame, new.view());
    let mut outside = None;
    for weight in [0.0, 0.5, 1.0] {
        h.renderer.render_pane_layout(
            new.bind_group(),
            Some(old.bind_group()),
            &h.view,
            (W as f32, H as f32),
            &[PaneBlit {
                bounds: neomacs_display_protocol::types::Rect::new(8.0, 8.0, 80.0, 48.0),
                content_origin: (8.0, 8.0),
                source: PaneSource::Previous,
                opacity: weight,
            }],
        );
        let pixels = read_back(&h);
        let bg = px(&pixels, 70, 45);
        eprintln!("pane Previous {weight}: {bg:?}");
        assert!((127..=129).contains(&bg[3]), "pane weight {weight}: {bg:?}");
        let color = Color::new(0.0, 0.5 * weight, 0.5 * (1.0 - weight), 0.5).linear_to_srgb();
        assert!(bg[1].abs_diff((color.g * 255.0).round() as u8) <= 2);
        assert!(bg[2].abs_diff((color.b * 255.0).round() as u8) <= 2);
        assert!(
            opaque_text(&pixels) > 0,
            "pane foreground remains opaque at {weight}"
        );
        let sample = px(&pixels, 2, 2);
        assert_eq!(
            sample,
            *outside.get_or_insert(sample),
            "outside patch unchanged"
        );
    }
}

#[test]
fn dispatched_scale_zoom_preserves_overlap_opacity_and_outside_pixels() {
    use neomacs_display_protocol::{
        DirectionlessTransitionEffect, ResolvedTransitionEffect, TransitionEasing,
    };
    let mut h = try_harness().expect("GPU required for ScaleZoom regression");
    let size = SnapshotSize::new(W, H).unwrap();
    let old = h.renderer.acquire_snapshot(size).unwrap();
    let new = h.renderer.acquire_snapshot(size).unwrap();
    let bounds = neomacs_display_protocol::types::Rect::new(8.0, 8.0, 80.0, 48.0);
    for alpha in [1.0, 0.5] {
        let mut frame = picture();
        frame.background = Color::GREEN;
        frame.background_alpha = alpha;
        // A broad foreground stipple interior remains fully covered after the
        // production scale resampling, unlike a one-pixel glyph stem.
        frame.faces.get_mut(&FaceId::new(0)).unwrap().stipple =
            Some(Box::new(neomacs_display_protocol::StipplePattern {
                width: 1,
                height: 1,
                bits: vec![1],
            }));
        frame.add_stretch(20.0, 16.0, 16.0, 24.0, Color::BLACK, FaceId::new(0), false);
        render(&mut h, &frame, old.view());
        render(&mut h, &frame, new.view());
        for progress in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let view = h.view.clone();
            render(&mut h, &frame, &view);
            let before = read_back(&h);
            h.renderer.render_transition_effect(
                &h.view,
                old.bind_group(),
                new.bind_group(),
                progress,
                0.0,
                &bounds,
                ResolvedTransitionEffect::Directionless(DirectionlessTransitionEffect::ScaleZoom),
                TransitionEasing::Linear,
                W,
                H,
                frame.background,
                frame.background_alpha,
            );
            let pixels = read_back(&h);
            let bg = px(&pixels, 70, 45);
            eprintln!("ScaleZoom {alpha}/{progress}: {bg:?}");
            assert!(
                bg[3].abs_diff((255.0_f32 * alpha).round() as u8) <= 1,
                "ScaleZoom overlap background {alpha}/{progress}: {bg:?}"
            );
            for x in [8, 9] {
                assert!(
                    px(&pixels, x, 32)[3].abs_diff((255.0_f32 * alpha).round() as u8) <= 1,
                    "ScaleZoom geometric complement {alpha}/{progress}: {:?}",
                    px(&pixels, x, 32)
                );
            }
            assert!(
                opaque_text(&pixels) > 0,
                "ScaleZoom opaque foreground {alpha}/{progress}"
            );
            assert_eq!(px(&pixels, 2, 2), px(&before, 2, 2));
            assert_eq!(px(&pixels, 94, 62), px(&before, 94, 62));
        }
    }
}

use super::*;

#[test]
fn dispatched_parallax_adds_overlapping_faded_pictures() {
    use neomacs_display_protocol::{
        AxisMotionTransitionEffect, ResolvedTransitionEffect, TransitionAxis, TransitionDirection,
        TransitionEasing,
    };
    let mut h = try_harness().expect("GPU required for shared weighted transition regression");
    let size = SnapshotSize::new(W, H).unwrap();
    let old = h.renderer.acquire_snapshot(size).unwrap();
    let new = h.renderer.acquire_snapshot(size).unwrap();
    for alpha in [1.0, 0.5] {
        let mut frame = picture();
        frame.background = Color::GREEN;
        frame.background_alpha = alpha;
        render(&mut h, &frame, old.view());
        render(&mut h, &frame, new.view());
        let view = h.view.clone();
        render(&mut h, &frame, &view);
        h.renderer.render_transition_effect(
            &h.view,
            old.bind_group(),
            new.bind_group(),
            0.5,
            0.0,
            &neomacs_display_protocol::types::Rect::new(8.0, 8.0, 80.0, 48.0),
            ResolvedTransitionEffect::AxisMotion {
                effect: AxisMotionTransitionEffect::Parallax,
                axis: TransitionAxis::Horizontal,
                direction: TransitionDirection::Forward,
                distance: 32.0,
            },
            TransitionEasing::Linear,
            W,
            H,
            frame.background,
            frame.background_alpha,
        );
        let bg = px(&read_back(&h), 48, 48);
        assert!(
            bg[3].abs_diff((255.0_f32 * alpha).round() as u8) <= 1,
            "parallax overlap {alpha}: {bg:?}"
        );
    }
}

use neomacs_renderer_wgpu::{
    SnapshotSize,
    shader_surface::{SurfaceContract, SurfaceShaderLanguage, compose_surface_wgsl},
};

#[test]
fn native_inset_clear_matches_completed_content_opacity() {
    use neomacs_display_protocol::ContentInsets;
    use neomacs_renderer_wgpu::renderer::{NativeContentPlacement, RenderTarget};
    let seed = WgpuRenderer::new(None, W, H).expect("GPU required for native inset regression");
    for format in [
        wgpu::TextureFormat::Bgra8UnormSrgb,
        wgpu::TextureFormat::Bgra8Unorm,
    ] {
        let renderer = WgpuRenderer::with_device(
            seed.device().clone(),
            seed.queue().clone(),
            W,
            H,
            format,
            1.0,
        );
        let target = renderer.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("native-inset-target"),
            size: wgpu::Extent3d {
                width: W,
                height: H,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = target.create_view(&Default::default());
        let atlas = WgpuGlyphAtlas::new(renderer.device());
        let mut h = Harness {
            renderer,
            target,
            view,
            atlas,
        };
        let surface = mapping_for(&FrameGlyphBuffer::with_size(W as f32, H as f32), W, H)
            .surface()
            .with_content_insets(ContentInsets::new(4, 8, 4, 4));
        let content = surface.content_surface().unwrap();
        let cw = content.device_width().get();
        let ch = content.device_height().get();
        let source = h
            .renderer
            .acquire_snapshot(SnapshotSize::new(cw, ch).unwrap())
            .unwrap();
        let mut frame = FrameGlyphBuffer::with_size(cw as f32, ch as f32);
        for background in [Color::GREEN, Color::rgb(0.15, 0.45, 0.75)] {
            frame.background = background;
            h.renderer.render_frame_glyphs(
                source.view(),
                &frame,
                &mut h.atlas,
                mapping_for(&frame, cw, ch),
                false,
                None,
                None,
                None,
                None,
                None,
            );
            for alpha in [1.0, 0.5, 0.25] {
                let placement =
                    NativeContentPlacement::new(RenderTarget::new(&h.view, surface), &source)
                        .unwrap();
                h.renderer
                    .place_native_content_with_opacity(placement, frame.background, alpha);
                let pixels = read_back(&h);
                let inset = px(&pixels, 2, 2);
                let center = px(&pixels, 48, 32);
                eprintln!(
                    "native inset {format:?} opacity {alpha}: inset {inset:?}, content {center:?}"
                );
                assert!(
                    inset.iter().zip(center).all(|(a, b)| a.abs_diff(b) <= 1),
                    "native inset and content must share storage convention: {format:?}/{alpha}: {inset:?} != {center:?}"
                );
                let straight = if format.is_srgb() {
                    background.linear_to_srgb()
                } else {
                    background
                };
                for (got, component) in center.iter().zip([straight.r, straight.g, straight.b, 1.0])
                {
                    assert!(
                        got.abs_diff((255.0 * alpha * component).round() as u8) <= 1,
                        "native premultiplied storage {format:?}/{alpha}: {center:?}"
                    );
                }
                assert!(
                    center[..3]
                        .iter()
                        .all(|c| *c <= center[3].saturating_add(1)),
                    "RGB must not exceed alpha"
                );
            }
        }
    }
}

fn picture() -> FrameGlyphBuffer {
    let mut frame = FrameGlyphBuffer::with_size(W as f32, H as f32);
    frame.background = Color::BLACK;
    frame.background_alpha = 0.5;
    frame.set_face(
        FaceId::new(0),
        Color::WHITE,
        Some(Color::BLACK),
        400,
        false,
        0,
        None,
        0,
        None,
        0,
        None,
    );
    frame.set_draw_context(DisplayWindowId::new(1), GlyphRowRole::Text, None);
    frame.add_char('M', 20.0, 16.0, 16.0, 24.0, 20.0, false);
    frame
}

fn render(h: &mut Harness, frame: &FrameGlyphBuffer, view: &wgpu::TextureView) {
    h.renderer.render_frame_glyphs(
        view,
        frame,
        &mut h.atlas,
        mapping_for(frame, W, H),
        false,
        None,
        None,
        None,
        None,
        None,
    );
}

fn opaque_text(buf: &[u8]) -> usize {
    (16..40)
        .flat_map(|y| (20..36).map(move |x| (x, y)))
        .filter(|&(x, y)| {
            let p = px(buf, x, y);
            p[0] > 245 && p[1] > 245 && p[2] > 245 && p[3] == 255
        })
        .count()
}

#[test]
fn background_alpha_keeps_text_opaque_through_crossfade() {
    let Some(mut h) = try_harness() else {
        eprintln!("SKIP: no GPU adapter");
        return;
    };
    let frame = picture();
    let size = SnapshotSize::new(W, H).unwrap();
    let old = h.renderer.acquire_snapshot(size).unwrap();
    let new = h.renderer.acquire_snapshot(size).unwrap();
    render(&mut h, &frame, old.view());
    render(&mut h, &frame, new.view());
    for mix in [0.0, 0.25, 0.5, 0.75, 1.0] {
        h.renderer.render_crossfade(
            &h.view,
            old.bind_group(),
            new.bind_group(),
            mix,
            &neomacs_display_protocol::types::Rect::new(0.0, 0.0, W as f32, H as f32),
            W,
            H,
        );
        let pixels = read_back(&h);
        assert!(
            (127..=129).contains(&px(&pixels, 80, 50)[3]),
            "background opacity must not compound at mix {mix}"
        );
        assert!(
            opaque_text(&pixels) > 0,
            "same foreground stays opaque at mix {mix}"
        );
    }
}

#[test]
fn background_alpha_survives_identity_frame_post() {
    let Some(mut h) = try_harness() else {
        eprintln!("SKIP: no GPU adapter");
        return;
    };
    let frame = picture();
    let src = h
        .renderer
        .acquire_snapshot(SnapshotSize::new(W, H).unwrap())
        .unwrap();
    render(&mut h, &frame, src.view());
    let source = compose_surface_wgsl(
        "fn mainImage(p: vec2<f32>) -> vec4<f32> { return textureSample(iChannel0, iChannel0Sampler, vec2<f32>(p.x, u.iResolution.y-p.y)/u.iResolution.xy); }",
        &[],
        SurfaceContract::V1,
    );
    h.renderer
        .set_frame_post(SurfaceShaderLanguage::Wgsl, &source, &[])
        .unwrap();
    h.renderer
        .frame_post_to_view(src.view(), &h.view, W, H, (0.0, 0.0));
    let pixels = read_back(&h);
    assert!((127..=129).contains(&px(&pixels, 80, 50)[3]));
    assert!(opaque_text(&pixels) > 0);
}

#[test]
fn child_background_alpha_and_lifecycle_opacity_apply_once() {
    let Some(mut h) = try_harness() else {
        eprintln!("SKIP: no GPU adapter");
        return;
    };
    h.renderer.resize(W, H);
    let mut parent = FrameGlyphBuffer::with_size(W as f32, H as f32);
    parent.background_alpha = 0.0;
    let child = picture();
    for radius in [0.0, 8.0] {
        for alpha in [1.0, 0.5] {
            h.renderer.render_frame_glyphs(
                &h.view,
                &parent,
                &mut h.atlas,
                mapping_for(&parent, W, H),
                false,
                None,
                None,
                None,
                None,
                None,
            );
            h.renderer
                .render_child_frame(
                    &h.view,
                    &child,
                    0.0,
                    0.0,
                    neomacs_display_protocol::RootSurfaceRect::new(0.0, 0.0, W as f32, H as f32)
                        .unwrap(),
                    &mut h.atlas,
                    W,
                    H,
                    false,
                    None,
                    radius,
                    false,
                    0,
                    0.0,
                    0.0,
                    None,
                    alpha,
                    1.0,
                    [0.0; 2],
                )
                .expect("child picture admitted");
            let pixels = read_back(&h);
            let expected = (128.0_f32 * alpha).round() as u8;
            assert!(
                px(&pixels, 80, 50)[3].abs_diff(expected) <= 1,
                "child background opacity: radius {radius}, alpha {alpha}"
            );
            if alpha == 1.0 {
                assert!(opaque_text(&pixels) > 0);
            }
        }
    }
}

#[test]
fn dispatched_crossfade_and_slide_replace_the_previous_picture() {
    use neomacs_display_protocol::{
        DirectionlessTransitionEffect, ResolvedTransitionEffect, TransitionAxis,
        TransitionDirection, TransitionEasing,
    };
    let Some(mut h) = try_harness() else {
        eprintln!("SKIP: no GPU adapter");
        return;
    };
    let frame = picture();
    let size = SnapshotSize::new(W, H).unwrap();
    let old = h.renderer.acquire_snapshot(size).unwrap();
    let new = h.renderer.acquire_snapshot(size).unwrap();
    render(&mut h, &frame, old.view());
    render(&mut h, &frame, new.view());
    let bounds = neomacs_display_protocol::types::Rect::new(0.0, 0.0, W as f32, H as f32);
    for mix in [0.0, 0.25, 0.5, 0.75, 1.0] {
        let destination = h.view.clone();
        render(&mut h, &frame, &destination);
        h.renderer.render_transition_effect(
            &h.view,
            old.bind_group(),
            new.bind_group(),
            mix,
            0.0,
            &bounds,
            ResolvedTransitionEffect::Directionless(DirectionlessTransitionEffect::Crossfade),
            TransitionEasing::Linear,
            W,
            H,
            frame.background,
            frame.background_alpha,
        );
        let pixels = read_back(&h);
        assert!((127..=129).contains(&px(&pixels, 80, 50)[3]));
        assert!(opaque_text(&pixels) > 0, "dispatched crossfade at {mix}");
    }
    for axis in [TransitionAxis::Horizontal, TransitionAxis::Vertical] {
        for direction in [TransitionDirection::Forward, TransitionDirection::Backward] {
            for mix in [0.0, 0.5, 1.0] {
                let destination = h.view.clone();
                render(&mut h, &frame, &destination);
                let distance = if axis == TransitionAxis::Horizontal {
                    W
                } else {
                    H
                } as f32;
                h.renderer.render_transition_slide(
                    &h.view,
                    old.bind_group(),
                    new.bind_group(),
                    mix,
                    axis,
                    direction,
                    &bounds,
                    distance,
                    W,
                    H,
                );
                let pixels = read_back(&h);
                assert!(
                    (127..=129).contains(&px(&pixels, 80, 50)[3]),
                    "slide {axis:?} {direction:?} {mix}"
                );
                assert!(
                    pixels
                        .chunks_exact(4)
                        .any(|p| p[0] > 245 && p[1] > 245 && p[2] > 245 && p[3] == 255)
                );
            }
        }
    }
}

#[test]
fn transition_postprocess_preserves_alpha_and_color_modulation() {
    use neomacs_display_protocol::{
        ResolvedTransitionEffect, TransitionDirection, TransitionEasing,
        VerticalPostProcessTransitionEffect,
    };
    let Some(mut h) = try_harness() else {
        eprintln!("SKIP: no GPU adapter");
        return;
    };
    let mut frame = picture();
    frame.background = Color::new(0.5, 0.5, 0.5, 1.0);
    let size = SnapshotSize::new(W, H).unwrap();
    let old = h.renderer.acquire_snapshot(size).unwrap();
    let new = h.renderer.acquire_snapshot(size).unwrap();
    render(&mut h, &frame, old.view());
    render(&mut h, &frame, new.view());
    let destination = h.view.clone();
    render(&mut h, &frame, &destination);
    let bounds = neomacs_display_protocol::types::Rect::new(0.0, 0.0, W as f32, H as f32);
    h.renderer.render_transition_effect(
        &h.view,
        old.bind_group(),
        new.bind_group(),
        0.0,
        0.0,
        &bounds,
        ResolvedTransitionEffect::VerticalPostProcess {
            effect: VerticalPostProcessTransitionEffect::ColorTemperature,
            direction: TransitionDirection::Forward,
            distance: H as f32,
        },
        TransitionEasing::Linear,
        W,
        H,
        frame.background,
        frame.background_alpha,
    );
    let pixels = read_back(&h);
    let bg = px(&pixels, 80, 50);
    assert!(
        (127..=129).contains(&bg[3]),
        "postprocess background: {bg:?}"
    );
    assert!(
        bg[0] > bg[2],
        "temperature tint must survive premultiplied copy: {bg:?}"
    );
    assert!(
        pixels
            .chunks_exact(4)
            .any(|p| p[0] > 245 && p[1] > 245 && p[2] > 230 && p[3] == 255)
    );
}

fn draw_popup(h: &mut Harness, child: &FrameGlyphBuffer, radius: f32, alpha: f32) {
    h.renderer
        .render_child_frame(
            &h.view,
            child,
            0.0,
            0.0,
            neomacs_display_protocol::RootSurfaceRect::new(0.0, 0.0, W as f32, H as f32).unwrap(),
            &mut h.atlas,
            W,
            H,
            false,
            None,
            radius,
            false,
            0,
            0.0,
            0.0,
            None,
            alpha,
            1.0,
            [0.0; 2],
        )
        .expect("child picture admitted");
}

#[test]
fn colored_uncovered_child_fill_matches_square_and_rounded() {
    let mut h = try_harness().expect("GPU adapter required for opacity regression");
    let mut parent = FrameGlyphBuffer::with_size(W as f32, H as f32);
    parent.background_alpha = 0.0;
    let mut child = parent.clone();
    child.background = Color::GREEN;
    child.background_alpha = 0.5;
    for alpha in [1.0, 0.5] {
        let mut centers = Vec::new();
        for radius in [0.0, 8.0] {
            let view = h.view.clone();
            render(&mut h, &parent, &view);
            draw_popup(&mut h, &child, radius, alpha);
            let pixels = read_back(&h);
            let center = px(&pixels, 48, 32);
            let expected_green = if alpha == 1.0 { 187 } else { 137 };
            assert!(
                center[1].abs_diff(expected_green) <= 1,
                "{radius}/{alpha}: {center:?}"
            );
            assert!(center[3].abs_diff((128.0_f32 * alpha).round() as u8) <= 1);
            if radius > 0.0 {
                assert_eq!(px(&pixels, 0, 0), [0; 4], "rounded corner clips fill");
                let edge = px(&pixels, 4, 0);
                assert!(edge[3] < center[3], "rounded edge retains coverage");
                assert!(edge[1] > 0, "colored antialiased edge is not lost");
            }
            centers.push(center);
        }
        assert_eq!(centers[0], centers[1]);
    }
}

#[test]
fn child_char_and_stretch_stipple_is_foreground_at_zero_background() {
    use neomacs_display_protocol::StipplePattern;
    let mut h = try_harness().expect("GPU adapter required for stipple regression");
    let mut parent = FrameGlyphBuffer::with_size(W as f32, H as f32);
    parent.background_alpha = 0.0;
    for stretch in [false, true] {
        for background_alpha in [0.0, 0.5] {
            let mut child = parent.clone();
            child.background = Color::BLACK;
            child.background_alpha = background_alpha;
            child.set_face(
                FaceId::new(1),
                Color::WHITE,
                Some(Color::BLACK),
                400,
                false,
                0,
                None,
                0,
                None,
                0,
                None,
            );
            child.faces.get_mut(&FaceId::new(1)).unwrap().stipple =
                Some(Box::new(StipplePattern {
                    width: 1,
                    height: 1,
                    bits: vec![1],
                }));
            child.set_draw_context(DisplayWindowId::new(1), GlyphRowRole::Text, None);
            if stretch {
                child.add_stretch(20.0, 16.0, 20.0, 20.0, Color::BLACK, FaceId::new(1), false);
            } else {
                child.add_char(' ', 20.0, 16.0, 20.0, 20.0, 16.0, false);
            }
            for radius in [0.0, 8.0] {
                for alpha in [1.0, 0.5] {
                    let view = h.view.clone();
                    render(&mut h, &parent, &view);
                    draw_popup(&mut h, &child, radius, alpha);
                    let pixels = read_back(&h);
                    let p = px(&pixels, 25, 20);
                    let expected = if alpha == 1.0 { 255 } else { 187 };
                    assert!(
                        p[0].abs_diff(expected) <= 1 && p[1] == p[0] && p[2] == p[0],
                        "{stretch}/{background_alpha}/{radius}/{alpha}: {p:?}"
                    );
                    assert!(p[3].abs_diff((255.0_f32 * alpha).round() as u8) <= 1);
                    assert_eq!(
                        px(&pixels, 10, 10)[3],
                        (255.0 * background_alpha * alpha).round() as u8
                    );
                }
            }
        }
    }
}

/// Run the test named `exact` in a child test process under a 1 MiB GPU
/// budget. Returns true inside that child, where the caller runs its body.
/// In the parent it requires the child to have run exactly one test and
/// passed: libtest exits successfully when `--exact` matches nothing.
fn in_gpu_budget_child(exact: &str) -> bool {
    if std::env::var("NEOMACS_GPU_BUDGET_MB").as_deref() == Ok("1") {
        return true;
    }
    let out = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", exact, "--nocapture"])
        .env("NEOMACS_GPU_BUDGET_MB", "1")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let mut results = stdout
        .lines()
        .filter_map(|line| line.strip_prefix("test result: ok. "));
    let counts = match (results.next(), results.next()) {
        (Some(counts), None) => counts,
        _ => "",
    };
    let count = |label: &str| {
        counts
            .split("; ")
            .find_map(|part| part.strip_suffix(label)?.parse::<u32>().ok())
    };
    assert!(
        out.status.success() && count(" passed") == Some(1) && count(" failed") == Some(0),
        "budget child for {exact} did not run exactly one passing test\n{stdout}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    false
}

/// Run in the bounded GPU lane with NEOMACS_GPU_BUDGET_MB=1. Real held
/// leases deny the required picture; no opaque substitute is rendered.
#[test]
fn mandatory_child_picture_refusal_is_typed_and_recovers_after_lease_release() {
    // Isolate the budget knob in a child test process: never mutate the
    // environment of other concurrently running GPU tests.
    if !in_gpu_budget_child(
        "opacity_test::mandatory_child_picture_refusal_is_typed_and_recovers_after_lease_release",
    ) {
        return;
    }
    let mut h = try_harness().expect("real GPU adapter required");
    assert_eq!(h.renderer.gpu_budget().limit_bytes().get(), 1_048_576);
    let mut parent = FrameGlyphBuffer::with_size(W as f32, H as f32);
    parent.background = Color::BLUE;
    let mut child = FrameGlyphBuffer::with_size(W as f32, H as f32);
    child.background = Color::GREEN;
    child.background_alpha = 0.5;
    let draw = |h: &mut Harness, child: &FrameGlyphBuffer| {
        h.renderer.render_child_frame(
            &h.view,
            child,
            0.0,
            0.0,
            neomacs_display_protocol::RootSurfaceRect::new(0.0, 0.0, W as f32, H as f32).unwrap(),
            &mut h.atlas,
            W,
            H,
            false,
            None,
            0.0,
            false,
            0,
            0.0,
            0.0,
            None,
            1.0,
            1.0,
            [0.0; 2],
        )
    };
    let view = h.view.clone();
    render(&mut h, &parent, &view);
    draw(&mut h, &child).unwrap();
    let normal = read_back(&h);
    let held = h
        .renderer
        .acquire_snapshot(SnapshotSize::new(512, 506).unwrap())
        .unwrap();
    let view = h.view.clone();
    render(&mut h, &parent, &view);
    let before = read_back(&h);
    assert!(
        draw(&mut h, &child).is_err(),
        "mandatory refusal must reach caller"
    );
    assert_eq!(read_back(&h), before);
    child.background_alpha = 1.0;
    draw(&mut h, &child).unwrap(); // positive opaque control needs no picture
    assert_eq!(px(&read_back(&h), 48, 32), [0, 255, 0, 255]);
    drop(held);
    child.background_alpha = 0.5;
    let view = h.view.clone();
    render(&mut h, &parent, &view);
    draw(&mut h, &child).unwrap();
    assert_eq!(read_back(&h), normal);
}

#[test]
fn source_over_transitions_fill_uncovered_geometry_with_the_background() {
    use neomacs_display_protocol::{
        HorizontalTransitionEffect, ResolvedTransitionEffect, TransitionAxis, TransitionDirection,
        TransitionEasing, VerticalTransitionEffect,
    };
    let mut h = try_harness().expect("GPU required for transition coverage regression");
    let size = SnapshotSize::new(W, H).unwrap();
    let old = h.renderer.acquire_snapshot(size).unwrap();
    let new = h.renderer.acquire_snapshot(size).unwrap();
    let bounds = neomacs_display_protocol::types::Rect::new(8.0, 8.0, 80.0, 48.0);
    let effects = [
        // Mid-flip the picture spans only x in roughly 36..60.
        (
            ResolvedTransitionEffect::CardFlip {
                axis: TransitionAxis::Horizontal,
            },
            0.4,
        ),
        // Halfway through a scroll twice the region's height, both tilted
        // pictures sit wholly outside it.
        (
            ResolvedTransitionEffect::Vertical {
                effect: VerticalTransitionEffect::Tilt,
                direction: TransitionDirection::Forward,
                distance: 96.0,
            },
            0.5,
        ),
        // Past halfway the old picture has faded out entirely while the
        // last lines have not begun their reveal.
        (
            ResolvedTransitionEffect::Horizontal {
                effect: HorizontalTransitionEffect::TypewriterReveal,
                direction: TransitionDirection::Forward,
            },
            0.6,
        ),
    ];
    for alpha in [1.0, 0.5] {
        let mut frame = picture();
        frame.background = Color::GREEN;
        frame.background_alpha = alpha;
        render(&mut h, &frame, old.view());
        render(&mut h, &frame, new.view());
        let expected = (255.0_f32 * alpha).round() as u8;
        for (effect, progress) in effects {
            let view = h.view.clone();
            render(&mut h, &frame, &view);
            let before = read_back(&h);
            h.renderer.render_transition_effect(
                &h.view,
                old.bind_group(),
                new.bind_group(),
                progress,
                0.0,
                &bounds,
                effect,
                TransitionEasing::Linear,
                W,
                H,
                frame.background,
                frame.background_alpha,
            );
            let pixels = read_back(&h);
            if matches!(effect, ResolvedTransitionEffect::Horizontal { .. }) {
                let p = px(&pixels, 86, 54);
                assert!(
                    p[3].abs_diff(expected) <= 1,
                    "unrevealed typewriter line at alpha {alpha}: {p:?}"
                );
                continue;
            }
            // No pixel of the transitioned region is left more transparent
            // than the frame background, wherever the pictures moved.
            let thinnest = (8..56)
                .flat_map(|y| (8..88).map(move |x| (x, y)))
                .min_by_key(|&(x, y)| px(&pixels, x, y)[3])
                .unwrap();
            let p = px(&pixels, thinnest.0, thinnest.1);
            eprintln!("{effect:?} {alpha}: thinnest {thinnest:?} {p:?}");
            assert!(
                p[3] >= expected.saturating_sub(1),
                "{effect:?} uncovered pixel {thinnest:?} at alpha {alpha}: {p:?}"
            );
            if matches!(effect, ResolvedTransitionEffect::CardFlip { .. }) {
                // Mid-flip the corners are background only, at exactly its alpha.
                for (x, y) in [(9, 9), (9, 32), (86, 54)] {
                    let p = px(&pixels, x, y);
                    assert!(
                        p[3].abs_diff(expected) <= 1,
                        "CardFlip uncovered pixel ({x},{y}) at alpha {alpha}: {p:?}"
                    );
                }
            }
            assert_eq!(px(&pixels, 2, 2), px(&before, 2, 2));
            assert_eq!(px(&pixels, 94, 62), px(&before, 94, 62));
        }
    }
}
