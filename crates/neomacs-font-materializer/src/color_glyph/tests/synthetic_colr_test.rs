//! COLR behavior tested on synthetic color tables added to a real outline
//! fixture.
//!
//! Real color fonts exercise one shape of the spec each; these cover the
//! rules that a font in the wild cannot be relied on to contain: version 0
//! layer records (which the unified painter must still serve), the
//! foreground palette index, palette alpha, a paint graph without any clip
//! box, an unbounded graph, and composite layers.

use super::*;
use crate::FontFileCache;
use neomacs_display_protocol::font::{FontMemoryAsset, FontOutlineAsset};
use std::sync::Arc;

#[derive(Clone, Copy, Debug)]
struct Rgba([u8; 4]);

const RED: Rgba = Rgba([220, 30, 30, 255]);
const GREEN: Rgba = Rgba([30, 200, 60, 255]);
const BLUE: Rgba = Rgba([40, 60, 230, 255]);
const YELLOW: Rgba = Rgba([240, 220, 40, 255]);

/// A filled rectangle: gradient sampling needs ink at every probe angle.
const BLOCK: char = '\u{2588}';

fn outline_fixture() -> Vec<u8> {
    std::fs::read(neomacs_test_fonts::mplus_1_code_thin()).expect("read outline fixture")
}

/// (tag, payload) for every table in `bytes`.
fn table_records(bytes: &[u8]) -> Vec<(u32, Vec<u8>)> {
    let count = u16::from_be_bytes([bytes[4], bytes[5]]) as usize;
    (0..count)
        .map(|index| {
            let record = &bytes[12 + index * 16..12 + index * 16 + 16];
            let tag: [u8; 4] = record[0..4].try_into().expect("table tag");
            let offset = u32::from_be_bytes(record[8..12].try_into().expect("offset")) as usize;
            let length = u32::from_be_bytes(record[12..16].try_into().expect("length")) as usize;
            (
                u32::from_be_bytes(tag),
                bytes[offset..offset + length].to_vec(),
            )
        })
        .collect()
}

/// Add synthetic color tables to the outline fixture.
fn font_with_color_tables(colr: Vec<u8>, cpal: Vec<u8>) -> Vec<u8> {
    let base = outline_fixture();
    let mut tables = table_records(&base);
    // The fixture's signature covers the tables being replaced.
    tables.retain(|(tag, _)| *tag != u32::from_be_bytes(*b"DSIG"));
    tables.push((u32::from_be_bytes(*b"COLR"), colr));
    tables.push((u32::from_be_bytes(*b"CPAL"), cpal));
    FontFileCache::standalone_sfnt_from_tables(tables).expect("serialize synthetic color font")
}

fn asset(bytes: Vec<u8>, key: &str) -> FontOutlineAsset {
    FontOutlineAsset::Memory(FontMemoryAsset::new(key, Arc::new(bytes), 0).expect("memory asset"))
}

/// The ink bounds of `c` in the outline fixture, in design units.
fn glyph_bounds(c: char) -> ttf_parser::Rect {
    let bytes = outline_fixture();
    let face = ttf_parser::Face::parse(&bytes, 0).expect("outline fixture parses");
    let glyph = face.glyph_index(c).expect("fixture maps the char");
    face.glyph_bounding_box(glyph)
        .expect("fixture glyph has ink")
}

/// A glyph id from the outline fixture, which every synthetic font reuses.
fn glyph(c: char) -> u16 {
    ttf_parser::Face::parse(&outline_fixture(), 0)
        .expect("outline fixture parses")
        .glyph_index(c)
        .expect("fixture maps the char")
        .0
}

fn rasterize(bytes: &[u8], key: &str, glyph: u16, foreground: [u8; 4]) -> Option<ColorGlyphRaster> {
    rasterize_at(bytes, key, glyph, foreground, 48.0)
}

fn rasterize_at(
    bytes: &[u8],
    key: &str,
    glyph: u16,
    foreground: [u8; 4],
    px_size: f32,
) -> Option<ColorGlyphRaster> {
    let asset = asset(bytes.to_vec(), key);
    let mut rasterizer = ColorGlyphRasterizer::new();
    rasterizer
        .rasterize(
            &asset,
            &ColorGlyphRequest {
                glyph_id: glyph,
                px_size,
                foreground,
                ..ColorGlyphRequest::default()
            },
        )
        .expect("synthetic source opens")
}

/// Mean color of pixels with any coverage, in straight alpha.
fn mean_color(raster: &ColorGlyphRaster) -> [f32; 3] {
    let mut sums = [0f64; 3];
    let mut weights = 0f64;
    for pixel in raster.rgba.chunks_exact(4) {
        let weight = f64::from(pixel[3]) / 255.0;
        for channel in 0..3 {
            sums[channel] += f64::from(pixel[channel]) * weight;
        }
        weights += weight;
    }
    assert!(weights > 0.0, "raster has no covered pixels");
    [
        (sums[0] / weights) as f32,
        (sums[1] / weights) as f32,
        (sums[2] / weights) as f32,
    ]
}

fn assert_close_to(mean: [f32; 3], expected: Rgba, tolerance: f32) {
    for channel in 0..3 {
        let delta = (mean[channel] - f32::from(expected.0[channel])).abs();
        assert!(
            delta <= tolerance,
            "channel {channel}: mean {mean:?} vs expected {:?}",
            expected.0
        );
    }
}

// --- table builders ------------------------------------------------------

fn f2dot14(value: f32) -> i16 {
    (f64::from(value) * 16384.0)
        .round()
        .clamp(-32768.0, 32767.0) as i16
}

fn offset24(value: u32) -> [u8; 3] {
    [
        ((value >> 16) & 0xFF) as u8,
        ((value >> 8) & 0xFF) as u8,
        (value & 0xFF) as u8,
    ]
}

/// Version 0 `CPAL` with one palette holding `colors` in RGBA order.
fn cpal_table(colors: &[Rgba]) -> Vec<u8> {
    let mut table = Vec::new();
    table.extend_from_slice(&0u16.to_be_bytes()); // version
    table.extend_from_slice(&(colors.len() as u16).to_be_bytes()); // numPaletteEntries
    table.extend_from_slice(&1u16.to_be_bytes()); // numPalettes
    table.extend_from_slice(&(colors.len() as u16).to_be_bytes()); // numColorRecords
    let header = 14u32;
    table.extend_from_slice(&header.to_be_bytes()); // colorRecordsArrayOffset
    table.extend_from_slice(&0u16.to_be_bytes()); // colorRecordIndices[0]
    for Rgba([red, green, blue, alpha]) in colors {
        // CPAL color records are BGRA.
        table.extend_from_slice(&[*blue, *green, *red, *alpha]);
    }
    table
}

/// Version 0 `COLR` with one base glyph. Layers are (glyph id, palette index).
fn colr_v0_table(base: u16, layers: &[(u16, u16)]) -> Vec<u8> {
    let base_records_offset = 14u32;
    let layer_records_offset = base_records_offset + 6;
    let mut table = Vec::new();
    table.extend_from_slice(&0u16.to_be_bytes()); // version
    table.extend_from_slice(&1u16.to_be_bytes()); // numBaseGlyphRecords
    table.extend_from_slice(&base_records_offset.to_be_bytes());
    table.extend_from_slice(&layer_records_offset.to_be_bytes());
    table.extend_from_slice(&(layers.len() as u16).to_be_bytes()); // numLayerRecords
    table.extend_from_slice(&base.to_be_bytes());
    table.extend_from_slice(&0u16.to_be_bytes()); // firstLayerIndex
    table.extend_from_slice(&(layers.len() as u16).to_be_bytes());
    for (glyph, palette) in layers {
        table.extend_from_slice(&glyph.to_be_bytes());
        table.extend_from_slice(&palette.to_be_bytes());
    }
    table
}

/// Version 1 `COLR` with one base glyph whose root paint table is `paint`.
fn colr_v1_table(base: u16, paint: &[u8]) -> Vec<u8> {
    let base_glyph_list_offset = 34u32;
    let list_header = 4 + 6; // count + one BaseGlyphPaintRecord
    let mut table = Vec::new();
    table.extend_from_slice(&1u16.to_be_bytes()); // version
    table.extend_from_slice(&0u16.to_be_bytes()); // numBaseGlyphRecords
    table.extend_from_slice(&0u32.to_be_bytes()); // baseGlyphRecordsOffset
    table.extend_from_slice(&0u32.to_be_bytes()); // layerRecordsOffset
    table.extend_from_slice(&0u16.to_be_bytes()); // numLayerRecords
    table.extend_from_slice(&base_glyph_list_offset.to_be_bytes());
    table.extend_from_slice(&0u32.to_be_bytes()); // layerListOffset
    table.extend_from_slice(&0u32.to_be_bytes()); // clipListOffset
    table.extend_from_slice(&0u32.to_be_bytes()); // varIndexMapOffset
    table.extend_from_slice(&0u32.to_be_bytes()); // itemVariationStoreOffset
    table.extend_from_slice(&1u32.to_be_bytes()); // numBaseGlyphPaintRecords
    table.extend_from_slice(&base.to_be_bytes());
    table.extend_from_slice(&(list_header as u32).to_be_bytes()); // paint, from the list start
    table.extend_from_slice(paint);
    table
}

fn paint_solid(palette_index: u16, alpha: f32) -> Vec<u8> {
    let mut paint = vec![2u8];
    paint.extend_from_slice(&palette_index.to_be_bytes());
    paint.extend_from_slice(&f2dot14(alpha).to_be_bytes());
    paint
}

/// Format 10: the glyph outline becomes the clip region for `child`.
fn paint_glyph(glyph: u16, child: &[u8]) -> Vec<u8> {
    const HEADER: u32 = 1 + 3 + 2;
    let mut paint = vec![10u8];
    paint.extend_from_slice(&offset24(HEADER));
    paint.extend_from_slice(&glyph.to_be_bytes());
    paint.extend_from_slice(child);
    paint
}

/// Format 4: a linear gradient along p0 -> p1, projected along p0p2.
fn paint_linear_gradient(
    x0: i16,
    y0: i16,
    x1: i16,
    y1: i16,
    x2: i16,
    y2: i16,
    stops: &[(f32, u16)],
) -> Vec<u8> {
    let mut color_line = vec![0u8]; // extend: pad
    color_line.extend_from_slice(&(stops.len() as u16).to_be_bytes());
    for (offset, palette_index) in stops {
        color_line.extend_from_slice(&f2dot14(*offset).to_be_bytes());
        color_line.extend_from_slice(&palette_index.to_be_bytes());
        color_line.extend_from_slice(&f2dot14(1.0).to_be_bytes());
    }
    const HEADER: u32 = 1 + 3 + 12;
    let mut paint = vec![4u8];
    paint.extend_from_slice(&offset24(HEADER));
    for value in [x0, y0, x1, y1, x2, y2] {
        paint.extend_from_slice(&value.to_be_bytes());
    }
    paint.extend_from_slice(&color_line);
    paint
}

/// Format 8: a sweep gradient around `center`, 0 degrees pointing +x in the
/// design grid, measured counter-clockwise as the spec defines it.
fn paint_sweep_gradient(
    center_x: i16,
    center_y: i16,
    start: f32,
    end: f32,
    stops: &[(f32, u16)],
) -> Vec<u8> {
    let mut color_line = vec![0u8]; // extend: pad
    color_line.extend_from_slice(&(stops.len() as u16).to_be_bytes());
    for (offset, palette_index) in stops {
        color_line.extend_from_slice(&f2dot14(*offset).to_be_bytes());
        color_line.extend_from_slice(&palette_index.to_be_bytes());
        color_line.extend_from_slice(&f2dot14(1.0).to_be_bytes());
    }
    const HEADER: u32 = 1 + 3 + 8;
    let mut paint = vec![8u8];
    paint.extend_from_slice(&offset24(HEADER));
    for value in [center_x, center_y] {
        paint.extend_from_slice(&value.to_be_bytes());
    }
    paint.extend_from_slice(&f2dot14(start).to_be_bytes());
    paint.extend_from_slice(&f2dot14(end).to_be_bytes());
    paint.extend_from_slice(&color_line);
    paint
}

/// Format 32: `backdrop` painted first, then `source` combined with `mode`.
fn paint_composite(backdrop: &[u8], mode: u8, source: &[u8]) -> Vec<u8> {
    const HEADER: u32 = 1 + 3 + 1 + 3;
    let mut paint = vec![32u8];
    paint.extend_from_slice(&offset24(HEADER + backdrop.len() as u32));
    paint.push(mode);
    paint.extend_from_slice(&offset24(HEADER));
    paint.extend_from_slice(backdrop);
    paint.extend_from_slice(source);
    paint
}

// --- tests ---------------------------------------------------------------

#[test]
fn colrv0_layers_paint_their_palette_colors() {
    let (base, layer_glyph) = (glyph('A'), glyph('H'));
    let font = font_with_color_tables(
        colr_v0_table(base, &[(layer_glyph, 0), (layer_glyph, 1)]),
        cpal_table(&[RED, GREEN]),
    );

    let raster = rasterize(&font, "synthetic:colrv0", base, [0, 0, 0, 255])
        .expect("version 0 layer records paint");
    // Layer 1 is drawn over layer 0, so both colors must appear.
    let mut has_red = false;
    let mut has_green = false;
    for pixel in raster.rgba.chunks_exact(4) {
        if pixel[3] > 200 {
            if pixel[0] > 180 && pixel[1] < 90 {
                has_red = true;
            }
            if pixel[1] > 150 && pixel[0] < 90 {
                has_green = true;
            }
        }
    }
    assert!(has_green, "the top layer's palette color is missing");
    // The first layer only shows where the second does not cover it; the
    // fixture's glyphs overlap, so some red must survive.
    assert!(has_red || mean_color(&raster)[1] > 100.0);
}

#[test]
fn colrv0_foreground_index_uses_the_text_foreground() {
    let (base, layer) = (glyph('A'), glyph('H'));
    let font = font_with_color_tables(colr_v0_table(base, &[(layer, 0xFFFF)]), cpal_table(&[BLUE]));
    let raster = rasterize(&font, "synthetic:foreground", base, [200, 10, 120, 255])
        .expect("foreground layer paints");
    assert_close_to(mean_color(&raster), Rgba([200, 10, 120, 255]), 8.0);
}

#[test]
fn colrv0_palette_alpha_multiplies_into_the_raster() {
    let cpal = cpal_table(&[Rgba([220, 30, 30, 128])]);
    let (base, layer) = (glyph('A'), glyph('H'));
    let font = font_with_color_tables(colr_v0_table(base, &[(layer, 0)]), cpal);

    let raster = rasterize(&font, "synthetic:alpha", base, [0, 0, 0, 255])
        .expect("translucent layer paints");
    let max_alpha = raster
        .rgba
        .chunks_exact(4)
        .map(|pixel| pixel[3])
        .max()
        .expect("pixels");
    assert!(
        (120..=132).contains(&max_alpha),
        "palette alpha 128 became {max_alpha}"
    );
}

#[test]
fn colrv1_paint_glyph_solid_needs_no_clip_box() {
    let base = glyph('A');
    let font = font_with_color_tables(
        colr_v1_table(base, &paint_glyph(base, &paint_solid(0, 1.0))),
        cpal_table(&[GREEN]),
    );
    let raster = rasterize(&font, "synthetic:colrv1", base, [0, 0, 0, 255])
        .expect("a bounded paint graph paints without a ClipBox");
    assert_close_to(mean_color(&raster), GREEN, 8.0);
}

#[test]
fn unbounded_root_solid_paints_nothing() {
    let base = glyph('A');
    let font = font_with_color_tables(
        colr_v1_table(base, &paint_solid(0, 1.0)),
        cpal_table(&[GREEN]),
    );
    assert_eq!(
        rasterize(&font, "synthetic:unbounded", base, [0, 0, 0, 255]),
        None,
        "an unbounded fill must not render"
    );
}

#[test]
fn paint_composite_source_mode_replaces_the_backdrop() {
    let base = glyph('A');
    let composite = paint_composite(
        &paint_glyph(base, &paint_solid(0, 1.0)),
        1, // CompositeMode::Source
        &paint_glyph(base, &paint_solid(1, 1.0)),
    );
    let font = font_with_color_tables(colr_v1_table(base, &composite), cpal_table(&[RED, GREEN]));
    let raster = rasterize(&font, "synthetic:composite", base, [0, 0, 0, 255])
        .expect("composite layers paint");
    // Source replaces the destination, so no red may survive.
    assert_close_to(mean_color(&raster), GREEN, 8.0);
}

#[test]
fn colrv1_linear_gradient_runs_along_its_axis() {
    let base = glyph(BLOCK);
    let bounds = glyph_bounds(BLOCK);
    // The color line spans the glyph's own ink, so t = 0 is its left edge.
    // p2 sits directly above p0, making the color line run horizontally.
    let gradient = paint_linear_gradient(
        bounds.x_min as i16,
        bounds.y_min as i16,
        bounds.x_max as i16,
        bounds.y_min as i16,
        bounds.x_min as i16,
        bounds.y_max as i16,
        &[(0.0, 0), (1.0, 1)],
    );
    let font = font_with_color_tables(
        colr_v1_table(base, &paint_glyph(base, &gradient)),
        cpal_table(&[RED, GREEN]),
    );
    let raster = rasterize_at(&font, "synthetic:linear", base, [0, 0, 0, 255], 64.0)
        .expect("a linear gradient paints");

    let t = |fraction: f32| -> [f32; 4] {
        let x = ((raster.width as f32 - 1.0) * fraction).round() as u32;
        let y = raster.height / 2;
        let index = ((y * raster.width + x) * 4) as usize;
        let pixel = &raster.rgba[index..index + 4];
        [
            f32::from(pixel[0]),
            f32::from(pixel[1]),
            f32::from(pixel[2]),
            f32::from(pixel[3]),
        ]
    };
    let left = t(0.05);
    let middle = t(0.5);
    let right = t(0.95);
    assert!(
        left[3] > 200.0 && left[0] > left[1] + 80.0,
        "the left end of the gradient must be the first stop: {left:?}"
    );
    assert!(
        right[3] > 200.0 && right[1] > right[0] + 80.0,
        "the right end of the gradient must be the last stop: {right:?}"
    );
    assert!(
        middle[0] < left[0] && middle[1] > left[1],
        "the middle of the gradient must interpolate: {middle:?}"
    );
}

#[test]
fn colrv1_sweep_gradient_follows_the_design_grid_angles() {
    let base = glyph(BLOCK);
    let bounds = glyph_bounds(BLOCK);
    let (cx, cy) = (
        i16::midpoint(bounds.x_min as i16, bounds.x_max as i16),
        i16::midpoint(bounds.y_min as i16, bounds.y_max as i16),
    );
    let sweep = paint_sweep_gradient(
        cx,
        cy,
        0.0,
        360.0,
        &[(0.0, 0), (0.25, 1), (0.5, 2), (0.75, 3)],
    );
    let font = font_with_color_tables(
        colr_v1_table(base, &paint_glyph(base, &sweep)),
        cpal_table(&[RED, GREEN, BLUE, YELLOW]),
    );
    let raster = rasterize_at(&font, "synthetic:sweep", base, [0, 0, 0, 255], 64.0)
        .expect("a sweep gradient paints");

    // Design-grid angles are counter-clockwise from +x with y pointing up, so
    // from the ink center: right is stop 0, above is stop 0.25, below is 0.75.
    // The bitmap's own y axis points down.
    let scale = raster.width as f32 / f32::from(bounds.x_max - bounds.x_min);
    let sample = |dx_design: f32, dy_design: f32| -> [f32; 4] {
        let x = ((f32::from(cx) + dx_design - f32::from(bounds.x_min)) * scale) as u32;
        let y = ((f32::from(bounds.y_max) - f32::from(cy) - dy_design) * scale) as u32;
        let index =
            ((y.min(raster.height - 1) * raster.width + x.min(raster.width - 1)) * 4) as usize;
        let pixel = &raster.rgba[index..index + 4];
        [
            f32::from(pixel[0]),
            f32::from(pixel[1]),
            f32::from(pixel[2]),
            f32::from(pixel[3]),
        ]
    };
    let reach = f32::from(bounds.x_max - bounds.x_min) * 0.35;
    // The +x probe must sit strictly on one side of the center row: exactly on
    // it, tiny-skia's angle lands on the sweep's seam (t = 1, the last stop).
    let right = sample(reach, -reach * 0.02);
    let above = sample(0.0, reach);
    let below = sample(0.0, -reach);
    assert!(
        right[3] > 200.0 && right[0] > right[1] + 60.0 && right[0] > right[2] + 60.0,
        "the +x direction must be the first stop: {right:?}"
    );
    assert!(
        above[3] > 200.0 && above[1] > above[0] + 60.0 && above[1] > above[2] + 60.0,
        "the +y direction must be the quarter-turn stop: {above:?}"
    );
    assert!(
        below[3] > 200.0 && below[0] > 150.0 && below[1] > 150.0 && below[2] < 110.0,
        "the -y direction must be the three-quarter-turn stop: {below:?}"
    );
}

/// Nested `PaintGlyph` clips are a conjunction, and the raster must honor the
/// whole stack: the measurement pass intersects every clip, so painting only
/// the innermost one would over-paint inside the outer clip's hole.
#[test]
fn nested_paint_glyph_clips_intersect() {
    let (base, outer, inner) = (glyph('A'), glyph('O'), glyph(BLOCK));
    // Inner ink covers the outer clip's hole, so an un-intersected stack
    // paints the hole and an intersected one leaves it empty.
    let paint = paint_glyph(outer, &paint_glyph(inner, &paint_solid(0, 1.0)));
    let font = font_with_color_tables(colr_v1_table(base, &paint), cpal_table(&[RED]));
    let raster = rasterize_at(&font, "synthetic:nested-clip", base, [0, 0, 0, 255], 96.0)
        .expect("nested clips paint");

    let sample = |x: u32, y: u32| -> [u8; 4] {
        let index = ((y * raster.width + x) * 4) as usize;
        raster.rgba[index..index + 4].try_into().expect("pixel")
    };
    let (mid_x, mid_y) = (raster.width / 2, raster.height / 2);
    assert_eq!(
        sample(mid_x, mid_y)[3],
        0,
        "the inner fill must not paint inside the outer clip's hole"
    );
    // The ring itself is still painted: its left edge at mid height.
    let ring_left = (0..mid_x)
        .rev()
        .map(|x| sample(x, mid_y))
        .find(|pixel| pixel[3] > 200);
    assert!(
        ring_left.is_some(),
        "the outer clip's own stroke must still be painted"
    );
}
