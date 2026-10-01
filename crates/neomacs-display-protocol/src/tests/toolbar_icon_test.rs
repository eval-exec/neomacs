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
    let source = ToolBarImageSource::File {
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
        ImageSizeSpec::new(AxisSize::AtMost(24), AxisSize::AtMost(24))
    );
}
