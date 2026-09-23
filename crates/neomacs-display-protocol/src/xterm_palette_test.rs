use crate::xterm_256_rgb;

#[test]
fn xterm_256_palette_covers_ansi_cube_and_grayscale_regions() {
    assert_eq!(xterm_256_rgb(1), (205, 0, 0));
    assert_eq!(xterm_256_rgb(16), (0, 0, 0));
    assert_eq!(xterm_256_rgb(196), (255, 0, 0));
    assert_eq!(xterm_256_rgb(231), (255, 255, 255));
    assert_eq!(xterm_256_rgb(232), (8, 8, 8));
    assert_eq!(xterm_256_rgb(255), (238, 238, 238));
}
