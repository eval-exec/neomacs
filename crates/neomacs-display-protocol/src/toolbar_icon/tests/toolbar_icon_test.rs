use crate::*;
use strum::IntoEnumIterator;

#[test]
fn every_image_background_policy_has_explicit_alpha_semantics() {
    for policy in ImageBackgroundPolicy::iter() {
        let colors =
            ImageColorContext::from_pixels(0xff0000, 0x123456).with_background_policy(policy);
        let expected = match policy {
            ImageBackgroundPolicy::FaceColor => [0x12, 0x34, 0x56, 255],
            ImageBackgroundPolicy::Transparent => [0, 0, 0, 0],
        };
        assert_eq!(colors.background_rgba8(), expected);
    }
}

#[test]
fn normal_and_compact_toolbar_publish_the_same_icon_face() {
    let fg = Color::rgb(1.0, 0.0, 0.0);
    let bg = Color::rgb(0.0, 1.0, 0.0);
    let normal = ToolBarContent::new(vec![], fg, bg, 24, 4);
    let compact = CompactBarContent::new(vec![], vec![], Color::BLACK, Color::WHITE, fg, bg, 24, 4);
    let source = ToolBarImageSource::ThemedVector {
        path: "open.svg".into(),
    };
    let key = normal
        .icon_style()
        .realize(source.clone(), DeviceScale::ONE);
    assert_eq!(key, compact.icon_style().realize(source, DeviceScale::ONE));
    assert_eq!(key.colors().foreground().rgb24(), 0xff0000);
    assert_eq!(key.colors().background().rgb24(), 0x00ff00);
    assert_eq!(
        key.colors().background_policy(),
        ImageBackgroundPolicy::Transparent
    );
    assert_eq!(
        key.size_spec(),
        ImageSizeSpec::new(AxisSize::Exact(24), AxisSize::Exact(24))
    );
}

/// A themed vector realizes at the slot's exact physical extent at every
/// device scale, so no authored intrinsic size is ever stretched into the slot.
#[test]
fn themed_vector_realizes_the_exact_slot_extent_at_fractional_scales() {
    for scale in [1.0_f32, 1.25, 1.5, 2.0] {
        let key = ToolBarIconStyle::from_chrome_face(24, Color::WHITE, Color::BLACK).realize(
            ToolBarImageSource::ThemedVector {
                path: "open.svg".into(),
            },
            DeviceScale::new(scale).expect("positive finite scale"),
        );
        let geometry = key.realization().resolve_geometry(
            key.size_spec(),
            ImageIntrinsicExtent::new(16.0, 16.0).expect("valid intrinsic extent"),
            ImageRotation::None,
        );
        let expected = (24.0_f64 * f64::from(scale)).ceil() as u32;
        assert_eq!(
            geometry.raster().dimensions(),
            (expected, expected),
            "raster extent at scale {scale}"
        );
        assert_eq!(
            geometry.layout().dimensions(),
            (24, 24),
            "logical slot stays theme-independent at scale {scale}"
        );
    }
}

/// GNU images keep `compute_image_size`'s clamp: a small native raster is not
/// enlarged, and a slot-sized raster fills the slot exactly.
#[test]
fn gnu_images_keep_the_clamp_and_are_never_upscaled() {
    let key = ToolBarIconStyle::from_chrome_face(24, Color::WHITE, Color::BLACK).realize(
        ToolBarImageSource::GnuRaster {
            path: "save.xpm".into(),
        },
        DeviceScale::new(1.7).expect("positive finite scale"),
    );
    assert_eq!(
        key.size_spec(),
        ImageSizeSpec::new(AxisSize::AtMost(24), AxisSize::AtMost(24))
    );
    let small = key.realization().resolve_geometry(
        key.size_spec(),
        ImageIntrinsicExtent::new(16.0, 16.0).expect("valid intrinsic extent"),
        ImageRotation::None,
    );
    assert_eq!(small.layout().dimensions(), (16, 16));
    assert_eq!(small.raster().dimensions(), (28, 28));
    let slot = key.realization().resolve_geometry(
        key.size_spec(),
        ImageIntrinsicExtent::new(24.0, 24.0).expect("valid intrinsic extent"),
        ImageRotation::None,
    );
    assert_eq!(slot.layout().dimensions(), (24, 24));
    assert_eq!(slot.raster().dimensions(), (41, 41));
}
