use super::*;

/// Where a pointer at surface `(x, y)` lands while nothing is moving.
///
/// A settled projection is the identity, so a fixture built with this
/// reads exactly as it did when this hit test took two `f32`s — and,
/// unlike two `f32`s, it had to come from a projection to exist.
fn settled_frame_point(x: f32, y: f32) -> PresentationFramePoint {
    neomacs_display_protocol::InteractionProjection::settled(
        neomacs_display_protocol::PresentationId::new(1),
    )
    .map(
        neomacs_display_protocol::GeometryPoint::<
            neomacs_display_protocol::RootSurfaceSpace,
            neomacs_display_protocol::LogicalPixels,
        >::from_px(x, y)
        .expect("a fixture's coordinates are finite"),
    )
    .expect("the identity of a finite point stays representable")
}

fn presentation(
    x: f32,
    y: f32,
    content: neomacs_display_protocol::XwidgetContentExtent,
    advance: f32,
) -> neomacs_display_protocol::XwidgetPresentationGeometry<neomacs_display_protocol::FrameSpace> {
    neomacs_display_protocol::XwidgetPresentationGeometry::new(
        neomacs_display_protocol::GeometryPoint::<
            neomacs_display_protocol::FrameSpace,
            neomacs_display_protocol::LogicalPixels,
        >::from_px(x, y)
        .expect("valid origin"),
        content,
        neomacs_display_protocol::XwidgetLayoutAdvance::new(neomacs_display_protocol::Px(advance))
            .expect("valid advance"),
        None,
    )
}

#[test]
fn hit_testing_keeps_xwidget_and_webview_identities_distinct() {
    let mut glyphs = neomacs_display_protocol::FrameGlyphBuffer::new();
    let content =
        neomacs_display_protocol::XwidgetContentExtent::new(320.0, 200.0).expect("extent");
    glyphs.add_xwidget(
        neomacs_display_protocol::XwidgetId::new(7),
        neomacs_display_protocol::WebViewId::new(91),
        presentation(10.0, 20.0, content, 320.0),
    );

    assert_eq!(
        webview_glyph_hit_test(&glyphs.glyphs, settled_frame_point(42.5, 75.0)),
        Some(WebViewPointerHit {
            view: neomacs_display_protocol::WebViewId::new(91),
            position: WebContentPoint::new(32.5, 55.0),
        })
    );
}

#[test]
fn hit_testing_rejects_the_clipped_part_of_an_xwidget() {
    let mut glyphs = neomacs_display_protocol::FrameGlyphBuffer::new();
    glyphs.set_draw_context(
        neomacs_display_protocol::DisplayWindowId::new(1),
        neomacs_display_protocol::GlyphRowRole::Text,
        Some(neomacs_display_protocol::Rect::new(20.0, 30.0, 50.0, 40.0)),
    );
    let content = neomacs_display_protocol::XwidgetContentExtent::new(100.0, 80.0).expect("extent");
    glyphs.add_xwidget(
        neomacs_display_protocol::XwidgetId::new(7),
        neomacs_display_protocol::WebViewId::new(91),
        presentation(10.0, 20.0, content, 100.0),
    );

    assert_eq!(
        webview_glyph_hit_test(&glyphs.glyphs, settled_frame_point(15.0, 25.0)),
        None
    );
    assert!(webview_glyph_hit_test(&glyphs.glyphs, settled_frame_point(25.0, 35.0)).is_some());
}

/// A slot cropped at the right edge does not shrink the pointer target:
/// the widget is still its own size behind the text-area clip, so a
/// point inside the clip but past the cropped slot still hits it.
#[test]
fn hit_testing_uses_the_widgets_own_extent_not_the_cropped_slot() {
    let mut glyphs = neomacs_display_protocol::FrameGlyphBuffer::new();
    glyphs.set_draw_context(
        neomacs_display_protocol::DisplayWindowId::new(1),
        neomacs_display_protocol::GlyphRowRole::Text,
        Some(neomacs_display_protocol::Rect::new(0.0, 0.0, 400.0, 100.0)),
    );
    let content = neomacs_display_protocol::XwidgetContentExtent::new(600.0, 40.0).expect("extent");
    glyphs.add_xwidget(
        neomacs_display_protocol::XwidgetId::new(7),
        neomacs_display_protocol::WebViewId::new(91),
        presentation(8.0, 10.0, content, 304.0),
    );

    assert_eq!(
        webview_glyph_hit_test(&glyphs.glyphs, settled_frame_point(350.0, 20.0)),
        Some(WebViewPointerHit {
            view: neomacs_display_protocol::WebViewId::new(91),
            position: WebContentPoint::new(342.0, 10.0),
        })
    );
    assert_eq!(
        webview_glyph_hit_test(&glyphs.glyphs, settled_frame_point(450.0, 20.0)),
        None,
        "past the clip nothing of the widget is visible"
    );
}
