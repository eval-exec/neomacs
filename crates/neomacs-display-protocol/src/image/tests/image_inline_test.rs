mod image_test;
use super::{AxisSize, ImageRealization, ImageRotation, ImageSizeSpec};

#[test]
fn intrinsic_extent_rejects_invalid_dimensions_without_rounding_valid_ones() {
    use super::ImageIntrinsicExtent;

    for invalid in [0.0, -1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(ImageIntrinsicExtent::new(invalid, 10.0).is_none());
        assert!(ImageIntrinsicExtent::new(10.0, invalid).is_none());
    }
    let extent = ImageIntrinsicExtent::new(52.91, 17.75).unwrap();
    assert_eq!(extent.dimensions(), (52.91, 17.75));
}

/// The invariant a reserved slot rests on: a spec that pins both axes reserves
/// the extent it will resolve to. The native size cannot enter that answer, so
/// the check runs over several natives, scales and rotations — and it is the
/// case an animated icon lives in, where a mismatch moves layout on every swap.
#[test]
fn provisional_extent_equals_resolved_geometry_when_both_axes_are_pinned() {
    use super::{ImageNativeExtent, SlotChange};

    let sizes = [
        ImageSizeSpec::new(AxisSize::Exact(24), AxisSize::Exact(24)),
        ImageSizeSpec::new(AxisSize::Exact(25), AxisSize::Exact(25)),
        ImageSizeSpec::new(AxisSize::Exact(30), AxisSize::Exact(18)),
    ];
    let scales = [1.0_f32, 0.8, 1.30 / 1.75, 1.25];
    let rotations = [
        ImageRotation::None,
        ImageRotation::Quarter,
        ImageRotation::Half,
        ImageRotation::ThreeQuarter,
    ];
    for size in sizes {
        for scale in scales {
            let realization = ImageRealization::new(scale, 1.0, 1.0);
            for rotation in rotations {
                for native in [(96, 96), (100, 200), (3, 7)] {
                    let provisional = realization.resolve_provisional(size, rotation);
                    let resolved = realization
                        .resolve_geometry(size, ImageNativeExtent::new(native.0, native.1), rotation)
                        .layout();
                    assert_eq!(
                        provisional.finalize(resolved),
                        SlotChange::Unchanged,
                        "{size:?} at scale {scale} {rotation:?} over {native:?} must reserve what it resolves to"
                    );
                }
            }
        }
    }
}

/// The other half of the invariant: when an axis is left to the native size,
/// the reservation legitimately moves once the header supplies the aspect
/// ratio — and that is the only case where a move is allowed.
#[test]
fn provisional_extent_moves_only_where_the_native_aspect_is_unknown() {
    use super::ImageNativeExtent;

    let realization = ImageRealization::default();
    let size = ImageSizeSpec::new(AxisSize::Native, AxisSize::Exact(24));
    let provisional = realization
        .resolve_provisional(size, ImageRotation::None)
        .layout();
    let resolved = realization
        .resolve_geometry(size, ImageNativeExtent::new(100, 200), ImageRotation::None)
        .layout();
    assert_eq!(
        provisional.dimensions(),
        (24, 24),
        "the reservation is a square of the pinned height"
    );
    assert_eq!(
        resolved.dimensions(),
        (12, 24),
        "100x200 pinned to a height of 24 is 12x24"
    );
}

#[test]
fn fractional_intrinsic_extent_keeps_the_ratio_through_geometry_resolution() {
    use super::ImageIntrinsicExtent;

    // GNU 31.1 image-size-oracle.el: scaling native dimensions gives
    // 68x23, while :height 17 uses the fractional ratio to give 51x17.
    let intrinsic = ImageIntrinsicExtent::new(52.910000000000004, 17.75).unwrap();
    let geometry = ImageRealization::with_device_scale(1.3, 2.0).resolve_geometry(
        ImageSizeSpec::default(),
        intrinsic,
        ImageRotation::Quarter,
    );
    assert_eq!(geometry.layout().dimensions(), (23, 68));
    assert_eq!(geometry.reported().dimensions(), (23, 68));
    assert_eq!(geometry.raster().dimensions(), (46, 136));
    assert_eq!(
        ImageSizeSpec::new(AxisSize::Native, AxisSize::Exact(17)).desired_intrinsic(intrinsic, 1.0),
        (51, 17)
    );
}

/// Every expectation below was measured from GNU Emacs 31 on a 40x20 PNG
/// with `image-scaling-factor` pinned to 1, so the numbers are observed
/// rather than derived from reading `compute_image_size`.
const NATIVE: (u32, u32) = (40, 20);

fn desired(spec: ImageSizeSpec, scale: f64) -> (u32, u32) {
    spec.desired(NATIVE.0, NATIVE.1, scale)
}

/// Measured from GNU Emacs 31 on the same 40x20 PNG.
#[test]
fn rotation_reduces_modulo_360_and_only_turns_on_multiples_of_90() {
    use ImageRotation as R;
    for (degrees, expected) in [
        (0.0, R::None),
        (90.0, R::Quarter),
        (180.0, R::Half),
        (270.0, R::ThreeQuarter),
        (360.0, R::None),
        (450.0, R::Quarter),
        // Emacs `mod` takes the divisor's sign: -90 reduces to 270.
        (-90.0, R::ThreeQuarter),
        // Not a multiple of 90: GNU leaves the image upright.
        (45.0, R::None),
        (f64::NAN, R::None),
    ] {
        assert_eq!(R::from_degrees(degrees), expected, "rotation {degrees}");
    }
}

#[test]
fn quarter_turns_exchange_the_axes() {
    // GNU: 40x20 with `:rotation 90` reports (20 . 40).
    assert_eq!(ImageRotation::Quarter.orient(40, 20), (20, 40));
    assert_eq!(ImageRotation::ThreeQuarter.orient(40, 20), (20, 40));
    assert_eq!(ImageRotation::Half.orient(40, 20), (40, 20));
    assert_eq!(ImageRotation::None.orient(40, 20), (40, 20));
}

#[test]
fn sizing_happens_before_rotation() {
    // GNU: `:rotation 90 :width 80` on 40x20 reports (40 . 80) — `:width`
    // sizes the upright image (80x40), then the turn swaps the axes.
    let sized = ImageSizeSpec::new(AxisSize::Exact(80), AxisSize::Native).desired(40, 20, 1.0);
    assert_eq!(sized, (80, 40));
    assert_eq!(ImageRotation::Quarter.orient(sized.0, sized.1), (40, 80));
}

#[test]
fn native_size_survives_when_nothing_is_requested() {
    assert_eq!(desired(ImageSizeSpec::default(), 1.0), (40, 20));
}

#[test]
fn scale_multiplies_both_axes() {
    assert_eq!(desired(ImageSizeSpec::default(), 2.0), (80, 40));
}

#[test]
fn width_is_a_target_and_keeps_the_aspect_ratio() {
    // GNU: `:width 80` => (80 . 40) — the height follows from the ratio.
    assert_eq!(
        desired(
            ImageSizeSpec::new(AxisSize::Exact(80), AxisSize::Native),
            1.0
        ),
        (80, 40)
    );
}

#[test]
fn height_is_a_target_and_keeps_the_aspect_ratio() {
    // GNU: `:height 40` => (80 . 40).
    assert_eq!(
        desired(
            ImageSizeSpec::new(AxisSize::Native, AxisSize::Exact(40)),
            1.0
        ),
        (80, 40)
    );
}

#[test]
fn max_width_clamps_and_keeps_the_aspect_ratio() {
    // GNU: `:max-width 20` => (20 . 10), NOT (20 . <unbounded>).
    assert_eq!(
        desired(
            ImageSizeSpec::new(AxisSize::AtMost(20), AxisSize::Native),
            1.0
        ),
        (20, 10)
    );
}

#[test]
fn width_overrides_max_width() {
    // GNU: `:width 80 :max-width 20` => (80 . 40). ":width overrides
    // :max-width" (src/image.c:2767) — conflating the two keys is what
    // made neomacs answer (20 . 4096).
    assert_eq!(
        desired(
            // GNU precedence resolved at construction: the target wins.
            ImageSizeSpec::new(AxisSize::resolve(Some(80), Some(20)), AxisSize::Native),
            1.0
        ),
        (80, 40)
    );
}

#[test]
fn explicit_width_and_height_skip_the_aspect_computation() {
    assert_eq!(
        desired(
            ImageSizeSpec::new(AxisSize::Exact(11), AxisSize::Exact(99)),
            1.0
        ),
        (11, 99)
    );
}

#[test]
fn targets_are_themselves_scaled() {
    // GNU multiplies :width/:height by the scale (src/image.c:2766).
    assert_eq!(
        desired(
            ImageSizeSpec::new(AxisSize::Exact(40), AxisSize::Native),
            2.0
        ),
        (80, 40)
    );
}

#[test]
fn fractional_realization_has_one_layout_and_raster_rounding_policy() {
    let realization = ImageRealization::new(1.3 / 1.75, 1.75, 1.75);

    assert_eq!(realization.layout_dimension(24), 18);
    assert_eq!(realization.raster_dimension(18), 32);
    // report recovers GNU image pixels from logical layout.
    assert_eq!(realization.image_pixel_dimension(18), 32);
}

#[test]
fn report_scale_one_leaves_layout_as_image_pixels() {
    let realization = ImageRealization::with_device_scale(1.0, 1.25);
    assert_eq!(realization.image_pixel_dimension(333), 333);
    assert_eq!(realization.raster_dimension(333), 417); // ceil(333*1.25)
}

#[test]
fn image_pixel_scale_snaps_default_hidpi_product_to_one() {
    // 1/1.25 × 1.25 is not bit-exact in f32; snap so native 333 stays 333.
    let realization = ImageRealization::new(1.0 / 1.25, 1.25, 1.25);
    assert_eq!(realization.image_pixel_scale(), 1.0);
    assert!((realization.layout_scale() - 0.8).abs() < 1e-6);
}
