//! Color conversion from rio-vt colors to neomacs Color.

use crate::core::types::Color;
use neomacs_display_protocol::xterm_256_rgb;
use rio_vt::config::colors::{AnsiColor, NamedColor};

/// Default 256-color palette (standard ANSI + extended colors).
/// First 16 are the standard terminal colors, 16-231 are the 6x6x6 color cube,
/// 232-255 are the grayscale ramp.
static COLOR_256: std::sync::LazyLock<[Color; 256]> = std::sync::LazyLock::new(|| {
    std::array::from_fn(|index| {
        let (red, green, blue) = xterm_256_rgb(index as u8);
        Color::from_u8(red, green, blue, 255)
    })
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
mod tests;
