//! Color conversion from rio-vt colors to neomacs Color.

use crate::core::types::Color;
use rio_vt::config::colors::{AnsiColor, NamedColor};

/// Default 256-color palette (standard ANSI + extended colors).
/// First 16 are the standard terminal colors, 16-231 are the 6x6x6 color cube,
/// 232-255 are the grayscale ramp.
static COLOR_256: once_cell::sync::Lazy<[Color; 256]> = once_cell::sync::Lazy::new(|| {
    let mut colors = [Color::BLACK; 256];

    // Standard 16 colors (dark + bright variants)
    let named: [(u8, u8, u8); 16] = [
        (0, 0, 0),       // Black
        (205, 0, 0),     // Red
        (0, 205, 0),     // Green
        (205, 205, 0),   // Yellow
        (0, 0, 238),     // Blue
        (205, 0, 205),   // Magenta
        (0, 205, 205),   // Cyan
        (229, 229, 229), // White
        (127, 127, 127), // Bright Black
        (255, 0, 0),     // Bright Red
        (0, 255, 0),     // Bright Green
        (255, 255, 0),   // Bright Yellow
        (92, 92, 255),   // Bright Blue
        (255, 0, 255),   // Bright Magenta
        (0, 255, 255),   // Bright Cyan
        (255, 255, 255), // Bright White
    ];
    for (i, (r, g, b)) in named.iter().enumerate() {
        colors[i] = Color {
            r: *r as f32 / 255.0,
            g: *g as f32 / 255.0,
            b: *b as f32 / 255.0,
            a: 1.0,
        };
    }

    // 6x6x6 color cube (indices 16-231)
    for i in 0..216 {
        let r = (i / 36) % 6;
        let g = (i / 6) % 6;
        let b = i % 6;
        let to_val = |c: usize| -> f32 {
            if c == 0 {
                0.0
            } else {
                (55 + 40 * c) as f32 / 255.0
            }
        };
        colors[16 + i] = Color {
            r: to_val(r),
            g: to_val(g),
            b: to_val(b),
            a: 1.0,
        };
    }

    // Grayscale ramp (indices 232-255)
    for i in 0..24 {
        let v = (8 + 10 * i) as f32 / 255.0;
        colors[232 + i] = Color {
            r: v,
            g: v,
            b: v,
            a: 1.0,
        };
    }

    colors
});

/// Convert a rio-vt AnsiColor to a neomacs Color.
///
/// `default_fg` and `default_bg` are used when the color is `Named(Foreground)`
/// or `Named(Background)`.
pub fn ansi_to_color(color: &AnsiColor, default_fg: &Color, default_bg: &Color) -> Color {
    match color {
        AnsiColor::Named(named) => named_to_color(*named, default_fg, default_bg),
        AnsiColor::Spec(rgb) => Color {
            r: rgb.r as f32 / 255.0,
            g: rgb.g as f32 / 255.0,
            b: rgb.b as f32 / 255.0,
            a: 1.0,
        },
        AnsiColor::Indexed(idx) => COLOR_256[*idx as usize],
    }
}

/// Convert a named ANSI color to neomacs Color.
fn named_to_color(named: NamedColor, default_fg: &Color, default_bg: &Color) -> Color {
    match named {
        NamedColor::Foreground => *default_fg,
        NamedColor::Background => *default_bg,
        NamedColor::Cursor => *default_fg,
        NamedColor::Black => COLOR_256[0],
        NamedColor::Red => COLOR_256[1],
        NamedColor::Green => COLOR_256[2],
        NamedColor::Yellow => COLOR_256[3],
        NamedColor::Blue => COLOR_256[4],
        NamedColor::Magenta => COLOR_256[5],
        NamedColor::Cyan => COLOR_256[6],
        NamedColor::White => COLOR_256[7],
        NamedColor::LightBlack => COLOR_256[8],
        NamedColor::LightRed => COLOR_256[9],
        NamedColor::LightGreen => COLOR_256[10],
        NamedColor::LightYellow => COLOR_256[11],
        NamedColor::LightBlue => COLOR_256[12],
        NamedColor::LightMagenta => COLOR_256[13],
        NamedColor::LightCyan => COLOR_256[14],
        NamedColor::LightWhite => COLOR_256[15],
        _ => *default_fg,
    }
}

#[cfg(test)]
#[path = "colors_test.rs"]
mod tests;
