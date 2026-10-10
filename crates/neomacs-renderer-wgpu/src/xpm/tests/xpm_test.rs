use super::*;

/// The face foreground every decode in this file passes as GNU's fallback.
const FALLBACK: [u8; 3] = [0xff, 0xff, 0xff];

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
    let result = decode_xpm_data(xpm, FALLBACK);
    assert!(result.is_some());
    let (w, h, rgba) = result.unwrap();
    assert_eq!(w, 4);
    assert_eq!(h, 4);
    assert_eq!(rgba.len(), 64); // 4*4*4
    // Top-left pixel should be red
    assert_eq!(&rgba[0..4], &[255, 0, 0, 255]);
    // Second pixel in second row should be transparent
    assert_eq!(&rgba[(4 + 1) * 4..(4 + 1) * 4 + 4], &[0, 0, 0, 0]);
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

/// `None` and the plain names, through the decoder. The value syntaxes
/// themselves are pinned in `neomacs-display-protocol`'s `x11_colors` tests,
/// which is the table the face path reads too.
#[test]
fn test_named_colors() {
    assert_eq!(decode_one_pixel("None"), [0, 0, 0, 0]);
    assert_eq!(decode_one_pixel("white"), [255, 255, 255, 255]);
    assert_eq!(decode_one_pixel("black"), [0, 0, 0, 255]);
    assert_eq!(decode_one_pixel("red"), [255, 0, 0, 255]);
    assert_eq!(decode_one_pixel("#FFFF00000000"), [255, 0, 0, 255]);
}

/// One 1x1 XPM whose only color key is `a`, defined by VALUE — the text after
/// the `c` key, which may contain spaces (`light blue`).
fn one_pixel_xpm(value: &str) -> String {
    format!("/* XPM */\nstatic char *sample[] = {{\n\"1 1 1 1\",\n\"a c {value}\",\n\"a\"\n}};\n")
}

fn decode_one_pixel(value: &str) -> [u8; 4] {
    let xpm = one_pixel_xpm(value);
    let (width, height, rgba) = decode_xpm_data(xpm.as_bytes(), FALLBACK).expect("1x1 XPM decodes");
    assert_eq!((width, height), (1, 1));
    rgba[..4].try_into().expect("one pixel")
}

/// GNU does not put an unresolvable color into the color table, and paints
/// those pixels with the frame's foreground pixel (src/image.c:6512-6513,
/// 6537-6538); a pixel key the table never defined behaves the same way. Black
/// is not that color, so substituting it made the failure invisible.
#[test]
fn unresolvable_colors_take_the_face_foreground_not_black() {
    let foreground = [0x12, 0x34, 0x56];
    let xpm = one_pixel_xpm("aqua"); // a CSS name `etc/rgb.txt` never defined
    let (width, height, rgba) = decode_xpm_data(xpm.as_bytes(), foreground).expect("decodes");
    assert_eq!((width, height), (1, 1));
    // GNU's fallback is opaque; the parameter cannot carry an alpha to lose.
    assert_eq!(&rgba[..4], &[0x12, 0x34, 0x56, 0xff]);
}

/// GNU resolves XPM color values through the frame terminal's
/// `defined_color_hook` (src/image.c:6503-6511), and that hook's name database
/// is the X11 `rgb.txt` data on every platform: XParseColor against the
/// server's database under X, `etc/rgb.txt` through `x-load-color-file` on
/// NS/W32/Android. A named color must therefore decode to the database's
/// bytes, not to a private palette's.
#[test]
fn named_colors_resolve_through_the_x11_rgb_database() {
    // gray0..gray100 are what XPM artwork uses for detail shading — e.g.
    // nyan-mode's nyan.xpm and GNU's own etc/images/commit.xpm (issue #545).
    // rgb.txt: gray14 = 36,36,36, gray50 = 127,127,127, gray75 = 191,191,191.
    assert_eq!(decode_one_pixel("gray14"), [0x24, 0x24, 0x24, 255]);
    assert_eq!(decode_one_pixel("gray50"), [0x7f, 0x7f, 0x7f, 255]);
    assert_eq!(decode_one_pixel("gray75"), [0xbf, 0xbf, 0xbf, 255]);
    // X11's `green` is pure green (CSS's is #008000) and X11's `maroon` is
    // #b03060 (CSS's is #800000) — the hex controls the report compares.
    assert_eq!(decode_one_pixel("green"), [0x00, 0xff, 0x00, 255]);
    assert_eq!(decode_one_pixel("maroon"), [0xb0, 0x30, 0x60, 255]);
    // XParseColor and the W32 map both match names case-insensitively.
    assert_eq!(decode_one_pixel("GRAY14"), [0x24, 0x24, 0x24, 255]);
}

/// GNU's color-line loop joins tokens until one of them is a color key
/// (src/image.c:6459-6485), so `light blue` reaches the database as one name
/// instead of stopping at `light`.
#[test]
fn multi_word_color_names_span_every_token_before_the_next_key() {
    assert_eq!(decode_one_pixel("light blue"), [0xad, 0xd8, 0xe6, 255]);
    assert_eq!(decode_one_pixel("dark sea green"), [0x8f, 0xbc, 0x8f, 255]);
    // The value ends where the next key begins, so trailing keys still parse:
    // `m black` after `light blue` leaves the `c` value intact.
    assert_eq!(
        decode_one_pixel("light blue m #000000"),
        [0xad, 0xd8, 0xe6, 255]
    );
}

/// GNU `parse_color_spec` accepts 1..=4 hex digits per channel
/// (src/xfaces.c:984-1017); `#RRRGGGBBB` (9 digits) is the 12-bit form XPM
/// tooling emits, and dropping it painted those pixels black.
#[test]
fn hex_color_values_cover_every_x11_channel_width() {
    assert_eq!(decode_one_pixel("#abc"), [0xaa, 0xbb, 0xcc, 255]);
    assert_eq!(decode_one_pixel("#aabbcc"), [0xaa, 0xbb, 0xcc, 255]);
    assert_eq!(decode_one_pixel("#fff000000"), [0xff, 0x00, 0x00, 255]);
    assert_eq!(decode_one_pixel("#000fff000"), [0x00, 0xff, 0x00, 255]);
    assert_eq!(decode_one_pixel("#ffff00000000"), [0xff, 0x00, 0x00, 255]);
}

/// The reported asset: the grayNN ladder in the palette of the shipped
/// `etc/images/commit.xpm` must arrive as rgb.txt's values. Pre-fix every one
/// of them decoded to black, which is the shadow the report's screenshot shows.
#[test]
fn shipped_commit_xpm_keeps_its_named_gray_ladder() {
    let path = neomacs_infra::workspace_root().join("etc/images/commit.xpm");
    let (width, height, rgba) = decode_xpm_file(&path, FALLBACK).expect("commit.xpm decodes");
    assert!(width > 0 && height > 0);
    let painted: std::collections::HashSet<[u8; 3]> = rgba
        .chunks_exact(4)
        .map(|pixel| [pixel[0], pixel[1], pixel[2]])
        .collect();
    // The named entries this file's pixel data actually paints, with rgb.txt's
    // values for them.
    for (name, expected) in [
        ("gray13", [33, 33, 33]),
        ("gray14", [36, 36, 36]),
        ("gray25", [64, 64, 64]),
        ("gray75", [191, 191, 191]),
        ("gray98", [250, 250, 250]),
        ("white", [255, 255, 255]),
    ] {
        assert!(
            painted.contains(&expected),
            "{name} is not painted as {expected:?} in {}",
            path.display()
        );
    }
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
    let result = decode_xpm_data(xpm, FALLBACK);
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
