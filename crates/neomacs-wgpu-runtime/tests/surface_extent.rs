use neomacs_wgpu_runtime::SurfaceExtent;

#[test]
fn zero_width_suspends_surface_configuration() {
    assert_eq!(SurfaceExtent::from_physical_size(0, 720), SurfaceExtent::Suspended);
}

#[test]
fn zero_height_suspends_surface_configuration() {
    assert_eq!(SurfaceExtent::from_physical_size(1280, 0), SurfaceExtent::Suspended);
}

#[test]
fn drawable_extent_preserves_nonzero_dimensions() {
    let extent = SurfaceExtent::from_physical_size(1280, 720);

    assert_eq!(extent.width(), Some(1280));
    assert_eq!(extent.height(), Some(720));
}
