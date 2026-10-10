//! The X11 color-name database (`etc/rgb.txt`) and the value syntaxes that name
//! it, shared by every resolver in the workspace.
//!
//! GNU resolves a color name through the frame terminal's `defined_color_hook`
//! (`src/xfaces.c`, `tty_defined_color` / `x_defined_color`), and every platform
//! answers from the same X11R6 `rgb.txt` data: the X server's copy under X
//! (`XParseColor`), and `etc/rgb.txt` itself through `x-load-color-file`
//! (`src/xfaces.c:7251`) on NS, W32 and Android. Face realization, `color-values`
//! and the image formats that carry names (XPM `c` keys) are all that one hook.
//!
//! This module exists because that answer had two sources: `neovm-core` resolved
//! face colors from `etc/rgb.txt` while the renderer's XPM decoder carried a
//! private CSS-flavored palette. The two disagreed exactly where the databases
//! differ, so `gray14` and `light blue` rendered black (unknown to the CSS
//! table) and `green`/`maroon` took their CSS values instead of X11's
//! (issue #545). `docs/design/display-crate-layout.md` gives the rule: a value
//! two crates can disagree about belongs here, next to `xterm_palette`.
//!
//! Scope: the database and the `#`-hex syntax. GNU's hook also accepts
//! `rgb:R/G/B` and `rgbi:R/G/B` (its `parse_color_spec`), which the evaluator
//! answers in its own 16-bit form; renderers do not meet those in image data.

include!(concat!(env!("OUT_DIR"), "/x11_colors.rs"));

/// Parse a `#`-prefixed X11 hex color: 3, 6, 9 or 12 hex digits, i.e. 1..=4
/// digits per channel.
///
/// GNU's `parse_color_spec` (`src/xfaces.c:976`) accepts the same four widths,
/// and deliberately scales rather than zero-extends each channel — `#f00` is
/// `#ff0000`, not `#f00000` (`src/xterm.c:9276-9280`). Each channel is
/// accumulated at its own width and reduced to the most-significant 8 bits,
/// which is what GNU's 16-bit value reduces to when it is drawn.
#[must_use]
pub fn x11_hex_color(spec: &str) -> Option<(u8, u8, u8)> {
    let digits = spec.strip_prefix('#')?.as_bytes();
    if digits.is_empty() || digits.len() % 3 != 0 {
        return None;
    }
    let per_channel = digits.len() / 3;
    if per_channel > 4 {
        return None;
    }
    let bits = 4 * per_channel as u32;
    let channel = |index: usize| -> Option<u8> {
        let mut raw: u16 = 0;
        for &byte in &digits[index * per_channel..(index + 1) * per_channel] {
            raw = (raw << 4) | u16::from(hex_digit(byte)?);
        }
        // 4-bit `#RGB`: replicate the nibble so 0xf -> 0xff (== 0xf * 0x11).
        Some(if bits >= 8 {
            (raw >> (bits - 8)) as u8
        } else {
            ((raw << 4) | raw) as u8
        })
    };
    Some((channel(0)?, channel(1)?, channel(2)?))
}

/// Resolve one color *value*: `#`-hex, or a name in the X11 database.
///
/// This is the RGB half of what GNU hands to its color hook; `None` (the XPM
/// transparency keyword) is the caller's to recognize, because GNU checks it
/// before the hook is ever consulted (`src/image.c:6505`).
#[must_use]
pub fn x11_color_value(value: &str) -> Option<(u8, u8, u8)> {
    x11_hex_color(value).or_else(|| x11_color_lookup(value))
}

/// One hex digit, or `None` for anything else — GNU walks the spec's *bytes*
/// and fails on a non-hex byte (`parse_hex_color_comp`, `src/xfaces.c:928`), so
/// a multi-byte character is rejected rather than sliced.
fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
#[path = "x11_colors/tests/x11_colors_test.rs"]
mod tests;
