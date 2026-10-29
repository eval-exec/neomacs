#[test]
fn default_rasterization_does_not_assume_an_rgb_lcd() {
    assert!(!super::default_subpixel_order().allows_horizontal_subpixel());
}
