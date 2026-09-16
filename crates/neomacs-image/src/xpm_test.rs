use super::*;

#[test]
fn test_basic_xpm3() {
    let xpm = br#"/* XPM */
static char * test[] = {
"4 4 2 1",
"  c None",
"X c #FF0000",
"XXXX",
"X  X",
"X  X",
"XXXX"
};"#;
    let result = decode_xpm_data(xpm);
    assert!(result.is_some());
    let (w, h, rgba) = result.unwrap();
    assert_eq!(w, 4);
    assert_eq!(h, 4);
    assert_eq!(rgba.len(), 64); // 4*4*4
    // Top-left pixel should be red
    assert_eq!(&rgba[0..4], &[255, 0, 0, 255]);
    // Second pixel in second row should be transparent
    assert_eq!(&rgba[(1 * 4 + 1) * 4..(1 * 4 + 1) * 4 + 4], &[0, 0, 0, 0]);
}

#[test]
fn test_query_dimensions() {
    let xpm = br#"/* XPM */
static char * test[] = {
"10 20 2 1",
"  c None",
"X c #000000",
"XXXXXXXXXX",
"XXXXXXXXXX",
"XXXXXXXXXX",
"XXXXXXXXXX",
"XXXXXXXXXX",
"XXXXXXXXXX",
"XXXXXXXXXX",
"XXXXXXXXXX",
"XXXXXXXXXX",
"XXXXXXXXXX",
"XXXXXXXXXX",
"XXXXXXXXXX",
"XXXXXXXXXX",
"XXXXXXXXXX",
"XXXXXXXXXX",
"XXXXXXXXXX",
"XXXXXXXXXX",
"XXXXXXXXXX",
"XXXXXXXXXX",
"XXXXXXXXXX"
};"#;
    let dims = query_xpm_dimensions(xpm);
    assert_eq!(dims, Some((10, 20)));
}

#[test]
fn test_hex_colors() {
    assert_eq!(parse_hex_color("FF0000"), [255, 0, 0, 255]);
    assert_eq!(parse_hex_color("00FF00"), [0, 255, 0, 255]);
    assert_eq!(parse_hex_color("0000FF"), [0, 0, 255, 255]);
    assert_eq!(parse_hex_color("F00"), [255, 0, 0, 255]);
    assert_eq!(parse_hex_color("FFFF00000000"), [255, 0, 0, 255]);
}

#[test]
fn test_named_colors() {
    assert_eq!(parse_color_value("None"), [0, 0, 0, 0]);
    assert_eq!(parse_color_value("white"), [255, 255, 255, 255]);
    assert_eq!(parse_color_value("black"), [0, 0, 0, 255]);
    assert_eq!(parse_color_value("red"), [255, 0, 0, 255]);
}

#[test]
fn test_multi_cpp() {
    // chars_per_pixel = 2
    let xpm = br###"/* XPM */
static char * test[] = {
"2 2 3 2",
".. c #FFFFFF",
"## c #000000",
"   c None",
"..##",
"##.."
};"###;
    let result = decode_xpm_data(xpm);
    assert!(result.is_some());
    let (w, h, rgba) = result.unwrap();
    assert_eq!(w, 2);
    assert_eq!(h, 2);
    // (0,0) = white
    assert_eq!(&rgba[0..4], &[255, 255, 255, 255]);
    // (1,0) = black
    assert_eq!(&rgba[4..8], &[0, 0, 0, 255]);
    // (0,1) = black
    assert_eq!(&rgba[8..12], &[0, 0, 0, 255]);
    // (1,1) = white
    assert_eq!(&rgba[12..16], &[255, 255, 255, 255]);
}
