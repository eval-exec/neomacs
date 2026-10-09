use super::{XwidgetContentExtent, XwidgetLayoutAdvance, XwidgetPresentationGeometry};
use crate::{
    FrameSpace, GeometryPoint, GeometryRect, LogicalPixels, Px, RootSurfaceSpace, SpaceTranslation,
};

#[test]
fn a_content_extent_needs_two_finite_positive_dimensions() {
    let extent = XwidgetContentExtent::new(600.0, 40.0).expect("valid extent");
    assert_eq!(extent.width_px(), 600.0);
    assert_eq!(extent.height_px(), 40.0);

    assert_eq!(XwidgetContentExtent::new(0.0, 40.0), None);
    assert_eq!(XwidgetContentExtent::new(600.0, -1.0), None);
    assert_eq!(XwidgetContentExtent::new(f32::NAN, 40.0), None);
    assert_eq!(XwidgetContentExtent::new(600.0, f32::INFINITY), None);
}

#[test]
fn deserialization_goes_through_the_constructor() {
    let extent = XwidgetContentExtent::new(600.0, 40.0).expect("valid extent");
    let json = serde_json::to_string(&extent).expect("serialize");
    assert_eq!(json, r#"{"width_px":600.0,"height_px":40.0}"#);
    let round_trip: XwidgetContentExtent = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(round_trip, extent);

    for rejected in [
        r#"{"width_px":0.0,"height_px":40.0}"#,
        r#"{"width_px":600.0,"height_px":-1.0}"#,
        r#"{"width_px":600.0,"height_px":null}"#,
    ] {
        let error = serde_json::from_str::<XwidgetContentExtent>(rejected)
            .expect_err("an extent the constructor refuses must not deserialize");
        assert!(
            error
                .to_string()
                .contains("not finite and strictly positive")
                || error.to_string().contains("invalid type"),
            "{rejected}: {error}"
        );
    }
}

/// GNU keeps the widget, glyph, and clip extents separate.  This is the
/// protocol seam consumed by native placement, Linux composition, pointer
/// hit-testing, and cursor geometry; callers should not need to reconstruct
/// those meanings from three unrelated `f32` fields.
#[test]
fn presentation_geometry_keeps_content_advance_and_clip_distinct() {
    let content = XwidgetContentExtent::new(600.0, 40.0).expect("content extent");
    let advance = XwidgetLayoutAdvance::new(Px(304.0)).expect("layout advance");
    let origin =
        GeometryPoint::<FrameSpace, LogicalPixels>::from_px(8.0, 16.0).expect("frame-local origin");
    let clip = GeometryRect::<FrameSpace, LogicalPixels>::new(0.0, 0.0, 312.0, 120.0)
        .expect("frame-local clip");
    let presentation = XwidgetPresentationGeometry::new(origin, content, advance, Some(clip));

    let slot = presentation.layout_slot_rect();
    assert_eq!(
        (slot.x(), slot.y(), slot.width(), slot.height()),
        (8.0, 16.0, 304.0, 40.0)
    );

    let visible = presentation
        .resolve_visible(None)
        .expect("valid geometry")
        .expect("part of the widget is visible");
    assert_eq!(visible.content_rect().width(), 600.0);
    assert_eq!(visible.visible_rect().width(), 304.0);
    assert_eq!(
        visible.texture_coordinates().as_array(),
        [0.0, 304.0 / 600.0, 0.0, 1.0]
    );

    let point = GeometryPoint::<FrameSpace, LogicalPixels>::from_px(300.0, 20.0)
        .expect("frame-local pointer");
    let content_point = visible
        .content_point_at(point)
        .expect("point lies in the visible portion");
    assert_eq!((content_point.x(), content_point.y()), (292.0, 4.0));
}

#[test]
fn an_xwidget_layout_advance_is_finite_and_strictly_positive() {
    assert_eq!(
        XwidgetLayoutAdvance::new(Px(304.0)).unwrap().px(),
        Px(304.0)
    );
    assert_eq!(XwidgetLayoutAdvance::new(Px(0.0)), None);
    assert_eq!(XwidgetLayoutAdvance::new(Px(-1.0)), None);
    assert_eq!(XwidgetLayoutAdvance::new(Px(f32::NAN)), None);
    assert_eq!(XwidgetLayoutAdvance::new(Px(f32::INFINITY)), None);
}

#[test]
fn presentation_translation_changes_coordinate_space_at_compile_time() {
    let content = XwidgetContentExtent::new(600.0, 40.0).expect("content extent");
    let presentation = XwidgetPresentationGeometry::new(
        GeometryPoint::<FrameSpace, LogicalPixels>::from_px(8.0, 16.0).unwrap(),
        content,
        XwidgetLayoutAdvance::new(Px(304.0)).unwrap(),
        None,
    );
    let frame_to_root =
        SpaceTranslation::<FrameSpace, RootSurfaceSpace, LogicalPixels>::from_px(100.0, 50.0)
            .unwrap();

    let rooted: XwidgetPresentationGeometry<RootSurfaceSpace> =
        presentation.translated(frame_to_root).unwrap();

    assert_eq!(
        (rooted.content_rect().x(), rooted.content_rect().y()),
        (108.0, 66.0)
    );
}

#[test]
fn moving_a_glyph_origin_does_not_move_its_window_clip() {
    let presentation = XwidgetPresentationGeometry::new(
        GeometryPoint::<FrameSpace, LogicalPixels>::from_px(8.0, 16.0).unwrap(),
        XwidgetContentExtent::new(600.0, 40.0).unwrap(),
        XwidgetLayoutAdvance::new(Px(304.0)).unwrap(),
        Some(GeometryRect::new(0.0, 0.0, 312.0, 120.0).unwrap()),
    );
    let displacement =
        SpaceTranslation::<FrameSpace, FrameSpace, LogicalPixels>::from_px(12.0, 6.0).unwrap();

    let moved = presentation
        .translated_origin(displacement)
        .expect("valid displaced origin");

    assert_eq!((moved.origin().x(), moved.origin().y()), (20.0, 22.0));
    assert_eq!(moved.clip_rect(), presentation.clip_rect());
}
