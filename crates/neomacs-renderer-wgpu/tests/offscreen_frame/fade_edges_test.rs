//! Real dispatch regressions: complementary FadeEdges pictures, not occlusion.
use super::*;
use neomacs_display_protocol::{
    ResolvedTransitionEffect, TransitionDirection, TransitionEasing, TransitionEdge,
    VerticalTransitionEffect, types::Rect,
};
use neomacs_renderer_wgpu::SnapshotSize;

const HEIGHT: u32 = 112;

fn harness() -> Harness {
    let renderer =
        WgpuRenderer::new(None, W, HEIGHT).expect("GPU required for FadeEdges regression");
    let atlas = WgpuGlyphAtlas::new(renderer.device());
    let target = renderer.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("fade-edges-target"),
        size: wgpu::Extent3d {
            width: W,
            height: HEIGHT,
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
    Harness {
        renderer,
        atlas,
        target,
        view,
    }
}

fn picture(background: Color, alpha: f32) -> FrameGlyphBuffer {
    let mut frame = FrameGlyphBuffer::with_size(W as f32, HEIGHT as f32);
    frame.background = background;
    frame.background_alpha = alpha;
    frame.set_face(
        FaceId::new(0),
        Color::WHITE,
        Some(background),
        400,
        false,
        0,
        None,
        0,
        None,
        0,
        None,
    );
    frame.faces.get_mut(&FaceId::new(0)).unwrap().stipple =
        Some(Box::new(neomacs_display_protocol::StipplePattern {
            width: 1,
            height: 1,
            bits: vec![1],
        }));
    frame.set_draw_context(DisplayWindowId::new(1), GlyphRowRole::Text, None);
    frame.add_stretch(24.0, 16.0, 16.0, 80.0, background, FaceId::new(0), false);
    frame
}

fn render(h: &mut Harness, frame: &FrameGlyphBuffer, view: &wgpu::TextureView) {
    h.renderer.render_frame_glyphs(
        view,
        frame,
        &mut h.atlas,
        mapping_for(frame, W, HEIGHT),
        false,
        None,
        None,
        None,
        None,
        None,
    );
}

fn pixels(h: &Harness) -> Vec<u8> {
    let row =
        (W * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let buffer = h.renderer.device().create_buffer(&wgpu::BufferDescriptor {
        label: Some("fade-edges-readback"),
        size: (row * HEIGHT) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = h
        .renderer
        .device()
        .create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &h.target,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row),
                rows_per_image: Some(HEIGHT),
            },
        },
        wgpu::Extent3d {
            width: W,
            height: HEIGHT,
            depth_or_array_layers: 1,
        },
    );
    h.renderer.queue().submit(Some(encoder.finish()));
    buffer.slice(..).map_async(wgpu::MapMode::Read, |_| {});
    h.renderer
        .device()
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(std::time::Duration::from_secs(3)),
        })
        .unwrap();
    let data = buffer.slice(..).get_mapped_range().unwrap();
    data.chunks_exact(row as usize)
        .flat_map(|r| r[..(W * 4) as usize].iter().copied())
        .collect()
}

fn expected(old_weight: f32, new_weight: f32, alpha: f32) -> [u8; 4] {
    let color = Color::new(
        0.0,
        alpha * old_weight,
        alpha * new_weight,
        alpha * (old_weight + new_weight),
    )
    .linear_to_srgb();
    [color.r, color.g, color.b, color.a].map(|v| (v * 255.0).round() as u8)
}

fn close(actual: [u8; 4], expected: [u8; 4], context: &str) {
    assert!(
        actual.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 2),
        "{context}: {actual:?} != {expected:?}"
    );
}

fn outside_unchanged(before: &[u8], after: &[u8]) {
    for y in 0..HEIGHT {
        for x in 0..W {
            if !(8..88).contains(&x) || !(8..104).contains(&y) {
                assert_eq!(
                    px(before, x, y),
                    px(after, x, y),
                    "outside scissor ({x},{y})"
                );
            }
        }
    }
}

// Independent geometric oracle for the edge attenuation at a pixel center:
// 16 six-pixel strips over a 96-pixel viewport; no copied shader or blend pass.
fn edge_weight(y: f32, offset: f32, weight: f32) -> f32 {
    let index = ((y - offset) / 6.0).floor();
    if !(0.0..16.0).contains(&index) {
        return 0.0;
    }
    let center = (index * 6.0 + 3.0 + offset).clamp(0.0, 96.0);
    let edge = (center.min(96.0 - center) / 14.4).min(1.0);
    weight * edge
}

fn fade_edges(alpha: f32) {
    let mut h = harness();
    let size = SnapshotSize::new(W, HEIGHT).unwrap();
    let old = h.renderer.acquire_snapshot(size).unwrap();
    let new = h.renderer.acquire_snapshot(size).unwrap();
    render(&mut h, &picture(Color::GREEN, alpha), old.view());
    render(&mut h, &picture(Color::BLUE, alpha), new.view());
    // Midpoint first makes the reported failure the actual overlapping-picture
    // defect, not endpoint attenuation. Both configured scroll directions run.
    for direction in [TransitionDirection::Forward, TransitionDirection::Backward] {
        for progress in [0.5, 0.0, 0.25, 0.75, 1.0] {
            let view = h.view.clone();
            render(&mut h, &picture(Color::rgb(0.3, 0.1, 0.2), 0.75), &view);
            let before = pixels(&h);
            h.renderer.render_transition_effect(
                &h.view,
                old.bind_group(),
                new.bind_group(),
                progress,
                0.0,
                &Rect::new(8.0, 8.0, 80.0, 96.0),
                ResolvedTransitionEffect::Vertical {
                    effect: VerticalTransitionEffect::FadeEdges,
                    direction,
                    distance: 8.0,
                },
                TransitionEasing::Linear,
                W,
                HEIGHT,
                Color::BLUE,
                alpha,
            );
            let after = pixels(&h);
            let bg = px(&after, 70, 56);
            eprintln!(
                "FadeEdges {alpha}/{direction:?}/{progress}: background {bg:?}, foreground {:?}, edge {:?}",
                px(&after, 30, 56),
                px(&after, 70, 12)
            );
            close(
                bg,
                expected(1.0 - progress, progress, alpha),
                "complementary interior pictures",
            );
            close(px(&after, 30, 56), [255; 4], "opaque stipple foreground");
            let sign = direction.sign();
            for y in [10, 12, 100, 102] {
                let local = y as f32 + 0.5 - 8.0;
                let old_weight = edge_weight(local, -sign * 8.0 * progress, 1.0 - progress);
                let new_weight = edge_weight(local, sign * 8.0 * (1.0 - progress), progress);
                close(
                    px(&after, 70, y),
                    expected(old_weight, new_weight, alpha),
                    "edge attenuation",
                );
            }
            outside_unchanged(&before, &after);
        }
    }
}

#[test]
fn dispatched_fade_edges_preserves_fractional_complementary_pictures() {
    fade_edges(0.5);
}

#[test]
fn dispatched_fade_edges_preserves_opaque_complementary_pictures() {
    fade_edges(1.0);
}

#[test]
fn dispatched_page_curl_keeps_ordered_source_over_occlusion() {
    let mut h = harness();
    let size = SnapshotSize::new(W, HEIGHT).unwrap();
    let old = h.renderer.acquire_snapshot(size).unwrap();
    let new = h.renderer.acquire_snapshot(size).unwrap();
    for alpha in [0.5, 1.0] {
        render(&mut h, &picture(Color::GREEN, alpha), old.view());
        render(&mut h, &picture(Color::BLUE, alpha), new.view());
        let view = h.view.clone();
        render(&mut h, &picture(Color::BLACK, 0.25), &view);
        let before = pixels(&h);
        h.renderer.render_transition_effect(
            &h.view,
            old.bind_group(),
            new.bind_group(),
            0.0,
            0.0,
            &Rect::new(8.0, 8.0, 80.0, 96.0),
            ResolvedTransitionEffect::PageCurl {
                edge: TransitionEdge::Right,
            },
            TransitionEasing::Linear,
            W,
            HEIGHT,
            Color::BLUE,
            alpha,
        );
        let after = pixels(&h);
        // Old picture covers the new one, not a complementary replacement.
        close(
            px(&after, 70, 56),
            expected(1.0, 1.0 - alpha, alpha),
            "PageCurl ordered occlusion",
        );
        outside_unchanged(&before, &after);
    }
}
