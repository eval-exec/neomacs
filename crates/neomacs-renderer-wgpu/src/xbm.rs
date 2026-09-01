//! Pure Rust XBM (X BitMap) decoder
//!
//! Parses XBM format image data and produces RGBA pixel buffers.
//! XBM is a monochrome bitmap format using C source code syntax.

/// Decode XBM image from in-memory data, returning (width, height, rgba_pixels).
///
/// `fg` and `bg` are RGBA colors for set (1) and unset (0) bits respectively.
pub fn decode_xbm_data(data: &[u8], fg: [u8; 4], bg: [u8; 4]) -> Option<(u32, u32, Vec<u8>)> {
    let text = std::str::from_utf8(data).ok()?;
    let (width, height, bits) = parse_xbm(text)?;
    let rgba = render_xbm(width, height, &bits, fg, bg);
    constrain_and_return(width, height, rgba)
}

/// Decode XBM image from a file path.
pub fn decode_xbm_file(
    path: &std::path::Path,
    fg: [u8; 4],
    bg: [u8; 4],
) -> Option<(u32, u32, Vec<u8>)> {
    let data = std::fs::read(path).ok()?;
    decode_xbm_data(&data, fg, bg)
}

/// Query XBM dimensions without full decode (header only).
pub fn query_xbm_dimensions(data: &[u8]) -> Option<(u32, u32)> {
    let text = std::str::from_utf8(data).ok()?;
    let (w, h) = parse_xbm_dimensions(text)?;
    Some((w, h))
}

/// Parse `#define name_width N` and `#define name_height N` from XBM text.
fn parse_xbm_dimensions(text: &str) -> Option<(u32, u32)> {
    let mut width: Option<u32> = None;
    let mut height: Option<u32> = None;

    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("#define ") {
            let parts: Vec<&str> = rest.split_whitespace().collect();
            if parts.len() == 2 {
                if parts[0].ends_with("_width") {
                    width = parts[1].parse().ok();
                } else if parts[0].ends_with("_height") {
                    height = parts[1].parse().ok();
                }
            }
        }
        if width.is_some() && height.is_some() {
            break;
        }
    }

    match (width, height) {
        (Some(w), Some(h)) if w > 0 && h > 0 => Some((w, h)),
        _ => None,
    }
}

/// Parse full XBM: dimensions + bit data.
fn parse_xbm(text: &str) -> Option<(u32, u32, Vec<u8>)> {
    let (width, height) = parse_xbm_dimensions(text)?;

    // Find the data array: look for `{ ... };`
    let brace_start = text.find('{')?;
    let brace_end = text[brace_start..].find('}')? + brace_start;
    let data_str = &text[brace_start + 1..brace_end];

    // Parse hex/decimal values
    let mut bytes = Vec::new();
    for token in data_str.split(',') {
        let token = token.trim();
        if token.is_empty() {
            continue;
        }
        if let Some(val) = parse_c_integer(token) {
            bytes.push(val as u8);
        }
    }

    // Validate: need at least (width+7)/8 * height bytes
    let bytes_per_line = width.div_ceil(8) as usize;
    let expected = bytes_per_line * height as usize;
    if bytes.len() < expected {
        tracing::warn!(
            "XBM: expected {} bytes for {}x{}, got {}",
            expected,
            width,
            height,
            bytes.len()
        );
        return None;
    }

    Some((width, height, bytes))
}

/// Parse a C integer literal (hex 0xNN or decimal).
fn parse_c_integer(s: &str) -> Option<u32> {
    let s = s.trim();
    // Strip trailing comments or other junk
    let s = s.trim_end_matches(|c: char| !c.is_ascii_hexdigit() && c != 'x' && c != 'X');
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u32::from_str_radix(hex, 16).ok()
    } else {
        s.parse().ok()
    }
}

/// Render XBM bit data to RGBA pixels.
/// XBM format: 1 bit per pixel, LSB first within each byte.
/// Set bits (1) get `fg` color, unset bits (0) get `bg` color.
fn render_xbm(width: u32, height: u32, bits: &[u8], fg: [u8; 4], bg: [u8; 4]) -> Vec<u8> {
    let w = width as usize;
    let h = height as usize;
    let bytes_per_line = w.div_ceil(8);
    let mut rgba = vec![0u8; w * h * 4];

    for y in 0..h {
        for x in 0..w {
            let byte_idx = y * bytes_per_line + x / 8;
            let bit_idx = x % 8;
            let bit_set = (bits[byte_idx] >> bit_idx) & 1 != 0;
            let color = if bit_set { &fg } else { &bg };
            let idx = (y * w + x) * 4;
            rgba[idx] = color[0];
            rgba[idx + 1] = color[1];
            rgba[idx + 2] = color[2];
            rgba[idx + 3] = color[3];
        }
    }

    rgba
}

/// XBM decodes at its native size; `ImageSizeSpec::desired` sizes it later.
fn constrain_and_return(width: u32, height: u32, rgba: Vec<u8>) -> Option<(u32, u32, Vec<u8>)> {
    Some((width, height, rgba))
}

#[cfg(test)]
#[path = "xbm_test.rs"]
mod tests;
