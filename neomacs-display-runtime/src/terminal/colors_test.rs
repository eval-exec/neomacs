use super::*;

const EPSILON: f32 = 1e-5;

/// Helper: convert an (r, g, b) u8 triple to expected Color floats.
fn expected(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0)
}

fn assert_color_eq(c: &Color, r: f32, g: f32, b: f32) {
    assert!(
        (c.r - r).abs() < EPSILON && (c.g - g).abs() < EPSILON && (c.b - b).abs() < EPSILON,
        "expected ({}, {}, {}), got ({}, {}, {})",
        r,
        g,
        b,
        c.r,
        c.g,
        c.b,
    );
}

fn assert_color_rgb(c: &Color, r: u8, g: u8, b: u8) {
    let (er, eg, eb) = expected(r, g, b);
    assert_color_eq(c, er, eg, eb);
}

// ---------------------------------------------------------------
// 1. Standard 16 ANSI colors (0-15) - verify known color values
// ---------------------------------------------------------------

#[test]
fn test_standard_16_black() {
    assert_color_rgb(&COLOR_256[0], 0, 0, 0);
}

#[test]
fn test_standard_16_red() {
    assert_color_rgb(&COLOR_256[1], 205, 0, 0);
}

#[test]
fn test_standard_16_green() {
    assert_color_rgb(&COLOR_256[2], 0, 205, 0);
}

#[test]
fn test_standard_16_yellow() {
    assert_color_rgb(&COLOR_256[3], 205, 205, 0);
}

#[test]
fn test_standard_16_blue() {
    assert_color_rgb(&COLOR_256[4], 0, 0, 238);
}

#[test]
fn test_standard_16_magenta() {
    assert_color_rgb(&COLOR_256[5], 205, 0, 205);
}

#[test]
fn test_standard_16_cyan() {
    assert_color_rgb(&COLOR_256[6], 0, 205, 205);
}

#[test]
fn test_standard_16_white() {
    assert_color_rgb(&COLOR_256[7], 229, 229, 229);
}

#[test]
fn test_standard_16_bright_black() {
    assert_color_rgb(&COLOR_256[8], 127, 127, 127);
}

#[test]
fn test_standard_16_bright_red() {
    assert_color_rgb(&COLOR_256[9], 255, 0, 0);
}

#[test]
fn test_standard_16_bright_green() {
    assert_color_rgb(&COLOR_256[10], 0, 255, 0);
}

#[test]
fn test_standard_16_bright_yellow() {
    assert_color_rgb(&COLOR_256[11], 255, 255, 0);
}

#[test]
fn test_standard_16_bright_blue() {
    assert_color_rgb(&COLOR_256[12], 92, 92, 255);
}

#[test]
fn test_standard_16_bright_magenta() {
    assert_color_rgb(&COLOR_256[13], 255, 0, 255);
}

#[test]
fn test_standard_16_bright_cyan() {
    assert_color_rgb(&COLOR_256[14], 0, 255, 255);
}

#[test]
fn test_standard_16_bright_white() {
    assert_color_rgb(&COLOR_256[15], 255, 255, 255);
}

#[test]
fn test_standard_16_all_alpha_is_one() {
    for i in 0..16 {
        assert!(
            (COLOR_256[i].a - 1.0).abs() < EPSILON,
            "color index {} has alpha {}, expected 1.0",
            i,
            COLOR_256[i].a,
        );
    }
}

// ---------------------------------------------------------------
// 2. 216-color cube (16-231) - boundary values and known colors
// ---------------------------------------------------------------

/// The 6x6x6 cube uses: 0 -> 0, 1 -> 95, 2 -> 135, 3 -> 175, 4 -> 215, 5 -> 255
fn cube_component(c: usize) -> u8 {
    if c == 0 { 0 } else { (55 + 40 * c) as u8 }
}

#[test]
fn test_cube_index_16_is_black() {
    // Cube (0,0,0) = index 16
    assert_color_rgb(&COLOR_256[16], 0, 0, 0);
}

#[test]
fn test_cube_index_231_is_white() {
    // Cube (5,5,5) = index 16 + 5*36 + 5*6 + 5 = 231
    assert_color_rgb(&COLOR_256[231], 255, 255, 255);
}

#[test]
fn test_cube_pure_red() {
    // Cube (5,0,0) = index 16 + 5*36 = 196
    assert_color_rgb(&COLOR_256[196], 255, 0, 0);
}

#[test]
fn test_cube_pure_green() {
    // Cube (0,5,0) = index 16 + 5*6 = 46
    assert_color_rgb(&COLOR_256[46], 0, 255, 0);
}

#[test]
fn test_cube_pure_blue() {
    // Cube (0,0,5) = index 16 + 5 = 21
    assert_color_rgb(&COLOR_256[21], 0, 0, 255);
}

#[test]
fn test_cube_component_values() {
    // Verify the cube formula: c == 0 => 0, else 55 + 40*c
    assert_eq!(cube_component(0), 0);
    assert_eq!(cube_component(1), 95);
    assert_eq!(cube_component(2), 135);
    assert_eq!(cube_component(3), 175);
    assert_eq!(cube_component(4), 215);
    assert_eq!(cube_component(5), 255);
}

#[test]
fn test_cube_all_entries_systematic() {
    // Verify every entry in the 6x6x6 cube against the formula
    for ri in 0..6usize {
        for gi in 0..6usize {
            for bi in 0..6usize {
                let idx = 16 + ri * 36 + gi * 6 + bi;
                let c = &COLOR_256[idx];
                let er = cube_component(ri) as f32 / 255.0;
                let eg = cube_component(gi) as f32 / 255.0;
                let eb = cube_component(bi) as f32 / 255.0;
                assert!(
                    (c.r - er).abs() < EPSILON
                        && (c.g - eg).abs() < EPSILON
                        && (c.b - eb).abs() < EPSILON,
                    "cube ({},{},{}) at index {}: expected ({},{},{}), got ({},{},{})",
                    ri,
                    gi,
                    bi,
                    idx,
                    er,
                    eg,
                    eb,
                    c.r,
                    c.g,
                    c.b,
                );
                assert!((c.a - 1.0).abs() < EPSILON);
            }
        }
    }
}

#[test]
fn test_cube_boundary_index_17() {
    // Cube (0,0,1) = index 17
    assert_color_rgb(&COLOR_256[17], 0, 0, 95);
}

#[test]
fn test_cube_mid_value() {
    // Cube (3,3,3) = index 16 + 3*36 + 3*6 + 3 = 145
    assert_color_rgb(&COLOR_256[145], 175, 175, 175);
}

// ---------------------------------------------------------------
// 3. Grayscale ramp (232-255) - verify gradient values
// ---------------------------------------------------------------

#[test]
fn test_grayscale_first() {
    // Index 232: v = (8 + 10*0) / 255 = 8/255
    let v = 8.0 / 255.0;
    assert_color_eq(&COLOR_256[232], v, v, v);
}

#[test]
fn test_grayscale_last() {
    // Index 255: v = (8 + 10*23) / 255 = 238/255
    let v = 238.0 / 255.0;
    assert_color_eq(&COLOR_256[255], v, v, v);
}

#[test]
fn test_grayscale_monotonically_increasing() {
    for i in 232..255 {
        assert!(
            COLOR_256[i + 1].r > COLOR_256[i].r,
            "grayscale not increasing at index {}: {} <= {}",
            i,
            COLOR_256[i + 1].r,
            COLOR_256[i].r,
        );
    }
}

#[test]
fn test_grayscale_all_entries() {
    for i in 0..24 {
        let v = (8 + 10 * i) as f32 / 255.0;
        let c = &COLOR_256[232 + i];
        assert_color_eq(c, v, v, v);
        assert!((c.a - 1.0).abs() < EPSILON);
    }
}

#[test]
fn test_grayscale_is_neutral() {
    // All grayscale entries should have r == g == b
    for i in 232..=255 {
        let c = &COLOR_256[i];
        assert!(
            (c.r - c.g).abs() < EPSILON && (c.g - c.b).abs() < EPSILON,
            "grayscale index {} is not neutral: ({}, {}, {})",
            i,
            c.r,
            c.g,
            c.b,
        );
    }
}

// ---------------------------------------------------------------
// 4. Named color indices via ansi_to_color
// ---------------------------------------------------------------

#[test]
fn test_named_black() {
    let fg = Color::WHITE;
    let bg = Color::BLACK;
    let c = ansi_to_color(&AnsiColor::Named(NamedColor::Black), &fg, &bg);
    assert_color_rgb(&c, 0, 0, 0);
}

#[test]
fn test_named_red() {
    let fg = Color::WHITE;
    let bg = Color::BLACK;
    let c = ansi_to_color(&AnsiColor::Named(NamedColor::Red), &fg, &bg);
    assert_color_rgb(&c, 205, 0, 0);
}

#[test]
fn test_named_green() {
    let fg = Color::WHITE;
    let bg = Color::BLACK;
    let c = ansi_to_color(&AnsiColor::Named(NamedColor::Green), &fg, &bg);
    assert_color_rgb(&c, 0, 205, 0);
}

#[test]
fn test_named_yellow() {
    let fg = Color::WHITE;
    let bg = Color::BLACK;
    let c = ansi_to_color(&AnsiColor::Named(NamedColor::Yellow), &fg, &bg);
    assert_color_rgb(&c, 205, 205, 0);
}

#[test]
fn test_named_blue() {
    let fg = Color::WHITE;
    let bg = Color::BLACK;
    let c = ansi_to_color(&AnsiColor::Named(NamedColor::Blue), &fg, &bg);
    assert_color_rgb(&c, 0, 0, 238);
}

#[test]
fn test_named_magenta() {
    let fg = Color::WHITE;
    let bg = Color::BLACK;
    let c = ansi_to_color(&AnsiColor::Named(NamedColor::Magenta), &fg, &bg);
    assert_color_rgb(&c, 205, 0, 205);
}

#[test]
fn test_named_cyan() {
    let fg = Color::WHITE;
    let bg = Color::BLACK;
    let c = ansi_to_color(&AnsiColor::Named(NamedColor::Cyan), &fg, &bg);
    assert_color_rgb(&c, 0, 205, 205);
}

#[test]
fn test_named_white() {
    let fg = Color::WHITE;
    let bg = Color::BLACK;
    let c = ansi_to_color(&AnsiColor::Named(NamedColor::White), &fg, &bg);
    assert_color_rgb(&c, 229, 229, 229);
}

#[test]
fn test_named_bright_black() {
    let fg = Color::WHITE;
    let bg = Color::BLACK;
    let c = ansi_to_color(&AnsiColor::Named(NamedColor::LightBlack), &fg, &bg);
    assert_color_rgb(&c, 127, 127, 127);
}

#[test]
fn test_named_bright_white() {
    let fg = Color::WHITE;
    let bg = Color::BLACK;
    let c = ansi_to_color(&AnsiColor::Named(NamedColor::LightWhite), &fg, &bg);
    assert_color_rgb(&c, 255, 255, 255);
}

#[test]
fn test_named_foreground_returns_fg() {
    let fg = Color::new(0.1, 0.2, 0.3, 1.0);
    let bg = Color::new(0.4, 0.5, 0.6, 1.0);
    let c = ansi_to_color(&AnsiColor::Named(NamedColor::Foreground), &fg, &bg);
    assert_color_eq(&c, 0.1, 0.2, 0.3);
}

#[test]
fn test_named_background_returns_bg() {
    let fg = Color::new(0.1, 0.2, 0.3, 1.0);
    let bg = Color::new(0.4, 0.5, 0.6, 1.0);
    let c = ansi_to_color(&AnsiColor::Named(NamedColor::Background), &fg, &bg);
    assert_color_eq(&c, 0.4, 0.5, 0.6);
}

#[test]
fn test_named_cursor_returns_fg() {
    let fg = Color::new(0.7, 0.8, 0.9, 1.0);
    let bg = Color::BLACK;
    let c = ansi_to_color(&AnsiColor::Named(NamedColor::Cursor), &fg, &bg);
    assert_color_eq(&c, 0.7, 0.8, 0.9);
}

// ---------------------------------------------------------------
// 5. Edge cases: index 0, 15, 16, 231, 232, 255
// ---------------------------------------------------------------

#[test]
fn test_edge_index_0() {
    let fg = Color::WHITE;
    let bg = Color::BLACK;
    let c = ansi_to_color(&AnsiColor::Indexed(0), &fg, &bg);
    assert_color_rgb(&c, 0, 0, 0);
}

#[test]
fn test_edge_index_15() {
    let fg = Color::WHITE;
    let bg = Color::BLACK;
    let c = ansi_to_color(&AnsiColor::Indexed(15), &fg, &bg);
    assert_color_rgb(&c, 255, 255, 255);
}

#[test]
fn test_edge_index_16() {
    let fg = Color::WHITE;
    let bg = Color::BLACK;
    // First entry of 6x6x6 cube: (0,0,0)
    let c = ansi_to_color(&AnsiColor::Indexed(16), &fg, &bg);
    assert_color_rgb(&c, 0, 0, 0);
}

#[test]
fn test_edge_index_231() {
    let fg = Color::WHITE;
    let bg = Color::BLACK;
    // Last entry of 6x6x6 cube: (5,5,5) = (255,255,255)
    let c = ansi_to_color(&AnsiColor::Indexed(231), &fg, &bg);
    assert_color_rgb(&c, 255, 255, 255);
}

#[test]
fn test_edge_index_232() {
    let fg = Color::WHITE;
    let bg = Color::BLACK;
    // First grayscale entry: v = 8/255
    let c = ansi_to_color(&AnsiColor::Indexed(232), &fg, &bg);
    let v = 8.0 / 255.0;
    assert_color_eq(&c, v, v, v);
}

#[test]
fn test_edge_index_255() {
    let fg = Color::WHITE;
    let bg = Color::BLACK;
    // Last grayscale entry: v = 238/255
    let c = ansi_to_color(&AnsiColor::Indexed(255), &fg, &bg);
    let v = 238.0 / 255.0;
    assert_color_eq(&c, v, v, v);
}

// ---------------------------------------------------------------
// 6. Color component ranges (all values should be 0.0-1.0)
// ---------------------------------------------------------------

#[test]
fn test_all_256_colors_in_range() {
    for i in 0..256 {
        let c = &COLOR_256[i];
        assert!(
            c.r >= 0.0 && c.r <= 1.0,
            "index {}: r={} out of range",
            i,
            c.r,
        );
        assert!(
            c.g >= 0.0 && c.g <= 1.0,
            "index {}: g={} out of range",
            i,
            c.g,
        );
        assert!(
            c.b >= 0.0 && c.b <= 1.0,
            "index {}: b={} out of range",
            i,
            c.b,
        );
        assert!(
            (c.a - 1.0).abs() < EPSILON,
            "index {}: a={} should be 1.0",
            i,
            c.a,
        );
    }
}

#[test]
fn test_spec_color_conversion() {
    let fg = Color::WHITE;
    let bg = Color::BLACK;
    let c = ansi_to_color(
        &AnsiColor::Spec(rio_vt::config::colors::ColorRgb {
            r: 128,
            g: 64,
            b: 32,
        }),
        &fg,
        &bg,
    );
    assert_color_rgb(&c, 128, 64, 32);
    assert!((c.a - 1.0).abs() < EPSILON);
}

#[test]
fn test_spec_color_extremes() {
    let fg = Color::WHITE;
    let bg = Color::BLACK;

    let black = ansi_to_color(
        &AnsiColor::Spec(rio_vt::config::colors::ColorRgb { r: 0, g: 0, b: 0 }),
        &fg,
        &bg,
    );
    assert_color_eq(&black, 0.0, 0.0, 0.0);

    let white = ansi_to_color(
        &AnsiColor::Spec(rio_vt::config::colors::ColorRgb {
            r: 255,
            g: 255,
            b: 255,
        }),
        &fg,
        &bg,
    );
    assert_color_eq(&white, 1.0, 1.0, 1.0);
}

// ---------------------------------------------------------------
// Boundary between standard colors and cube
// ---------------------------------------------------------------

#[test]
fn test_boundary_standard_to_cube() {
    // Index 15 is bright white (standard), index 16 is cube (0,0,0)
    let std_last = &COLOR_256[15];
    let cube_first = &COLOR_256[16];
    assert_color_rgb(std_last, 255, 255, 255);
    assert_color_rgb(cube_first, 0, 0, 0);
}

#[test]
fn test_boundary_cube_to_grayscale() {
    // Index 231 is cube (5,5,5), index 232 is grayscale start
    let cube_last = &COLOR_256[231];
    let gray_first = &COLOR_256[232];
    assert_color_rgb(cube_last, 255, 255, 255);
    let v = 8.0 / 255.0;
    assert_color_eq(gray_first, v, v, v);
}
