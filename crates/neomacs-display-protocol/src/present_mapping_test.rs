use std::num::NonZeroU32;

use crate::{
    DeviceScale, DeviceSurfacePoint, DrawableSurface, GeometrySize, LogicalPixels, PresentMapping,
    PresentationExtent, PresentationId, PresentedFramePoint, RootSurfaceSpace, SurfaceState,
};

fn assert_close(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() < 0.001,
        "expected {expected}, got {actual}"
    );
}

fn drawable(width: u32, height: u32, scale: f32) -> DrawableSurface {
    DrawableSurface::new(
        NonZeroU32::new(width).unwrap(),
        NonZeroU32::new(height).unwrap(),
        DeviceScale::new(scale).unwrap(),
    )
    .unwrap()
}

fn content(id: u64, width: f32, height: f32) -> PresentationExtent {
    PresentationExtent::new(
        PresentationId::new(id),
        GeometrySize::<LogicalPixels>::from_px(width, height).unwrap(),
    )
}

#[test]
fn present_mapping_clips_stale_maximize_presentation_without_stretching_it() {
    let mapping =
        PresentMapping::top_left_clip(drawable(3456, 2125, 1.75), content(5, 664.0, 682.0));

    assert_eq!(mapping.presentation(), PresentationId::new(5));
    assert_close(mapping.surface_logical_size().width(), 1974.8572);
    assert_close(mapping.surface_logical_size().height(), 1214.2858);

    let visible = mapping.visible_content_rect().unwrap();
    assert_close(visible.x(), 0.0);
    assert_close(visible.y(), 0.0);
    assert_close(visible.width(), 664.0);
    assert_close(visible.height(), 682.0);

    let device = mapping
        .device_from_frame(PresentedFramePoint::from_px(664.0, 682.0).unwrap())
        .unwrap();
    assert_close(device.x(), 1162.0);
    assert_close(device.y(), 1193.5);

    let inside = mapping
        .frame_from_device(DeviceSurfacePoint::from_px(1000.0, 100.0).unwrap())
        .unwrap();
    assert_close(inside.x(), 571.4286);
    assert_close(inside.y(), 57.1429);
    assert!(
        mapping
            .frame_from_device(DeviceSurfacePoint::from_px(2000.0, 100.0).unwrap())
            .is_none()
    );
}

#[test]
fn surface_state_makes_zero_sized_suspension_distinct_from_drawable_geometry() {
    let scale = DeviceScale::new(1.75).unwrap();
    assert_eq!(
        SurfaceState::from_device_size(0, 2125, scale).unwrap(),
        SurfaceState::Suspended
    );
    assert_eq!(
        SurfaceState::from_device_size(3456, 0, scale).unwrap(),
        SurfaceState::Suspended
    );
    assert!(matches!(
        SurfaceState::from_device_size(3456, 2125, scale).unwrap(),
        SurfaceState::Drawable(_)
    ));
}

#[test]
fn grid_rounding_clips_instead_of_changing_the_world_transform() {
    let mapping =
        PresentMapping::top_left_clip(drawable(1162, 1194, 1.75), content(9, 672.0, 700.0));

    assert_close(mapping.surface_logical_size().width(), 664.0);
    assert_close(mapping.surface_logical_size().height(), 682.2857);
    let visible = mapping.visible_content_rect().unwrap();
    assert_close(visible.width(), 664.0);
    assert_close(visible.height(), 682.2857);
}

#[test]
fn empty_presentation_has_no_visible_content() {
    let mapping = PresentMapping::top_left_clip(drawable(800, 600, 1.0), content(11, 0.0, 0.0));

    assert!(mapping.visible_content_rect().is_none());
    assert!(
        mapping
            .frame_from_device(DeviceSurfacePoint::from_px(0.0, 0.0).unwrap())
            .is_none()
    );
}

#[test]
fn mapping_types_keep_frame_device_and_root_surface_spaces_distinct() {
    let mapping = PresentMapping::top_left_clip(drawable(800, 600, 2.0), content(12, 400.0, 300.0));
    let _: crate::GeometryRect<RootSurfaceSpace, LogicalPixels> =
        mapping.visible_content_rect().unwrap();
}
