#[cfg(feature = "video")]
use neomacs_video::{VideoGeometry, VideoRotation};

#[cfg(feature = "video")]
use super::video_quad_vertices;

#[cfg(all(feature = "webview", target_os = "linux"))]
#[test]
fn inline_webview_preserves_content_crop_and_child_frame_offset() {
    use neomacs_display_protocol::{
        FrameSpace, GeometryPoint, LogicalPixels, Px, Rect, XwidgetLayoutAdvance,
        XwidgetPresentationGeometry,
    };

    let mut glyphs = neomacs_display_protocol::FrameGlyphBuffer::new();
    glyphs.set_draw_context(
        neomacs_display_protocol::DisplayWindowId::new(1),
        neomacs_display_protocol::GlyphRowRole::Text,
        Some(Rect::new(0.0, 0.0, 312.0, 120.0)),
    );
    let content = neomacs_display_protocol::XwidgetContentExtent::new(600.0, 40.0)
        .expect("valid xwidget content extent");
    let presentation = XwidgetPresentationGeometry::new(
        GeometryPoint::<FrameSpace, LogicalPixels>::from_px(8.0, 16.0).expect("valid origin"),
        content,
        XwidgetLayoutAdvance::new(Px(304.0)).expect("valid cropped advance"),
        None,
    );
    glyphs.add_xwidget(
        neomacs_display_protocol::XwidgetId::new(7),
        neomacs_display_protocol::WebViewId::new(91),
        presentation,
    );

    let quad = super::inline_webview_quad(&glyphs.glyphs[0], 100.0, 50.0)
        .expect("the visible xwidget produces a quad");

    assert_eq!(quad.id, neomacs_display_protocol::WebViewId::new(91));
    assert_eq!(
        quad.vertices.map(|vertex| vertex.position),
        [
            [108.0, 66.0],
            [412.0, 66.0],
            [412.0, 106.0],
            [108.0, 66.0],
            [412.0, 106.0],
            [108.0, 106.0],
        ]
    );
    assert_eq!(
        quad.vertices.map(|vertex| vertex.tex_coords),
        [
            [0.0, 0.0],
            [304.0 / 600.0, 0.0],
            [304.0 / 600.0, 1.0],
            [0.0, 0.0],
            [304.0 / 600.0, 1.0],
            [0.0, 1.0],
        ]
    );
}

#[test]
#[cfg(feature = "video")]
fn video_quad_uses_the_native_sampling_transform_instead_of_full_uvs() {
    let mut geometry = VideoGeometry::packed(4, 2);
    geometry.rotation = VideoRotation::Clockwise90;
    let vertices = video_quad_vertices(
        3.0,
        5.0,
        20.0,
        10.0,
        geometry
            .sampling_transform()
            .coordinates_for_destination_rect(0.0, 1.0, 0.25, 0.75),
        0.5,
    );

    assert_eq!(
        vertices.map(|vertex| vertex.tex_coords),
        [
            [0.25, 1.0],
            [0.25, 0.0],
            [0.75, 0.0],
            [0.25, 1.0],
            [0.75, 0.0],
            [0.75, 1.0],
        ]
    );
    assert!(vertices.iter().all(|vertex| vertex.color[3] == 0.5));
}
