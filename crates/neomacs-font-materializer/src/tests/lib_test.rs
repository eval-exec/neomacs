use super::*;

#[test]
fn gnu_default_line_height_discards_driver_internal_leading() {
    assert_eq!(
        bitmap_line_height(9.0, 3.0, 16.0, BitmapLineHeightPolicy::GnuDefault),
        12.0
    );
    assert_eq!(
        bitmap_line_height(9.0, 3.0, 16.0, BitmapLineHeightPolicy::NativeMetrics),
        16.0
    );
}

#[test]
fn gnu_fixed_mono_metrics_use_the_maximum_advance() {
    assert_eq!(
        fixed_font_horizontal_metrics(FixedFontSpacing::MonospaceOrCharacterCell, 8.0, 4.0, 7.5,),
        (8.0, 8.0)
    );
    assert_eq!(
        fixed_font_horizontal_metrics(FixedFontSpacing::ProportionalOrDual, 8.0, 4.0, 7.5),
        (4.0, 7.5)
    );
}
