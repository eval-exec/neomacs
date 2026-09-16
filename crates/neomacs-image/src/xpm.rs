//! Pure Rust XPM (X PixMap) decoder
//!
//! Parses XPM2 and XPM3 format image data and produces RGBA pixel buffers.

use std::collections::HashMap;
use std::path::Path;

/// Decode XPM image from in-memory data, returning (width, height, rgba_pixels).
pub fn decode_xpm_data(data: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let strings = extract_strings(data)?;
    decode_from_strings(&strings)
}

/// Decode XPM image from a file path.
pub fn decode_xpm_file(path: &Path) -> Option<(u32, u32, Vec<u8>)> {
    let data = std::fs::read(path).ok()?;
    decode_xpm_data(&data)
}

/// Query XPM dimensions without full decode (header only).
pub fn query_xpm_dimensions(data: &[u8]) -> Option<(u32, u32)> {
    let strings = extract_strings(data)?;
    if strings.is_empty() {
        return None;
    }
    let header = parse_header(strings[0])?;
    Some((header.width, header.height))
}

struct XpmHeader {
    width: u32,
    height: u32,
    ncolors: u32,
    chars_per_pixel: u32,
}

fn parse_header(s: &[u8]) -> Option<XpmHeader> {
    let text = std::str::from_utf8(s).ok()?;
    let mut parts = text.split_whitespace();
    let width: u32 = parts.next()?.parse().ok()?;
    let height: u32 = parts.next()?.parse().ok()?;
    let ncolors: u32 = parts.next()?.parse().ok()?;
    let chars_per_pixel: u32 = parts.next()?.parse().ok()?;
    if width == 0 || height == 0 || ncolors == 0 || chars_per_pixel == 0 {
        return None;
    }
    Some(XpmHeader {
        width,
        height,
        ncolors,
        chars_per_pixel,
    })
}

/// Extract quoted strings from XPM data.
/// Handles both XPM3 (/* XPM */ with C string array) and XPM2 (! XPM2 with plain lines).
fn extract_strings(data: &[u8]) -> Option<Vec<&[u8]>> {
    // Check for XPM2 format: starts with "! XPM2"
    if data.starts_with(b"! XPM2") {
        return extract_xpm2_lines(data);
    }

    // XPM3 format: extract C string literals between double quotes
    let mut strings = Vec::new();
    let mut i = 0;
    while i < data.len() {
        if data[i] == b'"' {
            i += 1;
            let start = i;
            while i < data.len() && data[i] != b'"' {
                // Handle backslash escapes
                if data[i] == b'\\' && i + 1 < data.len() {
                    i += 2;
                } else {
                    i += 1;
                }
            }
            strings.push(&data[start..i]);
            if i < data.len() {
                i += 1; // skip closing quote
            }
        } else {
            i += 1;
        }
    }

    if strings.is_empty() {
        None
    } else {
        Some(strings)
    }
}

/// Extract lines from XPM2 format (plain text, no C wrapper).
fn extract_xpm2_lines(data: &[u8]) -> Option<Vec<&[u8]>> {
    let mut lines: Vec<&[u8]> = Vec::new();
    for line in data.split(|&b| b == b'\n') {
        let trimmed = trim_bytes(line);
        // Skip empty lines and the header line "! XPM2"
        if trimmed.is_empty() || trimmed.starts_with(b"! XPM2") || trimmed.starts_with(b"!") {
            continue;
        }
        lines.push(trimmed);
    }
    if lines.is_empty() { None } else { Some(lines) }
}

fn trim_bytes(b: &[u8]) -> &[u8] {
    let start = b
        .iter()
        .position(|&c| c != b' ' && c != b'\t' && c != b'\r')
        .unwrap_or(b.len());
    let end = b
        .iter()
        .rposition(|&c| c != b' ' && c != b'\t' && c != b'\r')
        .map_or(start, |p| p + 1);
    &b[start..end]
}

fn decode_from_strings(strings: &[&[u8]]) -> Option<(u32, u32, Vec<u8>)> {
    if strings.is_empty() {
        return None;
    }

    let header = parse_header(strings[0])?;
    let cpp = header.chars_per_pixel as usize;
    let expected_strings = 1 + header.ncolors as usize + header.height as usize;
    if strings.len() < expected_strings {
        tracing::warn!(
            "XPM: expected {} strings, got {}",
            expected_strings,
            strings.len()
        );
        return None;
    }

    // Parse color table
    let mut colors: HashMap<Vec<u8>, [u8; 4]> = HashMap::with_capacity(header.ncolors as usize);
    for i in 0..header.ncolors as usize {
        let line = strings[1 + i];
        if line.len() < cpp {
            tracing::warn!("XPM: color line {} too short", i);
            return None;
        }
        let key = line[..cpp].to_vec();
        let rest = &line[cpp..];
        let color = parse_color_def(rest)?;
        colors.insert(key, color);
    }

    // Parse pixel data
    let w = header.width as usize;
    let h = header.height as usize;
    let mut rgba = vec![0u8; w * h * 4];

    for y in 0..h {
        let row = strings[1 + header.ncolors as usize + y];
        for x in 0..w {
            let start = x * cpp;
            let end = start + cpp;
            if end > row.len() {
                tracing::warn!(
                    "XPM: row {} too short (need {} bytes, have {})",
                    y,
                    end,
                    row.len()
                );
                return None;
            }
            let pixel_key = &row[start..end];
            let color = colors.get(pixel_key).unwrap_or(&[0, 0, 0, 255]);
            let idx = (y * w + x) * 4;
            rgba[idx] = color[0];
            rgba[idx + 1] = color[1];
            rgba[idx + 2] = color[2];
            rgba[idx + 3] = color[3];
        }
    }

    let width = header.width;
    let height = header.height;

    Some((width, height, rgba))
}

/// Parse a color definition from the rest of a color line (after the pixel key).
/// Looks for "c <color>" (visual color key). Falls back to other keys.
fn parse_color_def(rest: &[u8]) -> Option<[u8; 4]> {
    let text = std::str::from_utf8(rest).ok()?;
    let tokens: Vec<&str> = text.split_whitespace().collect();

    // Look for color key "c" (preferred), then "g" (grayscale), then "m" (mono)
    for key in &["c", "g", "m"] {
        for i in 0..tokens.len() {
            if tokens[i].eq_ignore_ascii_case(key) && i + 1 < tokens.len() {
                // Color value may span multiple tokens (e.g. "c None")
                let color_str = tokens[i + 1];
                return Some(parse_color_value(color_str));
            }
        }
    }

    // Fallback: if only one token after whitespace, treat as color
    if tokens.len() == 1 {
        return Some(parse_color_value(tokens[0]));
    }

    Some([0, 0, 0, 255]) // default black
}

/// Parse a color value string into RGBA.
fn parse_color_value(s: &str) -> [u8; 4] {
    let s = s.trim();

    // Transparent
    if s.eq_ignore_ascii_case("none") {
        return [0, 0, 0, 0];
    }

    // Hex color
    if let Some(stripped) = s.strip_prefix('#') {
        return parse_hex_color(stripped);
    }

    // Named X11 colors
    match s.to_ascii_lowercase().as_str() {
        "black" => [0, 0, 0, 255],
        "white" => [255, 255, 255, 255],
        "red" => [255, 0, 0, 255],
        "green" => [0, 128, 0, 255],
        "blue" => [0, 0, 255, 255],
        "yellow" => [255, 255, 0, 255],
        "cyan" | "aqua" => [0, 255, 255, 255],
        "magenta" | "fuchsia" => [255, 0, 255, 255],
        "gray" | "grey" => [128, 128, 128, 255],
        "darkgray" | "darkgrey" => [169, 169, 169, 255],
        "lightgray" | "lightgrey" => [211, 211, 211, 255],
        "maroon" => [128, 0, 0, 255],
        "olive" => [128, 128, 0, 255],
        "navy" => [0, 0, 128, 255],
        "purple" => [128, 0, 128, 255],
        "teal" => [0, 128, 128, 255],
        "silver" => [192, 192, 192, 255],
        "orange" => [255, 165, 0, 255],
        "pink" => [255, 192, 203, 255],
        "brown" => [165, 42, 42, 255],
        "gold" => [255, 215, 0, 255],
        "coral" => [255, 127, 80, 255],
        "salmon" => [250, 128, 114, 255],
        "tomato" => [255, 99, 71, 255],
        "khaki" => [240, 230, 140, 255],
        "violet" => [238, 130, 238, 255],
        "indigo" => [75, 0, 130, 255],
        "tan" => [210, 180, 140, 255],
        "beige" => [245, 245, 220, 255],
        "ivory" => [255, 255, 240, 255],
        "linen" => [250, 240, 230, 255],
        "wheat" => [245, 222, 179, 255],
        "snow" => [255, 250, 250, 255],
        "chocolate" => [210, 105, 30, 255],
        "sienna" => [160, 82, 45, 255],
        "peru" => [205, 133, 63, 255],
        "firebrick" => [178, 34, 34, 255],
        "crimson" => [220, 20, 60, 255],
        "darkred" => [139, 0, 0, 255],
        "darkgreen" => [0, 100, 0, 255],
        "darkblue" => [0, 0, 139, 255],
        "darkcyan" => [0, 139, 139, 255],
        "darkmagenta" => [139, 0, 139, 255],
        "darkorange" => [255, 140, 0, 255],
        "darkviolet" => [148, 0, 211, 255],
        "deeppink" => [255, 20, 147, 255],
        "deepskyblue" => [0, 191, 255, 255],
        "dimgray" | "dimgrey" => [105, 105, 105, 255],
        "dodgerblue" => [30, 144, 255, 255],
        "forestgreen" => [34, 139, 34, 255],
        "greenyellow" => [173, 255, 47, 255],
        "honeydew" => [240, 255, 240, 255],
        "hotpink" => [255, 105, 180, 255],
        "indianred" => [205, 92, 92, 255],
        "lavender" => [230, 230, 250, 255],
        "lawngreen" => [124, 252, 0, 255],
        "lemonchiffon" => [255, 250, 205, 255],
        "lightblue" => [173, 216, 230, 255],
        "lightcoral" => [240, 128, 128, 255],
        "lightcyan" => [224, 255, 255, 255],
        "lightgreen" => [144, 238, 144, 255],
        "lightpink" => [255, 182, 193, 255],
        "lightsalmon" => [255, 160, 122, 255],
        "lightseagreen" => [32, 178, 170, 255],
        "lightskyblue" => [135, 206, 250, 255],
        "lightsteelblue" => [176, 196, 222, 255],
        "lightyellow" => [255, 255, 224, 255],
        "lime" => [0, 255, 0, 255],
        "limegreen" => [50, 205, 50, 255],
        "mediumaquamarine" => [102, 205, 170, 255],
        "mediumblue" => [0, 0, 205, 255],
        "mediumorchid" => [186, 85, 211, 255],
        "mediumpurple" => [147, 112, 219, 255],
        "mediumseagreen" => [60, 179, 113, 255],
        "mediumslateblue" => [123, 104, 238, 255],
        "mediumspringgreen" => [0, 250, 154, 255],
        "mediumturquoise" => [72, 209, 204, 255],
        "mediumvioletred" => [199, 21, 133, 255],
        "midnightblue" => [25, 25, 112, 255],
        "mintcream" => [245, 255, 250, 255],
        "mistyrose" => [255, 228, 225, 255],
        "moccasin" => [255, 228, 181, 255],
        "navajowhite" => [255, 222, 173, 255],
        "oldlace" => [253, 245, 230, 255],
        "olivedrab" => [107, 142, 35, 255],
        "orangered" => [255, 69, 0, 255],
        "orchid" => [218, 112, 214, 255],
        "palegoldenrod" => [238, 232, 170, 255],
        "palegreen" => [152, 251, 152, 255],
        "paleturquoise" => [175, 238, 238, 255],
        "palevioletred" => [219, 112, 147, 255],
        "papayawhip" => [255, 239, 213, 255],
        "peachpuff" => [255, 218, 185, 255],
        "plum" => [221, 160, 221, 255],
        "powderblue" => [176, 224, 230, 255],
        "rosybrown" => [188, 143, 143, 255],
        "royalblue" => [65, 105, 225, 255],
        "saddlebrown" => [139, 69, 19, 255],
        "sandybrown" => [244, 164, 96, 255],
        "seagreen" => [46, 139, 87, 255],
        "seashell" => [255, 245, 238, 255],
        "skyblue" => [135, 206, 235, 255],
        "slateblue" => [106, 90, 205, 255],
        "slategray" | "slategrey" => [112, 128, 144, 255],
        "springgreen" => [0, 255, 127, 255],
        "steelblue" => [70, 130, 180, 255],
        "thistle" => [216, 191, 216, 255],
        "turquoise" => [64, 224, 208, 255],
        "yellowgreen" => [154, 205, 50, 255],
        "aquamarine" => [127, 255, 212, 255],
        "azure" => [240, 255, 255, 255],
        "bisque" => [255, 228, 196, 255],
        "blanchedalmond" => [255, 235, 205, 255],
        "burlywood" => [222, 184, 135, 255],
        "cadetblue" => [95, 158, 160, 255],
        "chartreuse" => [127, 255, 0, 255],
        "cornflowerblue" => [100, 149, 237, 255],
        "cornsilk" => [255, 248, 220, 255],
        "darkgoldenrod" => [184, 134, 11, 255],
        "darkolivegreen" => [85, 107, 47, 255],
        "darkorchid" => [153, 50, 204, 255],
        "darksalmon" => [233, 150, 122, 255],
        "darkseagreen" => [143, 188, 143, 255],
        "darkslateblue" => [72, 61, 139, 255],
        "darkslategray" | "darkslategrey" => [47, 79, 79, 255],
        "darkturquoise" => [0, 206, 209, 255],
        "darkyellow" | "darkkhaki" => [189, 183, 107, 255],
        "floralwhite" => [255, 250, 240, 255],
        "gainsboro" => [220, 220, 220, 255],
        "ghostwhite" => [248, 248, 255, 255],
        "goldenrod" => [218, 165, 32, 255],
        "aliceblue" => [240, 248, 255, 255],
        "antiquewhite" => [250, 235, 215, 255],
        _ => {
            tracing::debug!("XPM: unknown color name '{}', using black", s);
            [0, 0, 0, 255]
        }
    }
}

/// Parse hex color string (without '#' prefix).
fn parse_hex_color(hex: &str) -> [u8; 4] {
    let len = hex.len();
    match len {
        // #RGB
        3 => {
            let r = hex_digit(hex.as_bytes()[0]);
            let g = hex_digit(hex.as_bytes()[1]);
            let b = hex_digit(hex.as_bytes()[2]);
            [r << 4 | r, g << 4 | g, b << 4 | b, 255]
        }
        // #RRGGBB
        6 => {
            let r = hex_byte(&hex[0..2]);
            let g = hex_byte(&hex[2..4]);
            let b = hex_byte(&hex[4..6]);
            [r, g, b, 255]
        }
        // #RRRRGGGGBBBB (16-bit per channel)
        12 => {
            // Take high byte of each 16-bit channel
            let r = hex_byte(&hex[0..2]);
            let g = hex_byte(&hex[4..6]);
            let b = hex_byte(&hex[8..10]);
            [r, g, b, 255]
        }
        _ => {
            tracing::debug!("XPM: unsupported hex color length {}: #{}", len, hex);
            [0, 0, 0, 255]
        }
    }
}

fn hex_digit(c: u8) -> u8 {
    match c {
        b'0'..=b'9' => c - b'0',
        b'a'..=b'f' => c - b'a' + 10,
        b'A'..=b'F' => c - b'A' + 10,
        _ => 0,
    }
}

fn hex_byte(s: &str) -> u8 {
    let bytes = s.as_bytes();
    if bytes.len() >= 2 {
        hex_digit(bytes[0]) << 4 | hex_digit(bytes[1])
    } else if bytes.len() == 1 {
        let d = hex_digit(bytes[0]);
        d << 4 | d
    } else {
        0
    }
}

#[cfg(test)]
#[path = "xpm_test.rs"]
mod tests;
