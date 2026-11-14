use super::scroll_bar_visible_band;

#[test]
fn default_visible_band_is_half_the_allocated_band() {
    // GNU's GtkScrollbar allocates 16px under Breeze/Adwaita but paints an
    // approximately 8px trough/slider centered inside it. The layout band
    // stays 16px; only the painted band is inset.
    assert_eq!(scroll_bar_visible_band(16.0, 0), (4.0, 8.0));
    assert_eq!(scroll_bar_visible_band(20.0, 0), (5.0, 10.0));
}

#[test]
fn explicit_visible_width_is_clamped_to_the_allocated_band() {
    assert_eq!(scroll_bar_visible_band(16.0, 6), (5.0, 6.0));
    assert_eq!(scroll_bar_visible_band(16.0, 40), (0.0, 16.0));
    // A negative width is not a request to paint outside the band.
    assert_eq!(scroll_bar_visible_band(16.0, -1), (4.0, 8.0));
}

#[test]
fn tiny_band_keeps_one_visible_pixel() {
    assert_eq!(scroll_bar_visible_band(1.0, 0), (0.0, 1.0));
    // A sub-pixel band cannot hold a one-pixel mark; paint what exists.
    assert_eq!(scroll_bar_visible_band(0.5, 0), (0.0, 0.5));
    assert_eq!(scroll_bar_visible_band(0.0, 0), (0.0, 0.0));
}
