//! GNU `max-image-size` at the load boundary: the header decides, before any
//! decoder is asked for a pixel (`check_image_size`, `src/image.c:1811`).

use super::*;
use neomacs_display_protocol::{ImageNativeExtent, ImageSizeLimit, OversizedImage};
use neovm_core::emacs_core::image_catalog::{ImageLoadAttempt, ImageLoadToken};

fn fixture(name: &str) -> String {
    neomacs_infra::workspace_root()
        .join("test/data/image")
        .join(name)
        .to_str()
        .expect("utf8 fixture path")
        .to_owned()
}

fn load(image: u32) -> ImageLoadToken {
    ImageLoadToken::new(
        ImageId::new(image),
        ImageLoadAttempt::new(1).expect("non-zero attempt"),
    )
}

fn refused(event: Option<neomacs_renderer_wgpu::ImageCacheEvent>) -> String {
    match event {
        Some(neomacs_renderer_wgpu::ImageCacheEvent::Failed { error, .. }) => error.message(),
        other => panic!("expected a refused load, got {other:?}"),
    }
}

/// An image GNU would refuse is answered here, without the renderer being
/// asked for it at all, and with GNU's own diagnostic.
#[test]
fn an_image_over_the_limit_is_refused_with_gnus_own_diagnostic() {
    // 200x100 under a 100-pixel bound: over on the width axis alone.
    let event = oversized_load_event(
        load(1),
        neomacs_renderer_wgpu::ImageProbeSource::File(&fixture("blank-200x100.png")),
        ImageSizeLimit::from_axis_pixels(100),
    );

    let Some(neomacs_renderer_wgpu::ImageCacheEvent::Failed { error, .. }) = event else {
        panic!("expected a refused load, got {event:?}");
    };
    assert_eq!(
        error,
        neomacs_display_protocol::image_diagnostic::ImageDiagnostic::InvalidSize,
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
    let event = |path: &str, pixels: i64| {
        oversized_load_event(
            load(1),
            neomacs_renderer_wgpu::ImageProbeSource::File(path),
            ImageSizeLimit::from_axis_pixels(pixels),
        )
    };

    // 200x100: exactly at the bound on width, comfortably inside on height.
    assert!(event(&wide, 200).is_none(), "200 columns at a 200 bound");
    assert_eq!(
        refused(event(&wide, 199)),
        OversizedImage::MESSAGE,
        "one column over the bound is over the bound"
    );

    // 100x200: over on height only, which is a refusal all the same.
    assert!(event(&tall, 200).is_none(), "200 rows at a 200 bound");
    assert_eq!(refused(event(&tall, 199)), OversizedImage::MESSAGE);
}

/// A source this crate cannot measure is admitted: the only honest answer for
/// an unknown extent is "not known to be over the limit". GNU checks such
/// sources only after it has parsed them.
#[test]
fn an_unmeasurable_source_is_not_refused() {
    let limit = ImageSizeLimit::from_axis_pixels(1);

    assert!(
        oversized_load_event(
            load(1),
            neomacs_renderer_wgpu::ImageProbeSource::File("/nonexistent/neomacs.png"),
            limit
        )
        .is_none(),
        "a missing file has no header to refuse"
    );
    assert!(
        oversized_load_event(
            load(2),
            neomacs_renderer_wgpu::ImageProbeSource::Data(b"not an image at all"),
            limit
        )
        .is_none(),
        "bytes with no header are the decoder's problem, not the limit's"
    );
}

/// `:data` loads carry the same encoded header and are refused identically.
#[test]
fn an_oversized_data_source_is_refused() {
    let data = std::fs::read(fixture("blank-200x100.png")).expect("read image fixture");

    assert_eq!(
        refused(oversized_load_event(
            load(1),
            neomacs_renderer_wgpu::ImageProbeSource::Data(&data),
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
    let event = |ratio: f64| {
        oversized_load_event(
            load(1),
            neomacs_renderer_wgpu::ImageProbeSource::File(&wide),
            ImageSizeLimit::from_frame_ratio(ratio, Some(ImageNativeExtent::new(300, 1000))),
        )
    };

    // A 300x1000 frame at ratio 0.5 bounds width at 150 and height at 500:
    // 200x100 is over the width bound and nowhere near the height bound.
    assert_eq!(refused(event(0.5)), OversizedImage::MESSAGE);
    // The same image is well inside a frame twice as wide.
    assert!(event(1.0).is_none());
}

/// A non-numeric `max-image-size` is no limit: every source loads.
#[test]
fn no_limit_admits_every_fixture() {
    for name in ["blank-100x200.png", "blank-200x100.png", "black.gif"] {
        assert!(
            oversized_load_event(
                load(1),
                neomacs_renderer_wgpu::ImageProbeSource::File(&fixture(name)),
                ImageSizeLimit::UNLIMITED
            )
            .is_none(),
            "{name} must load under no limit"
        );
    }
}
