//! Paint completion is independent of whether the resulting pixels hold ink.
use super::super::test_support::{asset, rasterize_at_result};
use super::*;

#[test]
fn transparent_colr_paint_is_successful_with_positive_area() {
    let base = glyph('A');
    let font = font_with_color_tables(
        colr_v1_table(base, &paint_glyph(glyph(BLOCK), &paint_solid(0, 0.0))),
        cpal_table(&[RED]),
    );
    let raster = rasterize_at_result(&font, "transparent-colr", base, [0, 0, 0, 255], 48.0)
        .unwrap()
        .expect("a color record painted successfully");
    assert!(raster.width > 0 && raster.height > 0);
    assert_eq!(
        raster.rgba.len(),
        raster.width as usize * raster.height as usize * 4
    );
    assert!(raster.rgba.chunks_exact(4).all(|pixel| pixel[3] == 0));
}

#[test]
fn valid_no_paint_cases_are_successful_zero_area_rasters() {
    let base = glyph('A');
    for (key, paint) in [
        (
            "empty-clip",
            paint_glyph(glyph(' '), &paint_glyph(glyph(BLOCK), &paint_solid(0, 1.0))),
        ),
        (
            "empty-color-line",
            paint_glyph(
                glyph(BLOCK),
                &paint_linear_gradient(0, 0, 1000, 0, 0, 1000, &[]),
            ),
        ),
        (
            "identical-radial",
            paint_glyph(glyph(BLOCK), &repeat_radial(100, 100)),
        ),
        (
            "composite-clear",
            paint_composite(&paint_solid(0, 1.0), 0, &paint_solid(0, 1.0)),
        ),
    ] {
        let font = font_with_color_tables(colr_v1_table(base, &paint), cpal_table(&[RED, BLUE]));
        let raster = rasterize_at_result(&font, key, base, [0, 0, 0, 255], 48.0)
            .unwrap()
            .expect("successful no-paint must not become absent");
        assert_eq!((raster.width, raster.height), (0, 0), "{key}");
        assert!(raster.rgba.is_empty(), "{key}");
    }
}

#[test]
fn claimed_colr_layer_with_invalid_palette_reference_is_a_failure() {
    let base = glyph('A');
    // Version 0 propagates a bad palette reference from the parser. Version 1
    // intentionally skips unknown or unreadable paint subgraphs instead.
    let font = font_with_color_tables(
        colr_v0_table(base, &[(glyph(BLOCK), 1)]),
        cpal_table(&[RED]),
    );
    assert!(
        ttf_parser::Face::parse(&font, 0)
            .unwrap()
            .is_color_glyph(ttf_parser::GlyphId(base))
    );
    assert_eq!(
        rasterize_at_result(&font, "invalid-claimed-colr", base, [0, 0, 0, 255], 48.0),
        Err(ColorGlyphError::Paint {
            glyph_id: base,
            paint_source: ColorGlyphSource::Colr,
            reason: ColorPaintFailure::Malformed
        }),
    );
}

#[test]
fn unrecognized_colr_paint_is_a_successful_no_ink_glyph() {
    let base = glyph('A');
    let font = font_with_color_tables(colr_v1_table(base, &[255]), cpal_table(&[RED]));
    assert_eq!(layered_color_glyph_claim(&font, 0, base), Some(true));
    let raster = rasterize_at_result(&font, "unknown-paint-colr", base, [0, 0, 0, 255], 48.0)
        .expect("unrecognized paint subgraphs are skipped")
        .expect("a claimed glyph remains a successful paint");
    assert_eq!(raster.source, ColorGlyphSource::Colr);
    assert_eq!(
        raster.rgba.len(),
        raster.width as usize * raster.height as usize * 4
    );
    assert!(raster.rgba.chunks_exact(4).all(|pixel| pixel[3] == 0));
}

#[test]
fn skipped_radial_fill_is_a_successful_empty_raster() {
    let base = glyph('A');
    // Repeat's color-line start at -1 extends the cone to a negative radius.
    // tiny-skia returns no shader for that fill. Preserve the existing
    // paint-nothing behavior rather than failing the entire glyph.
    let font = font_with_color_tables(
        colr_v1_table(base, &paint_glyph(glyph(BLOCK), &repeat_radial(0, 100))),
        cpal_table(&[RED, BLUE]),
    );
    let raster = rasterize_at_result(&font, "skipped-radial", base, [0, 0, 0, 255], 48.0)
        .expect("a skipped fill is not a glyph failure")
        .expect("the color record is present");
    assert_eq!(
        raster.rgba.len(),
        raster.width as usize * raster.height as usize * 4
    );
    assert!(raster.rgba.chunks_exact(4).all(|pixel| pixel[3] == 0));
}

#[test]
fn skipped_radial_fill_preserves_other_visible_layers() {
    let base = glyph('A');
    let solid = paint_glyph(glyph(BLOCK), &paint_solid(0, 1.0));
    let palette = cpal_table(&[RED, BLUE]);
    let reference_font = font_with_color_tables(colr_v1_table(base, &solid), palette.clone());
    let reference = rasterize_at_result(
        &reference_font,
        "solid-reference",
        base,
        [0, 0, 0, 255],
        48.0,
    )
    .unwrap()
    .expect("the reference layer paints");
    assert!(reference.rgba.chunks_exact(4).any(|pixel| pixel[3] != 0));
    let font = font_with_color_tables(
        colr_v1_table_with_layers(
            base,
            &paint_colr_layers(0, 2),
            &[solid, paint_glyph(glyph(BLOCK), &repeat_radial(0, 100))],
            None,
        ),
        palette,
    );
    let raster = rasterize_at_result(
        &font,
        "solid-with-skipped-radial",
        base,
        [0, 0, 0, 255],
        48.0,
    )
    .expect("one skipped fill must not discard other layers")
    .expect("the color record is present");
    assert_eq!(raster, reference);
}

fn repeat_radial(start_radius: u16, end_radius: u16) -> Vec<u8> {
    let mut paint = vec![6]; // PaintRadialGradient, followed by a ColorLine.
    paint.extend_from_slice(&offset24(16));
    for value in [0, 0, start_radius, 0, 0, end_radius] {
        paint.extend_from_slice(&value.to_be_bytes());
    }
    paint.push(1); // Repeat.
    paint.extend_from_slice(&2u16.to_be_bytes());
    for (offset, palette) in [(-1.0, 0u16), (1.0, 1u16)] {
        paint.extend_from_slice(&f2dot14(offset).to_be_bytes());
        paint.extend_from_slice(&palette.to_be_bytes());
        paint.extend_from_slice(&f2dot14(1.0).to_be_bytes());
    }
    paint
}

#[test]
fn unclaimed_glyph_in_a_color_face_remains_absent() {
    let base = glyph('A');
    let font = font_with_color_tables(
        colr_v1_table(base, &paint_glyph(glyph(BLOCK), &paint_solid(0, 1.0))),
        cpal_table(&[RED]),
    );
    assert_eq!(layered_color_glyph_claim(&font, 0, glyph('B')), Some(false));
    assert_eq!(
        rasterize_at_result(&font, "unclaimed-colr", glyph('B'), [0, 0, 0, 255], 48.0),
        Ok(None)
    );
}

#[test]
fn available_backing_certifies_claim_per_glyph_without_a_durable_path() {
    let mono = super::super::test_support::outline_fixture();
    assert_eq!(layered_color_glyph_claim(&mono, 0, glyph(' ')), Some(false));
    let color = std::fs::read(neomacs_test_fonts::noto_color_emoji_colrv1()).unwrap();
    let grape = ttf_parser::Face::parse(&color, 0)
        .unwrap()
        .glyph_index('\u{1f347}')
        .unwrap();
    assert_eq!(layered_color_glyph_claim(&color, 0, grape.0), Some(true));
    assert_eq!(layered_color_glyph_claim(&[], 0, grape.0), None);
}

#[test]
fn unparsed_layered_tables_cannot_certify_absence() {
    let base = glyph('A');
    let colr = colr_v1_table(base, &paint_glyph(glyph(BLOCK), &paint_solid(0, 1.0)));
    for (key, tag, tables) in [
        (
            "malformed-colr",
            b"COLR",
            vec![(b"COLR", vec![0]), (b"CPAL", cpal_table(&[RED]))],
        ),
        ("missing-cpal", b"COLR", vec![(b"COLR", colr.clone())]),
        (
            "malformed-cpal",
            b"COLR",
            vec![(b"COLR", colr), (b"CPAL", vec![0])],
        ),
        ("malformed-svg", b"SVG ", vec![(b"SVG ", vec![0])]),
    ] {
        let font = fixture_with_tables(&tables);
        let face = ttf_parser::Face::parse(&font, 0).expect("outline face remains parseable");
        assert!(
            face.raw_face()
                .table(ttf_parser::Tag::from_bytes(tag))
                .is_some(),
            "{key}"
        );
        assert!(
            if tag == b"COLR" {
                face.tables().colr.is_none()
            } else {
                face.tables().svg.is_none()
            },
            "{key}"
        );
        assert_eq!(layered_color_glyph_claim(&font, 0, base), None, "{key}");
    }
}

#[test]
fn color_source_read_failure_is_retained_until_generation_clear() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("color.ttf");
    let file =
        neomacs_display_protocol::font::FontFileAsset::new(path.to_str().unwrap(), 0).unwrap();
    let source = FontOutlineAsset::File(file);
    let base = glyph('A');
    let request = ColorGlyphRequest {
        glyph_id: base,
        ..ColorGlyphRequest::default()
    };
    let mut painter = ColorGlyphRasterizer::new();
    let first = painter.rasterize(&source, &request);
    assert!(matches!(&first, Err(ColorGlyphError::Read { .. })));
    let font = font_with_color_tables(
        colr_v1_table(base, &paint_glyph(glyph(BLOCK), &paint_solid(0, 1.0))),
        cpal_table(&[RED]),
    );
    std::fs::write(&path, font).unwrap();
    assert_eq!(
        painter.rasterize(&source, &request),
        first,
        "cached errors cannot become absence"
    );
    painter.clear();
    assert!(painter.rasterize(&source, &request).unwrap().is_some());
    // The no-color result itself stays distinct from an unavailable file.
    let mono = asset(
        super::super::test_support::outline_fixture(),
        "plain-outline",
    );
    assert_eq!(painter.rasterize(&mono, &request), Ok(None));
}
