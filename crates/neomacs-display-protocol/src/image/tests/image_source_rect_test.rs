use super::{ImageMargins, ImageOpaqueBackground, ImageSourceRect};

const QUANTIZATION_TOLERANCE: f32 = 2.0 / u16::MAX as f32;

fn assert_approximately_eq(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() <= QUANTIZATION_TOLERANCE,
        "expected {expected}, got {actual}"
    );
}

#[test]
fn window_clip_coordinates_are_mapped_inside_the_image_slice() {
    let slice = ImageSourceRect::new(0.25, 0.5, 0.5, 0.25).expect("valid source rect");
    for ((actual_u, actual_v), (expected_u, expected_v)) in [
        (slice.map_uv(0.0, 0.0), (0.25, 0.5)),
        (slice.map_uv(0.5, 0.5), (0.5, 0.625)),
        (slice.map_uv(1.0, 1.0), (0.75, 0.75)),
    ] {
        assert_approximately_eq(actual_u, expected_u);
        assert_approximately_eq(actual_v, expected_v);
    }
}

#[test]
fn image_geometry_and_optional_background_remain_compact_and_unambiguous() {
    assert_eq!(std::mem::size_of::<ImageSourceRect>(), 8);
    assert_eq!(std::mem::size_of::<ImageMargins>(), 8);
    assert_eq!(std::mem::size_of::<ImageOpaqueBackground>(), 4);
    assert_eq!(ImageOpaqueBackground::new(None).get(), None);
    assert_eq!(ImageOpaqueBackground::new(Some(0)).get(), Some(0));
    assert_eq!(
        ImageOpaqueBackground::new(Some(0x12_34_56)).get(),
        Some(0x12_34_56)
    );
}
