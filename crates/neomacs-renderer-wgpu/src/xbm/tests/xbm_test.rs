use super::*;

#[test]
fn test_basic_xbm() {
    // A simple 8x2 XBM
    let xbm = b"#define test_width 8\n\
                 #define test_height 2\n\
                 static unsigned char test_bits[] = {\n\
                   0xff, 0x00 };\n";
    let fg = [255, 255, 255, 255]; // white
    let bg = [0, 0, 0, 255]; // black
    let result = decode_xbm_data(xbm, fg, bg);
    assert!(result.is_some());
    let (w, h, rgba) = result.unwrap();
    assert_eq!(w, 8);
    assert_eq!(h, 2);
    assert_eq!(rgba.len(), 8 * 2 * 4);
    // First row: all bits set -> all white
    for x in 0..8 {
        assert_eq!(&rgba[x * 4..x * 4 + 4], &fg);
    }
    // Second row: no bits set -> all black
    for x in 0..8 {
        let idx = (8 + x) * 4;
        assert_eq!(&rgba[idx..idx + 4], &bg);
    }
}

#[test]
fn test_query_dimensions() {
    let xbm = b"#define icon_width 16\n#define icon_height 32\nstatic ...";
    let dims = query_xbm_dimensions(xbm);
    assert_eq!(dims, Some((16, 32)));
}

#[test]
fn test_lsb_first() {
    // 4x1 XBM, byte 0x05 = 0b00000101 -> LSB first: bits 0,2 set
    let xbm = b"#define t_width 4\n\
                 #define t_height 1\n\
                 static unsigned char t_bits[] = { 0x05 };\n";
    let fg = [255, 0, 0, 255]; // red
    let bg = [0, 0, 255, 255]; // blue
    let result = decode_xbm_data(xbm, fg, bg);
    assert!(result.is_some());
    let (w, h, rgba) = result.unwrap();
    assert_eq!(w, 4);
    assert_eq!(h, 1);
    // Pixel 0: bit 0 set -> fg (red)
    assert_eq!(&rgba[0..4], &fg);
    // Pixel 1: bit 1 unset -> bg (blue)
    assert_eq!(&rgba[4..8], &bg);
    // Pixel 2: bit 2 set -> fg (red)
    assert_eq!(&rgba[8..12], &fg);
    // Pixel 3: bit 3 unset -> bg (blue)
    assert_eq!(&rgba[12..16], &bg);
}

#[test]
fn test_hex_parsing() {
    assert_eq!(parse_c_integer("0xff"), Some(255));
    assert_eq!(parse_c_integer("0xFF"), Some(255));
    assert_eq!(parse_c_integer("0x0a"), Some(10));
    assert_eq!(parse_c_integer("255"), Some(255));
    assert_eq!(parse_c_integer("0"), Some(0));
}

#[test]
fn test_custom_colors() {
    // 2x1 XBM, byte 0x01 -> bit 0 set, bit 1 unset
    let xbm = b"#define t_width 2\n\
                 #define t_height 1\n\
                 static unsigned char t_bits[] = { 0x01 };\n";
    let fg = [0, 255, 0, 255]; // green foreground
    let bg = [128, 128, 128, 255]; // gray background
    let result = decode_xbm_data(xbm, fg, bg);
    assert!(result.is_some());
    let (_, _, rgba) = result.unwrap();
    assert_eq!(&rgba[0..4], &fg); // bit 0 set -> green
    assert_eq!(&rgba[4..8], &bg); // bit 1 unset -> gray
}
