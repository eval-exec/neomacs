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

/// Pixels that are fully covered must hold exactly the top layer's colour:
/// source-over with full coverage replaces the destination outright, so any
/// other value there means the layer order or the colour mapping is wrong.
fn fully_opaque_mismatches(raster: &ColorGlyphRaster, expected: Rgba) -> (usize, usize) {
    let mut opaque = 0;
    let mut mismatched = 0;
    for pixel in raster.rgba.chunks_exact(4) {
        if pixel[3] != 255 {
            continue;
        }
        opaque += 1;
        if (0..3).any(|channel| pixel[channel].abs_diff(expected.0[channel]) > 2) {
            mismatched += 1;
        }
    }
    (opaque, mismatched)
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
    colr_v1_table_with_clip_box(base, paint, None)
}

/// Version 1 `COLR`, optionally carrying a `ClipList` whose single `ClipBox`
/// covers `base`.
///
/// The clip box is a bounding box: content outside it must not render, and it
/// is the one clip ancestor that stays active across a whole composite's
/// layers.
fn colr_v1_table_with_clip_box(
    base: u16,
    paint: &[u8],
    clip_box: Option<(i16, i16, i16, i16)>,
) -> Vec<u8> {
    let base_glyph_list_offset = 34u32;
    let list_header = 4 + 6; // count + one BaseGlyphPaintRecord
    // A non-zero offset with no ClipList behind it makes ttf-parser reject the
    // whole table, so the field stays zero when there is nothing to point at.
    let clip_list_offset = if clip_box.is_some() {
        base_glyph_list_offset + list_header + paint.len() as u32
    } else {
        0
    };
    let mut table = Vec::new();
    table.extend_from_slice(&1u16.to_be_bytes()); // version
    table.extend_from_slice(&0u16.to_be_bytes()); // numBaseGlyphRecords
    table.extend_from_slice(&0u32.to_be_bytes()); // baseGlyphRecordsOffset
    table.extend_from_slice(&0u32.to_be_bytes()); // layerRecordsOffset
    table.extend_from_slice(&0u16.to_be_bytes()); // numLayerRecords
    table.extend_from_slice(&base_glyph_list_offset.to_be_bytes());
    table.extend_from_slice(&0u32.to_be_bytes()); // layerListOffset
    table.extend_from_slice(&clip_list_offset.to_be_bytes());
    table.extend_from_slice(&0u32.to_be_bytes()); // varIndexMapOffset
    table.extend_from_slice(&0u32.to_be_bytes()); // itemVariationStoreOffset
    table.extend_from_slice(&1u32.to_be_bytes()); // numBaseGlyphPaintRecords
    table.extend_from_slice(&base.to_be_bytes());
    table.extend_from_slice(&list_header.to_be_bytes()); // paint, from the list start
    table.extend_from_slice(paint);
    if let Some((x_min, y_min, x_max, y_max)) = clip_box {
        table.push(1); // ClipList format
        table.extend_from_slice(&1u32.to_be_bytes()); // numClips
        table.extend_from_slice(&base.to_be_bytes()); // startGlyphID
        table.extend_from_slice(&base.to_be_bytes()); // endGlyphID
        table.extend_from_slice(&offset24(5 + 7)); // ClipBox, from the ClipList start
        table.push(1); // ClipBox format 1 (static)
        for value in [x_min, y_min, x_max, y_max] {
            table.extend_from_slice(&value.to_be_bytes());
        }
    }
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
    // The same shape twice, painted over itself: the later layer must win, in
    // palette order.  Overlapping distinct shapes could not distinguish "both
    // layers painted" from "the top layer painted".
    let green_on_top = font_with_color_tables(
        colr_v0_table(base, &[(layer_glyph, 0), (layer_glyph, 1)]),
        cpal_table(&[RED, GREEN]),
    );
    let raster = rasterize_at(
        &green_on_top,
        "synthetic:colrv0",
        base,
        [0, 0, 0, 255],
        96.0,
    )
    .expect("version 0 layer records paint");
    let (opaque, mismatched) = fully_opaque_mismatches(&raster, GREEN);
    assert!(opaque > 20, "only {opaque} fully covered pixels");
    assert_eq!(mismatched, 0, "the top layer must win where it is opaque");

    let red_on_top = font_with_color_tables(
        colr_v0_table(base, &[(layer_glyph, 1), (layer_glyph, 0)]),
        cpal_table(&[RED, GREEN]),
    );
    let raster = rasterize_at(
        &red_on_top,
        "synthetic:colrv0-reversed",
        base,
        [0, 0, 0, 255],
        96.0,
    )
    .expect("version 0 layer records paint in order");
    let (opaque, mismatched) = fully_opaque_mismatches(&raster, RED);
    assert!(opaque > 20, "only {opaque} fully covered pixels");
    assert_eq!(
        mismatched, 0,
        "reversing the layer order must reverse the result"
    );
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
fn clip_box_bounds_the_raster() {
    let base = glyph(BLOCK);
    let bounds = glyph_bounds(BLOCK);
    let paint = paint_glyph(base, &paint_solid(0, 1.0));
    let unclipped = font_with_color_tables(colr_v1_table(base, &paint), cpal_table(&[RED]));
    // A box over the left half of the block: the artwork must be cut at the
    // box, not merely resized.
    let half = i16::midpoint(bounds.x_min, bounds.x_max);
    let clipped = font_with_color_tables(
        colr_v1_table_with_clip_box(
            base,
            &paint,
            Some((
                bounds.x_min,
                bounds.y_min,
                half,
                bounds.y_max,
            )),
        ),
        cpal_table(&[RED]),
    );

    let wide = rasterize_at(&unclipped, "synthetic:nobox", base, [0, 0, 0, 255], 64.0)
        .expect("the unclipped fill paints");
    let cut = rasterize_at(&clipped, "synthetic:clipbox", base, [0, 0, 0, 255], 64.0)
        .expect("the clipped fill paints");
    assert!(
        cut.width + 2 <= wide.width / 2 + 4,
        "a half-width clip box left a {} px raster of the {} px one",
        cut.width,
        wide.width
    );
    // The cut edge is still painted: the clip is a bound, not a shrink.
    let last_column: u32 = cut
        .rgba
        .chunks_exact(4)
        .skip((cut.width - 1) as usize)
        .step_by(cut.width as usize)
        .map(|pixel| u32::from(pixel[3]))
        .sum();
    assert!(last_column > 0, "the box's edge must carry ink");
}

#[test]
fn source_over_composite_does_not_erode_edges() {
    // A curved shape: its outline gives the clip mask partial coverage at the
    // rim, which is exactly where a second application of that mask would
    // square the edge alpha.
    let base = glyph('O');
    let invisible = Rgba([0, 0, 0, 0]);
    // A box whose right edge cuts through the ink at a fractional device
    // position: that is where a second application of the same mask to the
    // composited layer would square the edge coverage.
    let bounds = glyph_bounds('O');
    let clip_box = Some((
        bounds.x_min,
        bounds.y_min,
        i16::midpoint(bounds.x_min, bounds.x_max) + 3,
        bounds.y_max,
    ));
    let plain = font_with_color_tables(
        colr_v1_table_with_clip_box(base, &paint_glyph(base, &paint_solid(0, 1.0)), clip_box),
        cpal_table(&[RED, invisible]),
    );
    // The same artwork one PaintComposite deep, over a backdrop that paints
    // nothing: the composited layer's clip was already applied when it was
    // filled, so applying it again would squarely multiply every edge
    // coverage and thin the shape.
    let composited = font_with_color_tables(
        colr_v1_table_with_clip_box(
            base,
            &paint_composite(
                &paint_glyph(base, &paint_solid(1, 1.0)),
                3, // CompositeMode::SourceOver
                &paint_glyph(base, &paint_solid(0, 1.0)),
            ),
            clip_box,
        ),
        cpal_table(&[RED, invisible]),
    );

    let plain_raster = rasterize_at(
        &plain,
        "synthetic:composite-plain",
        base,
        [0, 0, 0, 255],
        96.0,
    )
    .expect("the plain fill paints");
    let composited_raster = rasterize_at(
        &composited,
        "synthetic:composite-layer",
        base,
        [0, 0, 0, 255],
        96.0,
    )
    .expect("the composited fill paints");
    assert!(plain_raster.rgba.chunks_exact(4).any(|pixel| pixel[3] > 0));
    assert_eq!(
        (plain_raster.width, plain_raster.height),
        (composited_raster.width, composited_raster.height)
    );
    let worst = plain_raster
        .rgba
        .iter()
        .zip(&composited_raster.rgba)
        .map(|(plain, composited)| plain.abs_diff(*composited))
        .max()
        .expect("rasters have pixels");
    assert!(
        worst <= 1,
        "compositing eroded the artwork: worst channel delta {worst}"
    );
}

#[test]
fn colrv1_linear_gradient_runs_along_its_axis() {
    let base = glyph(BLOCK);
    let bounds = glyph_bounds(BLOCK);
    // The color line spans the glyph's own ink, so t = 0 is its left edge.
    // p2 sits directly above p0, making the color line run horizontally.
    let gradient = paint_linear_gradient(
        bounds.x_min,
        bounds.y_min,
        bounds.x_max,
        bounds.y_min,
        bounds.x_min,
        bounds.y_max,
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
        i16::midpoint(bounds.x_min, bounds.x_max),
        i16::midpoint(bounds.y_min, bounds.y_max),
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
