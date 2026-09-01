use super::color::VideoColorTransform;
use super::{BiPlanarVideoFormat, VideoColorimetry};

fn assert_rgb(actual: [f32; 3], expected: [f32; 3]) {
    for (actual, expected) in actual.into_iter().zip(expected) {
        assert!((actual - expected).abs() < 0.002, "{actual} != {expected}");
    }
}

#[test]
fn limited_bt709_maps_nominal_black_and_white_before_transfer_decoding() {
    let transform = VideoColorTransform::new(
        BiPlanarVideoFormat::Nv12,
        VideoColorimetry::BT709_LIMITED,
        super::VideoGeometry::packed(1920, 1080),
    );

    let neutral = 128.0 / 255.0;
    assert_rgb(
        transform.encoded_rgb([16.0 / 255.0, neutral, neutral]),
        [0.0; 3],
    );
    assert_rgb(
        transform.encoded_rgb([235.0 / 255.0, neutral, neutral]),
        [1.0; 3],
    );
}

#[test]
fn p010_word_normalization_preserves_ten_bit_nominal_black() {
    let transform = VideoColorTransform::new(
        BiPlanarVideoFormat::P010,
        VideoColorimetry::BT709_LIMITED,
        super::VideoGeometry::packed(3840, 2160),
    );
    let stored_black = (64_u16 << 6) as f32 / u16::MAX as f32;
    let stored_neutral_chroma = (512_u16 << 6) as f32 / u16::MAX as f32;

    assert_rgb(
        transform.encoded_rgb([stored_black, stored_neutral_chroma, stored_neutral_chroma]),
        [0.0; 3],
    );
}
