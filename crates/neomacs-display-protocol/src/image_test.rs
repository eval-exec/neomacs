use super::{
    ImageCacheUsage, ImageColorContext, ImageId, ImageLoadAttempt, ImageLoadToken,
    ImageNativeExtent, ImageRealization, ImageRgb, ImageRotation, ImageSequenceId, ImageSizeSpec,
    ImageStateEvent,
};

#[test]
fn image_realization_resolves_each_geometry_space_without_crossing_units() {
    let native = ImageNativeExtent::new(40, 20);
    let realization = ImageRealization::new(0.75, 2.0, 4.0 / 3.0);

    let geometry =
        realization.resolve_geometry(ImageSizeSpec::default(), native, ImageRotation::Quarter);

    assert_eq!(geometry.layout().dimensions(), (15, 30));
    assert_eq!(geometry.reported().dimensions(), (20, 40));
    assert_eq!(geometry.raster().dimensions(), (30, 60));
}

#[test]
fn decode_completion_carries_the_exact_load_attempt() {
    let load = ImageLoadToken::new(
        ImageId::new(17),
        ImageLoadAttempt::new(3).expect("non-zero attempt"),
    );

    let event = ImageStateEvent::DecodeCompleted(load);

    assert_eq!(event.image(), ImageId::new(17));
    assert_eq!(event, ImageStateEvent::DecodeCompleted(load));
}

#[test]
fn image_sequence_identity_excludes_the_retirement_sentinel() {
    assert!(ImageSequenceId::new(0).is_none());
    assert_eq!(ImageSequenceId::new(7).map(ImageSequenceId::get), Some(7));
}

#[test]
fn image_cache_usage_keeps_texture_and_sequence_domains_distinct() {
    let usage = ImageCacheUsage::new(4_096, 768);

    assert_eq!(usage.texture_bytes(), 4_096);
    assert_eq!(usage.decoded_sequence_bytes(), 768);
    assert_eq!(usage.total_bytes(), 4_864);
}

#[test]
fn image_rgb_preserves_black_as_a_real_opaque_color() {
    let black = ImageRgb::from_pixel(0x0000_0000);

    assert_eq!(black.rgb24(), 0x0000_0000);
    assert_eq!(black.rgba8(), [0, 0, 0, 0xff]);
}

#[test]
fn image_color_context_keeps_foreground_and_background_roles_distinct() {
    let colors = ImageColorContext::from_pixels(0xaa_12_34_56, 0xbb_65_43_21);

    assert_eq!(colors.foreground().rgb24(), 0x12_34_56);
    assert_eq!(colors.background().rgb24(), 0x65_43_21);
}

#[test]
fn unresolved_image_color_context_preserves_the_visible_monochrome_fallback() {
    let colors = ImageColorContext::default();

    assert_eq!(colors.foreground().rgb24(), 0x00ff_ffff);
    assert_eq!(colors.background().rgb24(), 0x0000_0000);
}
