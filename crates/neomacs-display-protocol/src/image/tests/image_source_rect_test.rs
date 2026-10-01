use super::{ImageLayoutAdvance, ImageMargins, ImageOpaqueBackground, ImageSourceRect};
use crate::types::Px;

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

/// GNU's inline-image crop removes the same pixels from the glyph's layout
/// advance and from its source slice (`it->pixel_width -= crop;
/// slice.width -= crop;`, `produce_image_glyph`, src/xdisp.c:32506-32507), so
/// the glyph draws the left part of the image rather than squeezing all of it
/// into the narrower box.
#[test]
fn cropping_the_right_edge_keeps_the_left_fraction_of_the_slice() {
    let cropped = ImageSourceRect::FULL
        .crop_right_to_fraction(0.8)
        .expect("0.8 of the full image is a non-empty rect");
    assert_approximately_eq(cropped.x(), 0.0);
    assert_approximately_eq(cropped.y(), 0.0);
    assert_approximately_eq(cropped.width(), 0.8);
    assert_approximately_eq(cropped.height(), 1.0);
    // The crop composes with an explicit (slice …) rect: it takes from the
    // right edge of what the spec already selected.
    let sliced = ImageSourceRect::new(0.25, 0.0, 0.5, 1.0).expect("valid source rect");
    let cropped_slice = sliced
        .crop_right_to_fraction(0.5)
        .expect("half of the slice remains");
    assert_approximately_eq(cropped_slice.x(), 0.25);
    assert_approximately_eq(cropped_slice.width(), 0.25);
}

#[test]
fn a_crop_that_leaves_nothing_to_sample_is_rejected() {
    assert_eq!(ImageSourceRect::FULL.crop_right_to_fraction(0.0), None);
    assert_eq!(ImageSourceRect::FULL.crop_right_to_fraction(-1.0), None);
    assert_eq!(ImageSourceRect::FULL.crop_right_to_fraction(f32::NAN), None);
    // A sliver below the u16 encoding's resolution is an empty rect, not a
    // one-pixel-wide one.
    assert_eq!(
        ImageSourceRect::FULL.crop_right_to_fraction(1.0 / 1_000_000.0),
        None
    );
    assert_eq!(
        ImageSourceRect::FULL.crop_right_to_fraction(1.0),
        Some(ImageSourceRect::FULL)
    );
    assert_eq!(
        ImageSourceRect::FULL.crop_right_to_fraction(2.0),
        Some(ImageSourceRect::FULL),
        "a fraction past the whole rect crops nothing"
    );
}

#[test]
fn a_cropped_layout_advance_must_be_positive_and_finite() {
    assert_eq!(
        ImageLayoutAdvance::new(Px(304.0)).map(|advance| advance.px().get()),
        Some(304.0)
    );
    assert_eq!(ImageLayoutAdvance::new(Px(0.0)), None);
    assert_eq!(ImageLayoutAdvance::new(Px(-1.0)), None);
    assert_eq!(ImageLayoutAdvance::new(Px(f32::INFINITY)), None);
    assert_eq!(std::mem::size_of::<ImageLayoutAdvance>(), 4);
}
