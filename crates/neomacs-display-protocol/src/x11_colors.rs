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

/// The `#`-hex form alone, reduced to 8-bit channels.
///
/// GNU's `parse_color_spec` (`src/xfaces.c:976`) accepts 1..=4 hex digits per
/// channel; each channel is scaled rather than zero-extended (`#f00` is
/// `#ff0000`, not `#f00000`, `src/xterm.c:9276-9280`) and then reduced to the
/// most significant 8 bits, which is what GNU does to the same 16-bit value
/// when it makes the pixel (`lookup_rgb_color`, `src/image.c:6884-6892`).
#[must_use]
pub fn x11_hex_color(spec: &str) -> Option<(u8, u8, u8)> {
    if !spec.starts_with('#') {
        return None;
    }
    x11_color_spec_16bit(spec.as_bytes()).map(reduce_to_8_bits)
}

/// Resolve one color *value* the way GNU hands it to its color hook: a numeric
/// spec (`#`-hex, `rgb:`, `rgbi:`) or a name in the X11 database.
///
/// `None` (the XPM transparency keyword) is the caller's to recognize, because
/// GNU checks it before the hook is ever consulted (`src/image.c:6505`).
#[must_use]
pub fn x11_color_value(value: &str) -> Option<(u8, u8, u8)> {
    x11_color_spec_16bit(value.as_bytes())
        .map(reduce_to_8_bits)
        .or_else(|| x11_color_lookup(value))
}

/// GNU `lookup_rgb_color` (`src/image.c:6884-6892`) at the only point that
/// matters to us: the hook's 16-bit channels become their high bytes.
fn reduce_to_8_bits((red, green, blue): (u16, u16, u16)) -> (u8, u8, u8) {
    ((red >> 8) as u8, (green >> 8) as u8, (blue >> 8) as u8)
}

/// `#`-prefixed hex payload split into three equal-length components:
/// GNU's `(len - 1) % 3 == 0` arm of `parse_color_spec` (src/xfaces.c:984).
fn parse_hex_color_payload(payload: &[u8]) -> Option<(u16, u16, u16)> {
    if payload.is_empty() || !payload.len().is_multiple_of(3) {
        return None;
    }
    let component_len = payload.len() / 3;
    let red = parse_hex_color_comp(&payload[..component_len])?;
    let green = parse_hex_color_comp(&payload[component_len..2 * component_len])?;
    let blue = parse_hex_color_comp(&payload[2 * component_len..])?;
    Some((red, green, blue))
}

/// One hex color component of 1-4 digits, normalized so the maximum value for
/// that digit count becomes 65535.
///
/// Mirrors GNU `parse_hex_color_comp` (src/xfaces.c:928): it walks the spec's
/// BYTES and fails on any non-hex byte, so a multi-byte character (whose UTF-8
/// bytes are all non-hex) is rejected rather than sliced.
fn parse_hex_color_comp(component: &[u8]) -> Option<u16> {
    let digits = component.len();
    if digits == 0 || digits > 4 {
        return None;
    }
    let mut value: u32 = 0;
    for &byte in component {
        let digit = match byte {
            b'0'..=b'9' => byte - b'0',
            b'A'..=b'F' => byte - b'A' + 10,
            b'a'..=b'f' => byte - b'a' + 10,
            _ => return None,
        };
        value = (value << 4) | u32::from(digit);
    }
    let max_value = (1u32 << (digits * 4)) - 1;
    Some((value * 65535 / max_value) as u16)
}

/// Decimal float component in [0,1], scaled to 16 bits.
///
/// Mirrors GNU `parse_float_color_comp` (src/xfaces.c:955): only decimal
/// literals without whitespace are accepted; an EMPTY component is `strtod`'s
/// 0.0 with `end == s == e`, so it parses as 0; and the scale uses `lrint`'s
/// round-half-to-even.
fn parse_float_color_comp(component: &[u8]) -> Option<u16> {
    if !component
        .iter()
        .all(|byte| matches!(byte, b'0'..=b'9' | b'.' | b'+' | b'-' | b'e' | b'E'))
    {
        return None;
    }
    let value: f64 = if component.is_empty() {
        0.0
    } else {
        // Every accepted byte is ASCII, so the UTF-8 conversion cannot fail,
        // and `parse` consumes the whole component like GNU's `end == e`.
        std::str::from_utf8(component).ok()?.parse().ok()?
    };
    if (0.0..=1.0).contains(&value) {
        Some((value * 65535.0).round_ties_even() as u16)
    } else {
        None
    }
}

/// Parse GNU's numeric color forms into 16-bit channels, shared by the
/// evaluator's `color-values` and the renderers that resolve image color values
/// (XPM `c` keys).
///
/// One parser, so the two cannot drift: a renderer takes each channel's most
/// significant 8 bits (`x11_hex_color` / `x11_color_value`), which is what GNU
/// does to the same 16-bit value when it makes the pixel (`lookup_rgb_color`,
/// `src/image.c:6884-6892`).
/// GNU `parse_color_spec` (src/xfaces.c:976): the three numeric color forms
/// `#RGB`, `rgb:R/G/B` and `rgbi:R/G/B`, each component 1-4 hex digits (or a
/// float in [0,1] for `rgbi`).
#[must_use]
pub fn x11_color_spec_16bit(spec: &[u8]) -> Option<(u16, u16, u16)> {
    if let Some(payload) = spec.strip_prefix(b"#") {
        return parse_hex_color_payload(payload);
    }
    if let Some(rest) = spec.strip_prefix(b"rgb:") {
        let mut components = rest.splitn(3, |&byte| byte == b'/');
        let red = parse_hex_color_comp(components.next()?)?;
        let green = parse_hex_color_comp(components.next()?)?;
        // GNU measures the last component to the end of the string, so a
        // further '/' stays inside it and fails the hex validation.
        let blue = parse_hex_color_comp(components.next()?)?;
        return Some((red, green, blue));
    }
    if let Some(rest) = spec.strip_prefix(b"rgbi:") {
        let mut components = rest.splitn(3, |&byte| byte == b'/');
        let red = parse_float_color_comp(components.next()?)?;
        let green = parse_float_color_comp(components.next()?)?;
        let blue = parse_float_color_comp(components.next()?)?;
        return Some((red, green, blue));
    }
    None
}

#[cfg(test)]
#[path = "x11_colors/tests/x11_colors_test.rs"]
mod tests;
