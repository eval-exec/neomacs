//! `SVG ` table behavior on synthetic documents, plus one real document from
//! the pinned COLRv1 emoji fixture.

use super::test_support::{fixture_with_tables, glyph, rasterize_at};
use super::*;
use std::io::Write as _;

/// A color that shows up in exactly one channel per test, so the probes can
/// name the source of a pixel unambiguously.
const INDIGO: [u8; 4] = [0x22, 0x00, 0xee, 0xff];
const GRASS: [u8; 4] = [0x22, 0xaa, 0x22, 0xff];

fn gzip_bytes(data: &[u8]) -> Vec<u8> {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(data).expect("gzip writer");
    encoder.finish().expect("gzip finishes")
}

/// One `SVG ` table with `documents` (start, end, data, gzip?).
fn svg_table(documents: &[(u16, u16, Vec<u8>, bool)]) -> Vec<u8> {
    let header = 10u32;
    let mut table = Vec::new();
    table.extend_from_slice(&0u16.to_be_bytes()); // version
    table.extend_from_slice(&header.to_be_bytes()); // svgDocumentListOffset
    table.extend_from_slice(&0u32.to_be_bytes()); // reserved
    table.extend_from_slice(&(documents.len() as u16).to_be_bytes());
    let mut offset = 2 + 12 * documents.len() as u32;
    let mut payloads = Vec::new();
    for (start, end, data, gzip) in documents {
        let payload = if *gzip {
            gzip_bytes(data)
        } else {
            data.clone()
        };
        table.extend_from_slice(&start.to_be_bytes());
        table.extend_from_slice(&end.to_be_bytes());
        table.extend_from_slice(&offset.to_be_bytes());
        table.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        offset += payload.len() as u32;
        payloads.push(payload);
    }
    for payload in payloads {
        table.extend_from_slice(&payload);
    }
    table
}

/// A minimal conforming document, with `body` inside the root.
fn document(body: &str) -> Vec<u8> {
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" \
         xmlns:xlink=\"http://www.w3.org/1999/xlink\" version=\"1.1\">{body}</svg>"
    )
    .into_bytes()
}

/// A document whose glyph element is a solid rectangle in the design grid,
/// y-down from the baseline: user y = -600..-100.
fn rect_document(glyph_id: u16, fill: &str) -> Vec<u8> {
    document(&format!(
        "<g id=\"glyph{glyph_id}\">\
         <rect x=\"100\" y=\"-600\" width=\"400\" height=\"500\" fill=\"{fill}\"/></g>"
    ))
}

/// One palette with `colors` in RGBA order.
fn cpal_table(colors: &[[u8; 4]]) -> Vec<u8> {
    let mut table = Vec::new();
    table.extend_from_slice(&0u16.to_be_bytes()); // version
    table.extend_from_slice(&(colors.len() as u16).to_be_bytes()); // numPaletteEntries
    table.extend_from_slice(&1u16.to_be_bytes()); // numPalettes
    table.extend_from_slice(&(colors.len() as u16).to_be_bytes()); // numColorRecords
    table.extend_from_slice(&14u32.to_be_bytes()); // colorRecordsArrayOffset
    table.extend_from_slice(&0u16.to_be_bytes()); // colorRecordIndices[0]
    for [red, green, blue, alpha] in colors {
        // CPAL color records are BGRA.
        table.extend_from_slice(&[*blue, *green, *red, *alpha]);
    }
    table
}

fn pixel(raster: &ColorGlyphRaster, x: u32, y: u32) -> [u8; 4] {
    let index = ((y * raster.width + x) * 4) as usize;
    raster.rgba[index..index + 4].try_into().expect("pixel")
}

fn ink(raster: &ColorGlyphRaster) -> usize {
    raster
        .rgba
        .chunks_exact(4)
        .filter(|pixel| pixel[3] > 0)
        .count()
}

#[test]
fn svg_glyph_paints_at_its_design_grid_position() {
    let base = glyph('\u{2588}');
    let font = fixture_with_tables(&[(
        b"SVG ",
        svg_table(&[(base, base, rect_document(base, "#2200ee"), false)]),
    )]);
    // unitsPerEm 1000 at 100 px: the rect covers device x 10..50, y -60..-10.
    let raster = rasterize_at(&font, "synthetic:svg-plain", base, [0, 0, 0, 255], 100.0)
        .expect("an SVG glyph paints");
    assert_eq!(raster.source, ColorGlyphSource::SvgDocuments);
    assert_eq!(
        (raster.left, raster.top, raster.width, raster.height),
        (10, 60, 40, 50)
    );
    assert_eq!(pixel(&raster, 20, 25), INDIGO);
}

#[test]
fn gzip_documents_render_identically() {
    let base = glyph('\u{2588}');
    let plain = fixture_with_tables(&[(
        b"SVG ",
        svg_table(&[(base, base, rect_document(base, "#2200ee"), false)]),
    )]);
    let gzipped = fixture_with_tables(&[(
        b"SVG ",
        svg_table(&[(base, base, rect_document(base, "#2200ee"), true)]),
    )]);
    let plain = rasterize_at(&plain, "synthetic:svg-plain2", base, [0, 0, 0, 255], 100.0)
        .expect("the plain document paints");
    let gzipped = rasterize_at(&gzipped, "synthetic:svg-gzip", base, [0, 0, 0, 255], 100.0)
        .expect("the gzip document paints");
    assert_eq!(
        (
            plain.left,
            plain.top,
            plain.width,
            plain.height,
            &plain.rgba
        ),
        (
            gzipped.left,
            gzipped.top,
            gzipped.width,
            gzipped.height,
            &gzipped.rgba
        )
    );
}

/// Spec examples 2 and 3: the same artwork, once in the y-negative quadrant
/// and once shifted by a root viewBox, must render identically.
#[test]
fn root_viewbox_shifts_the_glyph() {
    let base = glyph('\u{2588}');
    let direct = fixture_with_tables(&[(
        b"SVG ",
        svg_table(&[(base, base, rect_document(base, "#2200ee"), false)]),
    )]);
    let shifted = document(&format!(
        "<g id=\"glyph{base}\"><rect x=\"100\" y=\"400\" width=\"400\" height=\"500\" \
         fill=\"#2200ee\"/></g>"
    ));
    let shifted = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" version=\"1.1\" \
         viewBox=\"0 1000 1000 1000\">{}</svg>",
        String::from_utf8(shifted).expect("utf-8")
    )
    .into_bytes();
    let shifted = fixture_with_tables(&[(b"SVG ", svg_table(&[(base, base, shifted, false)]))]);
    let direct = rasterize_at(&direct, "synthetic:svg-direct", base, [0, 0, 0, 255], 100.0)
        .expect("the direct document paints");
    let shifted = rasterize_at(
        &shifted,
        "synthetic:svg-viewbox",
        base,
        [0, 0, 0, 255],
        100.0,
    )
    .expect("the viewBox document paints");
    assert_eq!(
        (
            direct.left,
            direct.top,
            direct.width,
            direct.height,
            &direct.rgba
        ),
        (
            shifted.left,
            shifted.top,
            shifted.width,
            shifted.height,
            &shifted.rgba
        )
    );
}

#[test]
fn shared_definitions_resolve_across_use() {
    let base = glyph('\u{2588}');
    let doc = document(&format!(
        "<defs><rect id=\"bar\" x=\"100\" y=\"-600\" width=\"400\" height=\"500\" \
         fill=\"#22aa22\"/></defs>\
         <g id=\"glyph{base}\"><use xlink:href=\"#bar\"/></g>"
    ));
    let font = fixture_with_tables(&[(b"SVG ", svg_table(&[(base, base, doc, false)]))]);
    let raster = rasterize_at(&font, "synthetic:svg-use", base, [0, 0, 0, 255], 100.0)
        .expect("a document using shared definitions paints");
    assert_eq!(pixel(&raster, 20, 25), GRASS);
}

#[test]
fn current_color_uses_the_text_foreground() {
    let base = glyph('\u{2588}');
    let font = fixture_with_tables(&[(
        b"SVG ",
        svg_table(&[(base, base, rect_document(base, "currentColor"), false)]),
    )]);
    let raster = rasterize_at(
        &font,
        "synthetic:svg-current",
        base,
        [200, 10, 120, 255],
        100.0,
    )
    .expect("currentColor paints");
    assert_eq!(pixel(&raster, 20, 25), [200, 10, 120, 255]);
}

#[test]
fn palette_variables_use_cpal_and_fall_back() {
    let base = glyph('\u{2588}');
    let doc = rect_document(base, "var(--color0, #00ff00)");
    let with_palette = fixture_with_tables(&[
        (b"SVG ", svg_table(&[(base, base, doc.clone(), false)])),
        (b"CPAL", cpal_table(&[[0x11, 0x22, 0x33, 0xff]])),
    ]);
    let raster = rasterize_at(
        &with_palette,
        "synthetic:svg-cpal",
        base,
        [0, 0, 0, 255],
        100.0,
    )
    .expect("a palette variable paints");
    assert_eq!(pixel(&raster, 20, 25), [0x11, 0x22, 0x33, 0xff]);

    let without_palette = fixture_with_tables(&[(b"SVG ", svg_table(&[(base, base, doc, false)]))]);
    let raster = rasterize_at(
        &without_palette,
        "synthetic:svg-no-cpal",
        base,
        [0, 0, 0, 255],
        100.0,
    )
    .expect("the fallback colour paints");
    assert_eq!(pixel(&raster, 20, 25), [0x00, 0xff, 0x00, 0xff]);
}

/// One document, two glyph descriptions: each glyph id renders its own
/// element out of the shared document.
#[test]
fn one_document_serves_each_glyph_in_its_range() {
    let (first, second) = (glyph('\u{2588}'), glyph('O'));
    let doc = document(&format!(
        "<g id=\"glyph{first}\"><rect x=\"100\" y=\"-600\" width=\"400\" height=\"500\" \
         fill=\"#2200ee\"/></g>\
         <g id=\"glyph{second}\"><rect x=\"100\" y=\"-600\" width=\"400\" height=\"500\" \
         fill=\"#22aa22\"/></g>"
    ));
    let font = fixture_with_tables(&[(
        b"SVG ",
        svg_table(&[(first.min(second), first.max(second), doc, false)]),
    )]);
    let first_raster = rasterize_at(&font, "synthetic:svg-two", first, [0, 0, 0, 255], 100.0)
        .expect("the first glyph paints");
    let second_raster = rasterize_at(&font, "synthetic:svg-two", second, [0, 0, 0, 255], 100.0)
        .expect("the second glyph paints");
    assert_eq!(pixel(&first_raster, 20, 25), INDIGO);
    assert_eq!(pixel(&second_raster, 20, 25), GRASS);
}

#[test]
fn a_document_without_the_glyph_element_paints_nothing() {
    let base = glyph('\u{2588}');
    let other = glyph('O');
    let font = fixture_with_tables(&[(
        b"SVG ",
        svg_table(&[(base, base, rect_document(other, "#2200ee"), false)]),
    )]);
    assert_eq!(
        rasterize_at(&font, "synthetic:svg-missing", base, [0, 0, 0, 255], 100.0),
        None,
        "a document without the glyph element is not a description for the glyph"
    );
}

/// The pinned COLRv1 emoji fixture carries real SVG documents (723 of them,
/// separate from its COLR graph); render one directly.
#[test]
fn real_svg_documents_render_from_the_pinned_fixture() {
    let bytes = std::fs::read(neomacs_test_fonts::noto_color_emoji_colrv1())
        .expect("read the COLRv1 fixture");
    let face = ttf_parser::Face::parse(&bytes, 0).expect("fixture parses");
    let svg = face.tables().svg.expect("the fixture has an SVG table");
    let document = svg
        .documents
        .find(ttf_parser::GlyphId(2))
        .expect("glyph 2 has an SVG document");
    assert!(!document.data.is_empty());

    let request = ColorGlyphRequest {
        glyph_id: 2,
        px_size: 64.0,
        ..ColorGlyphRequest::default()
    };
    let raster = super::svg::paint_svg_glyph(&face, &request).expect("the real document paints");
    assert!(
        ink(&raster) > 100,
        "the real document must paint real ink: {} pixels",
        ink(&raster)
    );
}

/// A crafted stream must not be able to expand without bound: the inflate
/// step refuses documents past its ceiling instead of allocating first.
#[test]
fn oversized_decompressed_documents_are_refused() {
    let payload = vec![b'x'; 4096];
    let compressed = gzip_bytes(&payload);
    assert!(
        super::svg::bounded_decompress(&compressed, 1024).is_none(),
        "a stream past the limit must be refused"
    );
    assert_eq!(
        super::svg::bounded_decompress(&compressed, 4096).as_deref(),
        Some(payload.as_slice()),
        "a stream at the limit decodes intact"
    );
}

/// Spec: a document that sets the color property itself overrides the host's
/// currentColor — and injecting a second color attribute would break the XML
/// outright.
#[test]
fn documents_with_their_own_root_color_keep_it() {
    let base = glyph('\u{2588}');
    let doc = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" version=\"1.1\" color=\"#123456\">\
         <g id=\"glyph{base}\"><rect x=\"100\" y=\"-600\" width=\"400\" height=\"500\" \
         fill=\"currentColor\"/></g></svg>"
    )
    .into_bytes();
    let font = fixture_with_tables(&[(b"SVG ", svg_table(&[(base, base, doc, false)]))]);
    let raster = rasterize_at(
        &font,
        "synthetic:svg-own-color",
        base,
        [200, 10, 120, 255],
        100.0,
    )
    .expect("the document's own color paints");
    assert_eq!(pixel(&raster, 20, 25), [0x12, 0x34, 0x56, 0xff]);
}

/// Definitions may live outside `defs` and still be referenced; pruning must
/// not take them away with the sibling that owns them.
#[test]
fn definitions_outside_defs_stay_reachable() {
    let base = glyph('\u{2588}');
    let gradient = "<linearGradient id=\"grad\" gradientUnits=\"userSpaceOnUse\" \
                    x1=\"100\" y1=\"-600\" x2=\"500\" y2=\"-600\">\
                    <stop offset=\"0\" stop-color=\"#2200ee\"/>\
                    <stop offset=\"1\" stop-color=\"#22aa22\"/></linearGradient>";
    let doc = document(&format!(
        "{gradient}<g id=\"glyph{base}\"><rect x=\"100\" y=\"-600\" width=\"400\" height=\"500\" \
         fill=\"url(#grad)\"/></g>"
    ));
    let font = fixture_with_tables(&[(b"SVG ", svg_table(&[(base, base, doc, false)]))]);
    let raster = rasterize_at(&font, "synthetic:svg-gradef", base, [0, 0, 0, 255], 100.0)
        .expect("a gradient defined outside defs still paints");
    let left = pixel(&raster, 2, 25);
    let right = pixel(&raster, 37, 25);
    assert!(
        left[2] > left[1] + 80 && left[3] > 200,
        "the gradient's first stop must show at the left: {left:?}"
    );
    assert!(
        right[1] > right[2] + 80 && right[3] > 200,
        "the gradient's last stop must show at the right: {right:?}"
    );
}

/// Only the requested glyph description paints: a stray renderable sibling
/// stays out of the rendered tree even though it is kept for references.
#[test]
fn stray_renderable_siblings_do_not_paint() {
    let base = glyph('\u{2588}');
    let stray = "<rect x=\"0\" y=\"-1000\" width=\"1000\" height=\"1000\" fill=\"#ff0000\"/>";
    let doc = document(&format!(
        "{stray}<g id=\"glyph{base}\"><rect x=\"100\" y=\"-600\" width=\"400\" height=\"500\" \
         fill=\"#2200ee\"/></g>"
    ));
    let font = fixture_with_tables(&[(b"SVG ", svg_table(&[(base, base, doc, false)]))]);
    let raster = rasterize_at(&font, "synthetic:svg-stray", base, [0, 0, 0, 255], 100.0)
        .expect("the glyph paints without the stray sibling");
    assert_eq!(
        (raster.left, raster.top, raster.width, raster.height),
        (10, 60, 40, 50),
        "the raster must be the glyph's own box, not the stray's"
    );
    assert_eq!(pixel(&raster, 20, 25), INDIGO);
}
