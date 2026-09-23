use crate::geometry::{
    DeviceScale, FrameSpace, GeometryError, GeometryPoint, GeometryRect, GeometrySize,
    GeometryUnit, LayoutRect, LogicalPixels, ParentFrameSpace, RootSurfaceSpace, RowSpace,
    SpaceTranslation, WindowSpace,
};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct TestUnit(i32);

impl GeometryUnit for TestUnit {
    fn valid_coordinate(self) -> bool {
        true
    }

    fn valid_extent(self) -> bool {
        self.0 >= 0
    }
}

#[test]
fn typed_transforms_compose_row_window_and_frame_spaces() {
    let local = LayoutRect::<RowSpace>::from_px(3.25, 4.5, 20.0, 10.0);
    let row_to_window = SpaceTranslation::<RowSpace, WindowSpace>::from_px(8.0, 18.0);
    let window_to_frame = SpaceTranslation::<WindowSpace, FrameSpace>::from_px(320.0, 100.0);

    let in_frame = row_to_window.then(window_to_frame).map_rect(local);

    assert_eq!(in_frame.x().to_px(), 331.25);
    assert_eq!(in_frame.y().to_px(), 122.5);
    assert_eq!(in_frame.width().to_px(), 20.0);
    assert_eq!(in_frame.height().to_px(), 10.0);
}

#[test]
fn device_scale_is_applied_only_at_the_frame_boundary() {
    let frame_rect = LayoutRect::<FrameSpace>::from_px(10.0, 20.0, 80.0, 40.0);
    let scale = DeviceScale::new(1.75).expect("positive finite scale");

    let device_rect = scale.map_frame_rect(frame_rect);

    assert_eq!(device_rect.x(), 17.5);
    assert_eq!(device_rect.y(), 35.0);
    assert_eq!(device_rect.width(), 140.0);
    assert_eq!(device_rect.height(), 70.0);
}

#[test]
fn device_scale_rejects_invalid_values() {
    assert!(DeviceScale::new(0.0).is_err());
    assert!(DeviceScale::new(f32::NAN).is_err());
    assert!(DeviceScale::new(f32::INFINITY).is_err());
}

#[test]
fn generic_layout_rect_keeps_its_zero_default() {
    let rect = LayoutRect::<FrameSpace>::default();

    assert_eq!(rect.x().to_px(), 0.0);
    assert_eq!(rect.y().to_px(), 0.0);
    assert_eq!(rect.width().to_px(), 0.0);
    assert_eq!(rect.height().to_px(), 0.0);
}

#[test]
fn public_generic_geometry_accepts_a_new_validated_unit() {
    let origin =
        GeometryPoint::<RootSurfaceSpace, TestUnit>::try_from_units(TestUnit(-3), TestUnit(4))
            .unwrap();
    let size = GeometrySize::new(TestUnit(20), TestUnit(10)).unwrap();

    let rect: GeometryRect<RootSurfaceSpace, TestUnit> =
        GeometryRect::from_origin_and_size(origin, size);

    assert_eq!(rect.origin().x_unit(), TestUnit(-3));
    assert_eq!(rect.origin().y_unit(), TestUnit(4));
    assert_eq!(rect.size().width_unit(), TestUnit(20));
    assert_eq!(rect.size().height_unit(), TestUnit(10));
}

#[test]
fn signed_parent_geometry_translates_into_root_space_without_changing_its_size() {
    let child = GeometryRect::<ParentFrameSpace, LogicalPixels>::new(-5.0, -7.0, 40.0, 30.0)
        .expect("negative origins and nonnegative extents are valid");
    let parent_to_root =
        SpaceTranslation::<ParentFrameSpace, RootSurfaceSpace, LogicalPixels>::from_px(120.0, 48.0)
            .expect("finite translation");

    let rooted: GeometryRect<RootSurfaceSpace, LogicalPixels> = parent_to_root
        .map_rect(child)
        .expect("finite translated geometry");

    assert_eq!((rooted.x(), rooted.y()), (115.0, 41.0));
    assert_eq!((rooted.width(), rooted.height()), (40.0, 30.0));
}

#[test]
fn same_space_intersection_reports_an_unrepresentable_extent() {
    let first = GeometryRect::<RootSurfaceSpace, LogicalPixels>::new(f32::MAX, 0.0, f32::MAX, 10.0)
        .unwrap();
    let second =
        GeometryRect::<RootSurfaceSpace, LogicalPixels>::new(f32::MAX, 0.0, f32::MAX, 10.0)
            .unwrap();

    assert_eq!(
        first.try_intersection(second),
        Err(GeometryError::InvalidGeometry)
    );
}
