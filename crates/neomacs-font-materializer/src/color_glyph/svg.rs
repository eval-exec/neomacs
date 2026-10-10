//! OpenType `SVG ` table glyph rendering (`usvg` parse, `resvg` rasterize).
//!
//! Spec rules implemented here rather than delegated:
//!
//! * The document's coordinate system is the design grid: y-down, origin at
//!   the baseline, one unit per design unit.  The initial viewport is the em
//!   square, and it must not clip.
//! * Only the element with `id` = glyph-plus-id describes the glyph; the rest
//!   of the document exists for referenceable definitions.
//! * Documents may be gzip-compressed (SVGZ), detected by its magic bytes.
//! * `currentColor` resolves to the text foreground, and a `var(--colorN,
//!   fallback)` paint resolves to the CPAL entry or to the fallback.

use super::colr::{MAX_PIXELS, MAX_SIDE};
use super::{ColorGlyphRaster, ColorGlyphRequest};
use crate::raster_sources::ColorGlyphSource;
use resvg::{tiny_skia, usvg};
use std::io::Read as _;
use ttf_parser::{Face, GlyphId};

/// Rasterize one glyph from the face's `SVG ` table, when the table has a
/// document covering the glyph.
pub(super) fn paint_svg_glyph(
    face: &Face<'_>,
    request: &ColorGlyphRequest<'_>,
) -> Option<ColorGlyphRaster> {
    let table = face.tables().svg?;
    let document = table.documents.find(GlyphId(request.glyph_id))?;
    let data = decompressed(document.data)?;
    // Spec: the (uncompressed) document must be UTF-8.
    let text = std::str::from_utf8(&data).ok()?;
    let prepared = prepare_document(text, face, request);
    // Only the glyph's own element is its description; the rest of the
    // document exists so it can be referenced.  Pruning before parsing keeps
    // the document's own coordinate context (a root viewBox, wrapper group
    // transforms) while dropping sibling glyph descriptions.
    let pruned = pruned_document(&prepared, request.glyph_id)?;

    let options = usvg::Options::default();
    let tree = usvg::Tree::from_data(pruned.as_bytes(), &options).ok()?;
    let node = find_glyph_node(tree.root(), request.glyph_id)?;

    let units_per_em = f32::from(face.units_per_em());
    if !units_per_em.is_finite() || units_per_em <= 0.0 {
        return None;
    }
    let scale = request.px_size / units_per_em;
    if !scale.is_finite() || scale <= 0.0 {
        return None;
    }

    // Design units are the document's own space (y-down from the baseline),
    // so the device transform scales and offsets without a y flip.
    let bounds = node.abs_layer_bounding_box()?;
    let left = (bounds.x() * scale + request.offset.0).floor();
    let top = (bounds.y() * scale + request.offset.1).floor();
    let right = ((bounds.x() + bounds.width()) * scale + request.offset.0).ceil();
    let bottom = ((bounds.y() + bounds.height()) * scale + request.offset.1).ceil();
    let width = right - left;
    let height = bottom - top;
    let cap = MAX_SIDE as f32;
    if !(1.0..=cap).contains(&width) || !(1.0..=cap).contains(&height) {
        return None;
    }
    let width_cells = width as u32;
    let height_cells = height as u32;
    if u64::from(width_cells) * u64::from(height_cells) > MAX_PIXELS {
        return None;
    }

    let mut pixmap = tiny_skia::Pixmap::new(width_cells, height_cells)?;
    // The pruned tree renders only the glyph description, in the document's
    // own coordinate system: the root transform is the device mapping with
    // the pixmap window folded in.
    let root =
        tiny_skia::Transform::from_translate(request.offset.0 - left, request.offset.1 - top)
            .pre_concat(tiny_skia::Transform::from_scale(scale, scale));
    resvg::render(&tree, root, &mut pixmap.as_mut());

    let rgba = pixmap.take_demultiplied();
    let expected = width_cells as usize * height_cells as usize * 4;
    if rgba.len() != expected {
        return None;
    }
    Some(ColorGlyphRaster {
        left: left as i32,
        top: -(top as i32),
        width: width_cells,
        height: height_cells,
        rgba,
        source: ColorGlyphSource::SvgDocuments,
    })
}

/// Decompressed-document ceiling.  Real documents stay far below it — the
/// pinned Noto fixture's largest decodes to 14 MB — so it only stops a
/// crafted stream from expanding without bound.
const MAX_SVG_DOCUMENT_BYTES: usize = 64 * 1024 * 1024;

/// Spec: documents may be gzip-encoded; the header starts `1F 8B 08`.
fn decompressed(data: &[u8]) -> Option<Vec<u8>> {
    if data.starts_with(&[0x1f, 0x8b, 0x08]) {
        bounded_decompress(data, MAX_SVG_DOCUMENT_BYTES)
    } else {
        Some(data.to_vec())
    }
}

/// Inflate `data`, failing when the decoded size exceeds `limit`.
pub(super) fn bounded_decompress(data: &[u8], limit: usize) -> Option<Vec<u8>> {
    let mut decoded = Vec::new();
    flate2::read::GzDecoder::new(data)
        .take(limit as u64 + 1)
        .read_to_end(&mut decoded)
        .ok()?;
    (decoded.len() <= limit).then_some(decoded)
}

/// The element `id` for one glyph, per the spec's `glyph<glyphID>` rule.
fn find_glyph_node(group: &usvg::Group, glyph_id: u16) -> Option<&usvg::Node> {
    let wanted = format!("glyph{glyph_id}");
    for node in group.children() {
        if node.id() == wanted {
            return Some(node);
        }
        if let usvg::Node::Group(child) = node
            && let Some(found) = find_glyph_node(child, glyph_id)
        {
            return Some(found);
        }
    }
    None
}

/// Drop every element that neither contains the glyph's element nor defines
/// something referenceable, keeping the document's coordinate context.
///
/// `None` when the document has no element for the glyph.
fn pruned_document(text: &str, glyph_id: u16) -> Option<String> {
    let document = usvg::roxmltree::Document::parse(text).ok()?;
    let wanted = format!("glyph{glyph_id}");
    let target = document
        .descendants()
        .find(|node| node.is_element() && node.attribute("id") == Some(wanted.as_str()))?;

    // Rebuild from the root down to the glyph's element, keeping every
    // sibling that is not another glyph description.  Definitions may live
    // outside `defs` and still be referenced, so a renderable sibling is
    // wrapped in one instead of deleted: its artwork must not paint, but
    // references into it must resolve.
    let mut edits: Vec<(std::ops::Range<usize>, String)> = Vec::new();
    let mut current = target;
    while let Some(parent) = current.parent_element() {
        for child in parent.children().filter(|child| child.is_element()) {
            if child == current {
                continue;
            }
            if is_glyph_description(&child) {
                edits.push((child.range(), String::new()));
            } else if GRAPHIC_ELEMENTS.contains(&child.tag_name().name()) {
                let body = &text[child.range()];
                edits.push((child.range(), format!("<defs>{body}</defs>")));
            }
        }
        current = parent;
    }
    edits.sort_by_key(|(range, _)| range.start);

    let mut out = String::with_capacity(text.len());
    let mut cursor = 0;
    for (range, replacement) in edits {
        out.push_str(&text[cursor..range.start]);
        out.push_str(&replacement);
        cursor = range.end;
    }
    out.push_str(&text[cursor..]);
    Some(out)
}

/// Element names usvg renders; keeping one outside a `defs` would paint
/// sibling artwork the requested glyph never asked for.
const GRAPHIC_ELEMENTS: &[&str] = &[
    "a", "circle", "ellipse", "g", "image", "line", "path", "polygon", "polyline", "rect", "svg",
    "switch", "text", "use",
];

/// Whether this element is another `glyph<id>` description — one that must
/// not render when a different glyph of the same document was requested.  The
/// requested glyph itself is on the ancestor chain, never a sibling.
fn is_glyph_description(node: &usvg::roxmltree::Node<'_, '_>) -> bool {
    node.attribute("id").is_some_and(|id| {
        id.strip_prefix("glyph")
            .is_some_and(|digits| !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
    })
}

/// Rewrite the document text for the two host-environment hooks the spec
/// defines: `currentColor` and CPAL palette variables.
fn prepare_document(text: &str, face: &Face<'_>, request: &ColorGlyphRequest<'_>) -> String {
    let substituted = substitute_palette_variables(text, face, request);
    inject_foreground(&substituted, request.foreground)
}

/// Set the SVG `color` property on the root, which usvg resolves
/// `currentColor` from (defaulting to black).
fn inject_foreground(text: &str, foreground: [u8; 4]) -> String {
    let Some(svg_start) = text.find("<svg") else {
        return text.to_owned();
    };
    let Some(tag_end) = text[svg_start..].find('>').map(|i| svg_start + i) else {
        return text.to_owned();
    };
    let start_tag = &text[svg_start..tag_end];
    // Spec: a document that sets the color property explicitly overrides the
    // host's currentColor, and a second attribute of the same name would not
    // even be XML.
    if has_attribute(start_tag, "color") {
        return text.to_owned();
    }
    let [red, green, blue, alpha] = foreground;
    let property = if alpha == 0xff {
        format!("color=\"#{red:02x}{green:02x}{blue:02x}\"")
    } else {
        format!("color=\"#{red:02x}{green:02x}{blue:02x}{alpha:02x}\"")
    };
    let mut out = String::with_capacity(text.len() + property.len() + 1);
    out.push_str(&text[..tag_end]);
    out.push(' ');
    out.push_str(&property);
    out.push_str(&text[tag_end..]);
    out
}

/// Whether a start tag already carries the attribute `name`, matched at a
/// name boundary so `color-interpolation` is not a `color`.
fn has_attribute(start_tag: &str, name: &str) -> bool {
    let mut rest = start_tag;
    while let Some(index) = rest.find(name) {
        let before_ok = rest[..index]
            .chars()
            .next_back()
            .is_none_or(|c| c.is_whitespace());
        let after = &rest[index + name.len()..];
        if before_ok && after.trim_start().starts_with('=') {
            return true;
        }
        rest = &after[..];
    }
    false
}

/// Replace `var(--colorN, fallback)` with the CPAL entry, or with the
/// fallback when the name is not a palette entry or there is no CPAL table.
fn substitute_palette_variables(
    text: &str,
    face: &Face<'_>,
    request: &ColorGlyphRequest<'_>,
) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("var(") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 4..];
        let Some(end) = matching_paren(after) else {
            // No closing parenthesis: leave the text alone.
            out.push_str(&rest[start..]);
            return out;
        };
        let arguments = &after[..end];
        match resolve_palette_variable(arguments, face, request) {
            Some(color) => out.push_str(&color),
            None => {
                out.push_str("var(");
                out.push_str(arguments);
                out.push(')');
            }
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

/// The index of the `)` closing an already-opened `var(`, ignoring nested
/// parentheses in a fallback value such as `rgb(0, 0, 255)`.
fn matching_paren(after_open: &str) -> Option<usize> {
    let mut depth = 0usize;
    for (index, byte) in after_open.bytes().enumerate() {
        match byte {
            b'(' => depth += 1,
            b')' => {
                if depth == 0 {
                    return Some(index);
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    None
}

/// Resolve `--colorN[, fallback]` against the CPAL table.
fn resolve_palette_variable(
    arguments: &str,
    face: &Face<'_>,
    request: &ColorGlyphRequest<'_>,
) -> Option<String> {
    let (name, fallback) = match arguments.split_once(',') {
        Some((name, fallback)) => (name.trim(), Some(fallback.trim())),
        None => (arguments.trim(), None),
    };
    if let Some(entry) = name
        .strip_prefix("--color")
        .and_then(|number| number.parse::<u16>().ok())
        && let Some(color) = face
            .raw_face()
            .table(ttf_parser::Tag::from_bytes(b"CPAL"))
            .and_then(ttf_parser::cpal::Table::parse)
            .and_then(|palettes| palettes.get(request.palette, entry))
    {
        let ttf_parser::RgbaColor {
            red,
            green,
            blue,
            alpha,
        } = color;
        return Some(if alpha == 0xff {
            format!("#{red:02x}{green:02x}{blue:02x}")
        } else {
            format!("#{red:02x}{green:02x}{blue:02x}{alpha:02x}")
        });
    }
    fallback.map(str::to_owned)
}
