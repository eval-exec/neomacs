use super::{ImageDefaultScale, ImageScaleEnvironment, ImageScalePolicy};

#[test]
fn auto_scale_realizes_gnu_x11_sized_pixels_on_fractional_wayland() {
    let environment = ImageScaleEnvironment::new(7.2, 1.75, ImageDefaultScale::Auto);

    let realization = environment.resolve(ImageScalePolicy::Default);

    // GNU's auto policy sees a 13-device-pixel frame column and therefore
    // realizes 24px at 1.3x.  Neomacs lays that out in logical pixels and
    // rasterizes it in device pixels.
    assert_eq!(realization.layout_dimension(24), 18);
    assert_eq!(realization.raster_dimension(18), 32);
}

#[test]
fn auto_scale_reconstructs_gnu_device_column_from_integer_logical_geometry() {
    // Neomacs exposes an integer logical `frame-char-width`, while GNU's
    // FRAME_COLUMN_WIDTH is the corresponding integer device-pixel font
    // metric.  At 1.75 scale a 7px logical cell therefore occupies the
    // 13px device column used by GNU's automatic image scale, not 12px.
    let environment = ImageScaleEnvironment::new(7.0, 1.75, ImageDefaultScale::Auto);

    let realization = environment.resolve(ImageScalePolicy::Default);

    assert_eq!(realization.layout_dimension(24), 18);
    assert_eq!(realization.raster_dimension(18), 32);
}

#[test]
fn auto_scale_is_identity_at_one_x_when_the_column_is_under_ten_pixels() {
    let environment = ImageScaleEnvironment::new(7.2, 1.0, ImageDefaultScale::Auto);

    let realization = environment.resolve(ImageScalePolicy::Default);

    assert_eq!(realization.layout_dimension(24), 24);
    assert_eq!(realization.raster_dimension(24), 24);
}

#[test]
fn explicit_image_scale_does_not_consult_the_default_policy() {
    let environment = ImageScaleEnvironment::new(
        7.2,
        1.75,
        ImageDefaultScale::Explicit(2.0.try_into().expect("valid scale")),
    );

    let realization = environment.resolve(ImageScalePolicy::Explicit(
        0.5.try_into().expect("valid scale"),
    ));

    assert_eq!(realization.layout_dimension(24), 12);
    assert_eq!(realization.raster_dimension(12), 21);
}
