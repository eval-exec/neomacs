//! A scrolling crop must overwrite exactly its viewport and preserve neighbors.
use super::*;
use neomacs_display_protocol::Rect;
use neomacs_renderer_wgpu::SnapshotSize;
use neomacs_renderer_wgpu::renderer::{RenderTarget, SnapshotRegion};

#[test]
fn pooled_scroll_crop_preserves_neighbors_and_reverses_at_multiple_scales() {
    let Some(mut h) = try_harness() else { return };
    for scale in [1.0, 2.0] {
        let source = h
            .renderer
            .acquire_snapshot(SnapshotSize::new(W, H).unwrap())
            .unwrap();
        let mut content = FrameGlyphBuffer::with_size(W as f32 / scale, H as f32 / scale);
        content.background = Color::BLACK;
        content.set_draw_context(DisplayWindowId::new(1), GlyphRowRole::Text, None);
        content.add_stretch(
            0.0,
            0.0,
            W as f32 / scale,
            32.0 / scale,
            Color::GREEN,
            FaceId::new(0),
            false,
        );
        content.add_stretch(
            0.0,
            32.0 / scale,
            W as f32 / scale,
            32.0 / scale,
            Color::BLUE,
            FaceId::new(0),
            false,
        );
        content.add_stretch(
            8.0 / scale,
            0.0,
            16.0 / scale,
            H as f32 / scale,
            Color::WHITE,
            FaceId::new(0),
            false,
        );
        h.renderer.render_frame_glyphs(
            source.view(),
            &content,
            &mut h.atlas,
            mapping_for_scale(&content, W, H, scale),
            false,
            None,
            None,
            None,
            None,
            None,
        );
        let mut background = FrameGlyphBuffer::with_size(W as f32 / scale, H as f32 / scale);
        background.background = Color::RED;
        h.renderer.render_frame_glyphs(
            &h.view,
            &background,
            &mut h.atlas,
            mapping_for_scale(&background, W, H, scale),
            false,
            None,
            None,
            None,
            None,
            None,
        );
        let destination =
            FrameRect::new(16.0 / scale, 8.0 / scale, 32.0 / scale, 16.0 / scale).unwrap();
        // Forward and reverse select different portions of the SAME raster;
        // no glyph render runs between these crops.
        for source_y in [0.0, 24.0, 32.0, 0.0] {
            let region =
                SnapshotRegion::new(&source, Rect::new(8.0, source_y, 32.0, 16.0), destination)
                    .unwrap();
            h.renderer
                .begin_draw(RenderTarget::new(
                    &h.view,
                    mapping_for_scale(&background, W, H, scale).surface(),
                ))
                .blit_snapshot_region(region);
            let pixels = read_tex(&h.renderer, &h.target);
            for y in 0..H {
                for x in 0..W {
                    let want = if (16..48).contains(&x) && (8..24).contains(&y) {
                        if x < 32 {
                            [255, 255, 255, 255]
                        } else if source_y + ((y - 8) as f32) < 32.0 {
                            [0, 255, 0, 255]
                        } else {
                            [0, 0, 255, 255]
                        }
                    } else {
                        [255, 0, 0, 255]
                    };
                    let got = pxb(&pixels, x, y);
                    assert!(
                        got.iter()
                            .zip(want)
                            .all(|(&a, b)| (i32::from(a) - b).abs() <= 1),
                        "scale={scale} source_y={source_y} ({x},{y}): {got:?} != {want:?}"
                    );
                }
            }
        }
        for invalid in [
            Rect::new(-1.0, 0.0, 10.0, 10.0),
            Rect::new(0.0, 60.0, 10.0, 10.0),
            Rect::new(0.0, 0.0, f32::INFINITY, 1.0),
            Rect::new(0.0, 0.0, 0.0, 1.0),
        ] {
            assert!(SnapshotRegion::new(&source, invalid, destination).is_none());
        }
    }
}
