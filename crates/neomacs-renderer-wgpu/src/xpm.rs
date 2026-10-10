//! Pure Rust XPM (X PixMap) decoder
//!
//! Parses XPM2 and XPM3 format image data and produces RGBA pixel buffers.
//!
//! Color handling mirrors GNU `xpm_load_image` (src/image.c:6327-6591): each
//! color line's keys are walked in GNU's order, a value runs until the next key
//! token (so `light blue` is one name), and the value is resolved through the
//! same X11 `rgb.txt` database face colors come from
//! ([`neomacs_display_protocol::x11_color_value`]). `None` is transparency, and
//! a value that resolves to no color leaves its key out of the table, so those
//! pixels take the frame's foreground pixel (`fallback`) exactly as GNU paints
//! them — never a substitute like black.

use std::collections::HashMap;
use std::path::Path;

/// Decode XPM image from in-memory data, returning (width, height, rgba_pixels).
///
/// `fallback` is what GNU paints a pixel whose color key has no resolvable
/// color with: `FRAME_FOREGROUND_PIXEL` read once at load (src/image.c:6518,
/// 6537-6538) -- the frame's `foreground-color` parameter, which GNU keeps
/// equal to the `default` face's foreground (src/xfaces.c:4394-4404). Callers
/// pass `ImageColorContext::frame_foreground`, which is that value; the face
/// the image is displayed under and the specification's `:foreground` are not
/// consulted, as in GNU. GNU's fallback is opaque, so no alpha travels.
pub fn decode_xpm_data(data: &[u8], fallback: [u8; 3]) -> Option<(u32, u32, Vec<u8>)> {
    let strings = extract_strings(data)?;
    decode_from_strings(&strings, fallback)
}

/// Decode XPM image from a file path. See [`decode_xpm_data`] for `fallback`.
pub fn decode_xpm_file(path: &Path, fallback: [u8; 3]) -> Option<(u32, u32, Vec<u8>)> {
    let data = std::fs::read(path).ok()?;
    decode_xpm_data(&data, fallback)
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

fn decode_from_strings(strings: &[&[u8]], fallback: [u8; 3]) -> Option<(u32, u32, Vec<u8>)> {
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

    // Parse color table. GNU fails the whole image for a malformed line
    // (`goto failure`, src/image.c:6440-6516) but keeps walking for a line
    // whose color merely does not resolve: that key stays out of the table.
    let mut colors: HashMap<Vec<u8>, XpmColorValue> =
        HashMap::with_capacity(header.ncolors as usize);
    for i in 0..header.ncolors as usize {
        let line = strings[1 + i];
        // `len <= chars_per_pixel` is GNU's failure test: a color line must
        // carry definitions after its pixel key (src/image.c:6448).
        if line.len() <= cpp {
            tracing::warn!("XPM: color line {} has no definitions", i);
            return None;
        }
        let key = line[..cpp].to_vec();
        match parse_color_line(&line[cpp..])? {
            ColorLine::Defined(color) => {
                colors.insert(key, color);
            }
            ColorLine::Unresolved => tracing::debug!(
                "XPM: color line {:?} defines no resolvable color; \
                 pixels using it take the fallback",
                String::from_utf8_lossy(&line[cpp..])
            ),
        }
    }

    // Parse pixel data
    let w = header.width as usize;
    let h = header.height as usize;
    let mut rgba = vec![0u8; w * h * 4];
    let fallback = XpmColorValue::Rgb(fallback);

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
            let color = colors.get(pixel_key).copied().unwrap_or(fallback).rgba8();
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

/// GNU's XPM color keys (src/image.c:6288-6296), in GNU's priority order: a
/// line may carry several, and a color display uses the highest one present
/// (`best_key` is `c` whenever the display has color, src/image.c:6416-6417),
/// the first occurrence of it winning.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum XpmColorKey {
    /// `s`: the symbolic name looked up in `:color-symbols`. Recognized so a
    /// value walk ends at it — GNU's `max_key` starts above it, so it never
    /// contributes a pixel color, and neither does neomacs's unimplemented
    /// symbol table.
    Symbol,
    /// `m`: monochrome.
    Mono,
    /// `g4`: four gray levels.
    Gray4,
    /// `g`: grayscale.
    Gray,
    /// `c`: color.
    Color,
}

impl XpmColorKey {
    /// GNU compares keys with `strcmp` (`xpm_str_to_color_key`,
    /// src/image.c:6298): case matters.
    fn from_token(token: &[u8]) -> Option<Self> {
        match token {
            b"s" => Some(Self::Symbol),
            b"m" => Some(Self::Mono),
            b"g4" => Some(Self::Gray4),
            b"g" => Some(Self::Gray),
            b"c" => Some(Self::Color),
            _ => None,
        }
    }
}

/// One color a color line resolved to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum XpmColorValue {
    /// `None`: GNU records `Qt` and the pixel is masked out (src/image.c:6505).
    Transparent,
    /// A color the display hook resolved.
    Rgb([u8; 3]),
}

impl XpmColorValue {
    fn rgba8(self) -> [u8; 4] {
        match self {
            Self::Transparent => [0, 0, 0, 0],
            Self::Rgb([red, green, blue]) => [red, green, blue, 255],
        }
    }
}

/// What one color line contributes to the color table
/// (GNU src/image.c:6487-6513).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ColorLine {
    Defined(XpmColorValue),
    /// No key this display uses, or a value that resolved to nothing: GNU
    /// leaves the key out of the table, and pixels carrying it are painted with
    /// the frame's foreground (src/image.c:6537-6538).
    Unresolved,
}

/// Read one color line's `KEY VALUE [KEY VALUE ...]` tail — everything after
/// the pixel key — as GNU does (src/image.c:6453-6485).
///
/// A value runs until the next key token, which is what makes `c light blue`
/// one name instead of a name and a stray token. `None` means the line is
/// malformed, which fails the whole image in GNU too, rather than substituting
/// a color for it.
fn parse_color_line(rest: &[u8]) -> Option<ColorLine> {
    // GNU tokenizes with `strtok (buffer, " \t")`, so runs of blanks and tabs
    // separate and disappear.
    let mut tokens = rest
        .split(|byte| *byte == b' ' || *byte == b'\t')
        .filter(|token| !token.is_empty());

    let mut key = XpmColorKey::from_token(tokens.next()?)?;
    let mut chosen: Option<(XpmColorKey, Vec<u8>)> = None;

    loop {
        // GNU requires a value after every key (`color == NULL` fails).
        let mut value = tokens.next()?.to_vec();
        let mut next_key = None;
        for token in tokens.by_ref() {
            if let Some(found) = XpmColorKey::from_token(token) {
                next_key = Some(found);
                break;
            }
            // `color[strlen (color)] = ' '`: continue the value across the
            // blank `strtok` replaced.
            value.push(b' ');
            value.extend_from_slice(token);
        }

        // A strictly higher key replaces the choice, so equal keys keep the
        // first value (`max_key < key`, src/image.c:6478).
        if key != XpmColorKey::Symbol && chosen.as_ref().is_none_or(|(best, _)| *best < key) {
            chosen = Some((key, value));
        }

        match next_key {
            Some(found) => key = found,
            None => break,
        }
    }

    Some(match chosen {
        Some((_, value)) => match resolve_color_value(&value) {
            Some(color) => ColorLine::Defined(color),
            None => ColorLine::Unresolved,
        },
        None => ColorLine::Unresolved,
    })
}

/// Resolve one color value the way GNU hands it to the frame's
/// `defined_color_hook` (src/image.c:6505-6510): `None` is transparency,
/// everything else is an X11 database name or a numeric spec (`#`-hex,
/// `rgb:R/G/B`, `rgbi:R/G/B`).
///
/// `opaque` is deliberately not special-cased. The libXpm loader maps that name
/// to the frame foreground explicitly (src/image.c:5634-5641); the hand-written
/// loader this decoder mirrors leaves it unresolved, which paints the same
/// pixel (src/image.c:6512-6538). `etc/images/letter.xpm` ships that name and
/// is covered by the corpus cross-check beside these tests.
fn resolve_color_value(value: &[u8]) -> Option<XpmColorValue> {
    // GNU's `xstrcasecmp (max_color, "None")`.
    let value = std::str::from_utf8(value).ok()?;
    if value.eq_ignore_ascii_case("none") {
        return Some(XpmColorValue::Transparent);
    }
    neomacs_display_protocol::x11_color_value(value).map(|(r, g, b)| XpmColorValue::Rgb([r, g, b]))
}

#[cfg(test)]
#[path = "xpm/tests/xpm_test.rs"]
mod tests;

/// The independent-decoder cross-check lives beside the GNU-shaped tests:
/// GNU is the specification, so it is a smoke check rather than an oracle.
#[cfg(test)]
#[path = "xpm/tests/corpus_test.rs"]
mod corpus_tests;
