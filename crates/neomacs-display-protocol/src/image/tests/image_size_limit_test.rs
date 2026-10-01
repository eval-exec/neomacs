//! GNU `check_image_size` (`src/image.c:1811-1836`), arm by arm.

use super::{ImageIntrinsicExtent, ImageNativeExtent, ImageSizeLimit, OversizedImage};

fn native(width: u32, height: u32) -> ImageNativeExtent {
    ImageNativeExtent::new(width, height)
}

/// `FIXNUMP`: one absolute pixel bound, and it is inclusive on both axes.
#[test]
fn integer_limit_admits_its_own_value_and_nothing_above_it() {
    let limit = ImageSizeLimit::from_axis_pixels(100);

    assert!(
        limit.permits(native(100, 100)),
        "the limit itself is allowed"
    );
    assert!(!limit.permits(native(101, 100)), "one pixel over is not");
    assert!(!limit.permits(native(100, 101)), "on either axis");
    assert!(limit.permits(native(1, 1)));
}

/// Width-over and height-over are independent refusals, and neither can be
/// averaged away by a small other axis.
#[test]
fn a_single_oversized_axis_refuses_the_image() {
    let limit = ImageSizeLimit::from_axis_pixels(100);

    assert!(
        !limit.permits(native(400, 1)),
        "wide and flat is still wide"
    );
    assert!(
        !limit.permits(native(1, 400)),
        "tall and thin is still tall"
    );
}

/// `XFIXNUM` is a signed word: GNU refuses every image when the value is
/// non-positive, because no real extent is `<= 0`.
#[test]
fn non_positive_integer_limit_refuses_every_image() {
    for pixels in [0, -1, i64::MIN] {
        let limit = ImageSizeLimit::from_axis_pixels(pixels);
        assert_eq!(limit.maximum().dimensions(), (0, 0), "pixels = {pixels}");
        assert!(!limit.permits(native(1, 1)), "pixels = {pixels}");
    }
}

/// `FLOATP`: each axis is bounded by *its own* frame dimension times the
/// ratio. A square-looking image can therefore fail on width alone.
#[test]
fn fractional_limit_bounds_each_axis_by_its_own_frame_dimension() {
    let frame = native(400, 1000);
    let limit = ImageSizeLimit::from_frame_ratio(0.5, Some(frame));

    assert_eq!(limit.maximum().dimensions(), (200, 500));
    assert!(limit.permits(native(200, 500)), "exactly at both bounds");
    assert!(
        !limit.permits(native(201, 500)),
        "width bound is the frame's"
    );
    assert!(
        !limit.permits(native(200, 501)),
        "height bound is the frame's"
    );
    assert!(
        !limit.permits(native(300, 300)),
        "within the height bound, over the width bound"
    );
}

/// GNU's null-frame fallback is a fixed 1024x1024 (`src/image.c:1830`).
#[test]
fn fractional_limit_without_a_frame_uses_gnus_arbitrary_size() {
    let limit = ImageSizeLimit::from_frame_ratio(1.0, None);

    assert_eq!(limit.maximum().dimensions(), (1024, 1024));
    assert!(limit.permits(native(1024, 1024)));
    assert!(!limit.permits(native(1025, 1024)));
}

/// `width <= X` for an integer width is `width <= floor (X)`, so a fractional
/// bound keeps the whole pixel it covers.
#[test]
fn a_fractional_bound_is_floored_to_the_pixels_it_covers() {
    let limit = ImageSizeLimit::from_frame_ratio(0.5, Some(native(3, 3)));

    assert_eq!(limit.maximum().dimensions(), (1, 1));
    assert!(limit.permits(native(1, 1)));
    assert!(!limit.permits(native(2, 2)));
}

/// A ratio that is not a number, or not positive, admits nothing — the same
/// verdict the comparison `width <= NaN` reaches.
#[test]
fn unusable_ratios_admit_nothing() {
    for ratio in [f64::NAN, -1.0, 0.0, f64::NEG_INFINITY] {
        let limit = ImageSizeLimit::from_frame_ratio(ratio, Some(native(800, 600)));
        assert_eq!(limit.maximum().dimensions(), (0, 0), "ratio = {ratio}");
        assert!(!limit.permits(native(1, 1)), "ratio = {ratio}");
    }
}

/// An infinite bound is no bound: every representable extent passes.
#[test]
fn an_infinite_ratio_is_no_limit() {
    let limit = ImageSizeLimit::from_frame_ratio(f64::INFINITY, Some(native(800, 600)));

    assert_eq!(limit, ImageSizeLimit::UNLIMITED);
    assert!(limit.permits(native(u32::MAX, u32::MAX)));
}

/// The non-numeric `max-image-size` arm: GNU's `else return 1`.
#[test]
fn no_limit_admits_every_representable_extent() {
    let limit = ImageSizeLimit::UNLIMITED;

    assert!(limit.permits(native(1, 1)));
    assert!(limit.permits(native(u32::MAX, u32::MAX)));
}

/// GNU's own initializer, `make_float (MAX_IMAGE_SIZE)` with
/// `MAX_IMAGE_SIZE 10.0` (`src/image.c:1740`, `:13047`).
#[test]
fn the_default_is_gnus_ten_frame_ratio() {
    let limit = ImageSizeLimit::default();

    assert_eq!(limit.maximum().dimensions(), (10240, 10240));
    assert!(limit.permits(native(10240, 4000)));
    assert!(!limit.permits(native(10241, 1)));
}

/// A vector document's extent is fractional. GNU compares the rasterized
/// integer size, so a fractional extent is compared with its ceiling: never
/// admitted by a rounding that shrinks it.
#[test]
fn a_fractional_source_extent_is_compared_with_its_ceiling() {
    let limit = ImageSizeLimit::from_axis_pixels(100);
    let intrinsic = |width: f64, height: f64| {
        ImageIntrinsicExtent::new(width, height).expect("positive finite test extent")
    };

    assert!(limit.permits_intrinsic(intrinsic(100.0, 99.5)));
    assert!(
        !limit.permits_intrinsic(intrinsic(100.5, 10.0)),
        "100.5 rasterizes to 101 pixels"
    );
    assert!(!limit.permits_intrinsic(intrinsic(10.0, 100.5)));
}

/// The diagnostic is GNU's, character for character.
#[test]
fn the_refusal_carries_gnus_own_message() {
    let limit = ImageSizeLimit::from_axis_pixels(10);
    let refused = OversizedImage::new(native(11, 12), limit);

    assert_eq!(
        OversizedImage::MESSAGE,
        "Invalid image size (see `max-image-size')"
    );
    assert_eq!(refused.extent().dimensions(), (11, 12));
    assert_eq!(refused.limit(), limit);
}
