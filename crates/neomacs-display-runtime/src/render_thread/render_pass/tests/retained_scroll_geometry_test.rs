use super::*;
#[test]
fn raster_extent_preserves_device_phase_and_rejects_oversized_coverage() {
    let scale = DeviceScale::new(1.5).unwrap();
    let (size, origin) = raster_geometry(Rect::new(2.25, 3.75, 20.0, 40.0), scale, 1024).unwrap();
    assert_eq!(origin, (2.0, 10.0 / 3.0));
    assert_eq!((size.width(), size.height()), (31, 61));
    assert!(raster_geometry(Rect::new(0.0, 0.0, 10.0, 10000.0), scale, 1024).is_none());
}
