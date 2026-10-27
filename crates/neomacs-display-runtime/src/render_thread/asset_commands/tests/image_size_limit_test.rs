//! GNU `max-image-size` at the load boundary: the header decides, before any
//! CPU decoder allocates pixels (`check_image_size`, `src/image.c:1811`).

use neomacs_display_protocol::{
    ImageAnimationPolicy, ImageColorContext, ImageFrameIndex, ImageId, ImageMaskPolicy,
    ImageNativeExtent, ImageRealization, ImageRotation, ImageSequenceId, ImageSizeLimit,
    ImageSizeSpec, OversizedImage,
    image::EncodedBytes,
    image_diagnostic::{ImageDiagnostic, ImageLoadIdentity},
};
use neomacs_renderer_wgpu::{ImageDecoder, ImageProbeSource, SvgResourceContext};
use neovm_core::emacs_core::image_catalog::{ImageLoadAttempt, ImageLoadToken};
use std::time::Duration;

fn fixture(name: &str) -> String {
    neomacs_infra::workspace_root()
        .join("test/data/image")
        .join(name)
        .to_str()
        .expect("utf8 fixture path")
        .to_owned()
}

fn decode(source: ImageProbeSource<'_>, limit: ImageSizeLimit) -> Result<(), ImageDiagnostic> {
    // Retain the CPU decoder owner until terminal metadata is observed.
    let decoder = ImageDecoder::new();
    let client = decoder.client();
    let load = ImageLoadToken::new(
        ImageId::new(1),
        ImageLoadAttempt::new(1).expect("non-zero attempt"),
    );
    let sequence = ImageSequenceId::new(1).expect("non-zero sequence");
    match source {
        ImageProbeSource::File(path) => client.load_file_with_id(
            load,
            path,
            ImageSizeSpec::default(),
            ImageRotation::None,
            ImageRealization::default(),
            ImageColorContext::default(),
            ImageMaskPolicy::Preserve,
            ImageAnimationPolicy::disabled(),
            ImageFrameIndex::default(),
            sequence,
            ImageLoadIdentity::unspecified(),
            limit,
        ),
        ImageProbeSource::Data(data) => client.load_data_with_id(
            load,
            EncodedBytes::copy_of(data),
            ImageSizeSpec::default(),
            ImageRotation::None,
            ImageRealization::default(),
            ImageColorContext::default(),
            ImageMaskPolicy::Preserve,
            ImageAnimationPolicy::disabled(),
            ImageFrameIndex::default(),
            sequence,
            SvgResourceContext::Isolated,
            ImageLoadIdentity::unspecified(),
            limit,
        ),
    }
    client
        .wait_for_terminal(load, Duration::from_secs(5))
        .expect("CPU decoder must publish a terminal result")
        .map(|_| ())
}

fn refused(result: Result<(), ImageDiagnostic>) -> String {
    let diagnostic = result.expect_err("expected the CPU load to be refused");
    assert_eq!(diagnostic, ImageDiagnostic::InvalidSize);
    diagnostic.message()
}

/// The shared CPU decoder refuses an oversized image with GNU's diagnostic.
#[test]
fn an_image_over_the_limit_is_refused_with_gnus_own_diagnostic() {
    // 200x100 under a 100-pixel bound: over on the width axis alone.
    let result = decode(
        ImageProbeSource::File(&fixture("blank-200x100.png")),
        ImageSizeLimit::from_axis_pixels(100),
    );

    let error = result.expect_err("expected an oversized CPU load to be refused");
    assert_eq!(
        error,
        ImageDiagnostic::InvalidSize,
        "the refusal is GNU's own size diagnostic, not a bare message"
    );
    assert_eq!(error.message(), OversizedImage::MESSAGE);
    assert_eq!(
        OversizedImage::MESSAGE,
        "Invalid image size (see `max-image-size')"
    );
}

/// The bound is inclusive on each axis, and each axis refuses on its own.
#[test]
fn the_limit_is_inclusive_on_both_axes() {
    let wide = fixture("blank-200x100.png");
    let tall = fixture("blank-100x200.png");
    let decoded = |path: &str, pixels: i64| {
        decode(
            ImageProbeSource::File(path),
            ImageSizeLimit::from_axis_pixels(pixels),
        )
    };

    // 200x100: exactly at the bound on width, comfortably inside on height.
    assert!(decoded(&wide, 200).is_ok(), "200 columns at a 200 bound");
    assert_eq!(
        refused(decoded(&wide, 199)),
        OversizedImage::MESSAGE,
        "one column over the bound is over the bound"
    );

    // 100x200: over on height only, which is a refusal all the same.
    assert!(decoded(&tall, 200).is_ok(), "200 rows at a 200 bound");
    assert_eq!(refused(decoded(&tall, 199)), OversizedImage::MESSAGE);
}

/// Missing and malformed sources fail decoding, without a size diagnostic.
#[test]
fn unmeasurable_sources_fail_without_the_size_refusal() {
    let limit = ImageSizeLimit::from_axis_pixels(1);
    for source in [
        ImageProbeSource::File("/nonexistent/neomacs.png"),
        ImageProbeSource::Data(b"not an image at all"),
    ] {
        let error = decode(source, limit).expect_err("invalid source must fail decoding");
        assert_ne!(error, ImageDiagnostic::InvalidSize);
    }
}

/// `:data` loads carry the same encoded header and are refused identically.
#[test]
fn an_oversized_data_source_is_refused() {
    let data = std::fs::read(fixture("blank-200x100.png")).expect("read image fixture");

    assert_eq!(
        refused(decode(
            ImageProbeSource::Data(&data),
            ImageSizeLimit::from_axis_pixels(100),
        )),
        OversizedImage::MESSAGE
    );
}

/// A frame ratio bounds each axis by its own frame dimension, so an image can
/// be refused for being wide even when it is well inside the height bound.
#[test]
fn a_frame_ratio_refuses_the_axis_that_exceeds_its_own_frame_dimension() {
    let wide = fixture("blank-200x100.png");
    let decoded = |ratio: f64| {
        decode(
            ImageProbeSource::File(&wide),
            ImageSizeLimit::from_frame_ratio(ratio, Some(ImageNativeExtent::new(300, 1000))),
        )
    };

    // A 300x1000 frame at ratio 0.5 bounds width at 150 and height at 500:
    // 200x100 is over the width bound and nowhere near the height bound.
    assert_eq!(refused(decoded(0.5)), OversizedImage::MESSAGE);
    // The same image is well inside a frame twice as wide.
    assert!(decoded(1.0).is_ok());
}

/// A non-numeric `max-image-size` is no limit: every source loads.
#[test]
fn no_limit_admits_every_fixture() {
    for name in ["blank-100x200.png", "blank-200x100.png", "black.gif"] {
        assert!(
            decode(
                ImageProbeSource::File(&fixture(name)),
                ImageSizeLimit::UNLIMITED
            )
            .is_ok(),
            "{name} must load under no limit"
        );
    }
}
