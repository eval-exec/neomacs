//! COLR behavior tested on synthetic color tables added to a real outline
//! fixture.
//!
//! Real color fonts exercise one shape of the spec each; these cover the
//! rules that a font in the wild cannot be relied on to contain: version 0
//! layer records (which the unified painter must still serve), the
//! foreground palette index, palette alpha, a paint graph without any clip
//! box, an unbounded graph, and composite layers.

use super::test_support::{fixture_with_tables, glyph, glyph_bounds, rasterize, rasterize_at};
use super::*;

#[derive(Clone, Copy, Debug)]
struct Rgba([u8; 4]);

const RED: Rgba = Rgba([220, 30, 30, 255]);
const GREEN: Rgba = Rgba([30, 200, 60, 255]);
const BLUE: Rgba = Rgba([40, 60, 230, 255]);
const YELLOW: Rgba = Rgba([240, 220, 40, 255]);

/// A filled rectangle: gradient sampling needs ink at every probe angle.
const BLOCK: char = '\u{2588}';

/// Add synthetic color tables to the outline fixture.
fn font_with_color_tables(colr: Vec<u8>, cpal: Vec<u8>) -> Vec<u8> {
    fixture_with_tables(&[(b"COLR", colr), (b"CPAL", cpal)])
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
    colr_v1_table_full(base, paint, None, clip_box)
}

/// Version 1 `COLR` with a `LayerList` holding `layers`, for paint graphs the
/// root reaches through `PaintColrLayers`.
fn colr_v1_table_with_layers(
    base: u16,
    paint: &[u8],
    layers: &[Vec<u8>],
    clip_box: Option<(i16, i16, i16, i16)>,
) -> Vec<u8> {
    colr_v1_table_full(base, paint, Some(layers), clip_box)
}

/// A `LayerList` with `layers` as paint tables, offsets from the list start.
fn layer_list_bytes(layers: &[Vec<u8>]) -> Vec<u8> {
    let mut list = Vec::new();
    list.extend_from_slice(&(layers.len() as u32).to_be_bytes());
    let mut offset = 4 + 4 * layers.len() as u32;
    for layer in layers {
        list.extend_from_slice(&offset.to_be_bytes());
        offset += layer.len() as u32;
    }
    for layer in layers {
        list.extend_from_slice(layer);
    }
    list
}

fn colr_v1_table_full(
    base: u16,
    paint: &[u8],
    layers: Option<&[Vec<u8>]>,
    clip_box: Option<(i16, i16, i16, i16)>,
) -> Vec<u8> {
    let base_glyph_list_offset = 34u32;
    let list_header = 4 + 6; // count + one BaseGlyphPaintRecord
    let layer_list = layers.map(layer_list_bytes);
    let paint_end = base_glyph_list_offset + list_header + paint.len() as u32;
    let layer_list_offset = if layer_list.is_some() { paint_end } else { 0 };
    let table_end = paint_end + layer_list.as_ref().map_or(0, Vec::len) as u32;
    // A non-zero offset with no ClipList behind it makes ttf-parser reject the
    // whole table, so the field stays zero when there is nothing to point at.
    let clip_list_offset = if clip_box.is_some() { table_end } else { 0 };
    let mut table = Vec::new();
    table.extend_from_slice(&1u16.to_be_bytes()); // version
    table.extend_from_slice(&0u16.to_be_bytes()); // numBaseGlyphRecords
    table.extend_from_slice(&0u32.to_be_bytes()); // baseGlyphRecordsOffset
    table.extend_from_slice(&0u32.to_be_bytes()); // layerRecordsOffset
    table.extend_from_slice(&0u16.to_be_bytes()); // numLayerRecords
    table.extend_from_slice(&base_glyph_list_offset.to_be_bytes());
    table.extend_from_slice(&layer_list_offset.to_be_bytes());
    table.extend_from_slice(&clip_list_offset.to_be_bytes());
    table.extend_from_slice(&0u32.to_be_bytes()); // varIndexMapOffset
    table.extend_from_slice(&0u32.to_be_bytes()); // itemVariationStoreOffset
    table.extend_from_slice(&1u32.to_be_bytes()); // numBaseGlyphPaintRecords
    table.extend_from_slice(&base.to_be_bytes());
    table.extend_from_slice(&list_header.to_be_bytes()); // paint, from the list start
    table.extend_from_slice(paint);
    if let Some(layer_list) = layer_list {
        table.extend_from_slice(&layer_list);
    }
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

/// Format 8: a sweep gradient around `center`, in counter-clockwise degrees
/// from +x in the design grid, as the spec defines them.
///
/// The spec stores degrees biased by 180 ("add 1.0 and multiply by 180° to
/// retrieve counter-clockwise degrees"), and the builder takes the degrees so
/// tests cannot repeat the mistake of encoding them raw.
fn paint_sweep_gradient(
    center_x: i16,
    center_y: i16,
    start_degrees: f32,
    end_degrees: f32,
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
    for degrees in [start_degrees, end_degrees] {
        paint.extend_from_slice(&f2dot14(degrees / 180.0 - 1.0).to_be_bytes());
    }
    paint.extend_from_slice(&color_line);
    paint
}

/// Format 1: the layer list range is painted bottom-up.
fn paint_colr_layers(first_layer_index: u32, layers_count: u8) -> Vec<u8> {
    let mut paint = vec![1u8];
    paint.push(layers_count);
    paint.extend_from_slice(&first_layer_index.to_be_bytes());
    paint
}

/// Format 12: an affine transform applied to `child`. Matrix values are
/// 16.16 fixed point, in the order xx, yx, xy, yy, dx, dy.
fn paint_transform(child: &[u8], matrix: [f32; 6]) -> Vec<u8> {
    const HEADER: u32 = 1 + 3 + 3;
    const MATRIX: u32 = 24;
    let mut paint = vec![12u8];
    paint.extend_from_slice(&offset24(HEADER + MATRIX)); // child, after the matrix
    paint.extend_from_slice(&offset24(HEADER)); // Affine2x3
    for value in matrix {
        let fixed = (f64::from(value) * 65536.0).round() as i32;
        paint.extend_from_slice(&fixed.to_be_bytes());
    }
    paint.extend_from_slice(child);
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
            Some((bounds.x_min, bounds.y_min, half, bounds.y_max)),
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
    // A box whose right edge cuts through the ring's ink at a fractional
    // device position: that is where a second application of the same mask to
    // the composited layer would square the edge coverage.
    let bounds = glyph_bounds('O');
    let clip_box = Some((
        bounds.x_min,
        bounds.y_min,
        bounds.x_min + 3 * (bounds.x_max - bounds.x_min) / 4,
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
    // The shader's unit angle wraps at +x, where t = 0 — the first stop. A
    // hair below the axis still rounds onto the center row, which is the seam
    // row itself.
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

// --- paint graph fidelity regressions ------------------------------------

/// A `PaintGlyph` clip and the fill it bounds are the same path.  Repainting
/// that fill through the clip's own coverage multiplies every antialiased
/// edge by itself, so version 1 artwork would render thinner than the same
/// shape through a version 0 layer record.  One painter serves both versions,
/// so the two rasters must agree exactly.
#[test]
fn paint_glyph_edges_match_version0_layers() {
    let (base, layer) = (glyph('A'), glyph('O'));
    let version0 = font_with_color_tables(colr_v0_table(base, &[(layer, 0)]), cpal_table(&[RED]));
    let version1 = font_with_color_tables(
        colr_v1_table(base, &paint_glyph(layer, &paint_solid(0, 1.0))),
        cpal_table(&[RED]),
    );
    let flat = rasterize_at(&version0, "synthetic:v0-edges", base, [0, 0, 0, 255], 64.0)
        .expect("the version 0 layer paints");
    let painted = rasterize_at(&version1, "synthetic:v1-edges", base, [0, 0, 0, 255], 64.0)
        .expect("the version 1 fill paints");
    assert_eq!((flat.width, flat.height), (painted.width, painted.height));
    let partial = flat
        .rgba
        .chunks_exact(4)
        .filter(|pixel| (1..255).contains(&pixel[3]))
        .count();
    assert!(
        partial > 12,
        "the shape needs antialiased edges for the probe: {partial} partial pixels"
    );
    let worst = flat
        .rgba
        .iter()
        .zip(&painted.rgba)
        .map(|(flat, painted)| flat.abs_diff(*painted))
        .max()
        .expect("rasters have pixels");
    assert!(
        worst <= 2,
        "the clip was applied to its own fill: worst channel delta {worst}"
    );
}

/// The same clip pushed twice around the same fill must not compound: the
/// region it paints is one copy of the outline.
#[test]
fn nested_identical_clips_apply_once() {
    let (base, layer) = (glyph('A'), glyph('O'));
    let once = font_with_color_tables(
        colr_v1_table(base, &paint_glyph(layer, &paint_solid(0, 1.0))),
        cpal_table(&[RED]),
    );
    let twice = font_with_color_tables(
        colr_v1_table(
            base,
            &paint_glyph(layer, &paint_glyph(layer, &paint_solid(0, 1.0))),
        ),
        cpal_table(&[RED]),
    );
    let once = rasterize_at(&once, "synthetic:one-clip", base, [0, 0, 0, 255], 64.0)
        .expect("a single clip paints");
    let twice = rasterize_at(&twice, "synthetic:two-clips", base, [0, 0, 0, 255], 64.0)
        .expect("nested identical clips paint");
    assert_eq!((once.width, once.height), (twice.width, twice.height));
    let worst = once
        .rgba
        .iter()
        .zip(&twice.rgba)
        .map(|(once, twice)| once.abs_diff(*twice))
        .max()
        .expect("rasters have pixels");
    assert!(
        worst <= 2,
        "an identical clip was applied twice: worst channel delta {worst}"
    );
}

/// Spec: a clip box makes the color glyph bounded without inspecting the
/// graph, so a root fill with no shape of its own paints the box.
#[test]
fn clip_box_bounds_a_shapeless_fill() {
    let base = glyph(BLOCK);
    let bounds = glyph_bounds(BLOCK);
    let half = i16::midpoint(bounds.x_min, bounds.x_max);
    let full = font_with_color_tables(
        colr_v1_table(base, &paint_glyph(base, &paint_solid(0, 1.0))),
        cpal_table(&[RED]),
    );
    let boxed = font_with_color_tables(
        colr_v1_table_with_clip_box(
            base,
            &paint_solid(0, 1.0),
            Some((bounds.x_min, bounds.y_min, half, bounds.y_max)),
        ),
        cpal_table(&[RED]),
    );
    let full = rasterize_at(
        &full,
        "synthetic:box-solid-full",
        base,
        [0, 0, 0, 255],
        64.0,
    )
    .expect("the shaped fill paints");
    let cut = rasterize_at(&boxed, "synthetic:box-solid", base, [0, 0, 0, 255], 64.0)
        .expect("the clip box alone bounds the fill, so it must paint");
    let half_width = full.width as i32 / 2;
    assert!(
        (half_width - 3..=half_width + 3).contains(&(cut.width as i32)),
        "the box is half the glyph but the raster is {} px of {}",
        cut.width,
        full.width
    );
    assert!(
        cut.rgba.chunks_exact(4).any(|pixel| pixel[3] == 255),
        "the box interior must be filled"
    );
}

/// Spec: an outline with no contours puts an empty clip in force.  Everything
/// inside it must disappear, however far down the graph it sits.
#[test]
fn inkless_clip_hides_its_subgraph() {
    let (base, blank) = (glyph('A'), glyph(' '));
    let font = font_with_color_tables(
        colr_v1_table(
            base,
            &paint_glyph(blank, &paint_glyph(glyph(BLOCK), &paint_solid(0, 1.0))),
        ),
        cpal_table(&[RED]),
    );
    assert_eq!(
        rasterize(&font, "synthetic:inkless-clip", base, [0, 0, 0, 255]),
        None,
        "an empty clip must clip everything away"
    );
}

/// A bare fill after a `pop_clip` has no shape of its own any more.  Nothing
/// bounds it — no clip box and no `PaintGlyph` ancestor — so the color glyph
/// definition is unbounded, and the spec requires the whole glyph not to
/// render, not just the bare fill to be skipped.
#[test]
fn a_bare_layer_fill_has_no_inherited_outline() {
    let (base, layer) = (glyph('A'), glyph('H'));
    let layers = vec![
        paint_glyph(layer, &paint_solid(0, 1.0)),
        paint_solid(1, 1.0),
    ];
    let root = paint_colr_layers(0, 2);
    let unbounded = font_with_color_tables(
        colr_v1_table_with_layers(base, &root, &layers, None),
        cpal_table(&[RED, GREEN]),
    );
    assert_eq!(
        rasterize(&unbounded, "synthetic:stale-outline", base, [0, 0, 0, 255]),
        None,
        "an unbounded color glyph must not render at all"
    );

    // The same graph under a clip box is bounded: the bare fill paints the
    // box, and the second layer covers the first.
    let bounds = glyph_bounds('H');
    let bounded = font_with_color_tables(
        colr_v1_table_with_layers(
            base,
            &root,
            &layers,
            Some((bounds.x_min, bounds.y_min, bounds.x_max, bounds.y_max)),
        ),
        cpal_table(&[RED, GREEN]),
    );
    let raster = rasterize(&bounded, "synthetic:boxed-layers", base, [0, 0, 0, 255])
        .expect("a boxed layer list paints");
    assert_close_to(mean_color(&raster), GREEN, 8.0);
}

/// Spec format 32: COMPOSITE_SRC_IN is bounded when *either* operand is, so
/// an unbounded backdrop must still paint: it fills the surface under the
/// source, and source-in keeps the artwork.  Skipping the unbounded fill
/// would leave the destination transparent and erase the composite's result.
#[test]
fn composite_src_in_paints_its_unbounded_backdrop() {
    let base = glyph('A');
    let composite = paint_composite(
        &paint_solid(0, 1.0),                     // backdrop: red, unbounded
        5,                                        // CompositeMode::SourceIn
        &paint_glyph(base, &paint_solid(1, 1.0)), // source: bounded green A
    );
    let font = font_with_color_tables(colr_v1_table(base, &composite), cpal_table(&[RED, GREEN]));
    let raster = rasterize(&font, "synthetic:composite-src-in", base, [0, 0, 0, 255])
        .expect("a bounded composite paints");
    let painted = raster.rgba.chunks_exact(4).filter(|p| p[3] > 0).count();
    assert!(
        painted > 100,
        "source-in of a full-surface backdrop must keep the source's ink, got {painted} painted pixels"
    );
    // The backdrop's alpha is one everywhere, so source-in is the source
    // itself: every covered pixel is the source colour, at its own coverage.
    assert_close_to(mean_color(&raster), GREEN, 8.0);
}

/// Spec format 32: COMPOSITE_CLEAR is always bounded, but clearing over
/// nothing observable leaves nothing to show, and the glyph measures empty.
#[test]
fn composite_clear_over_unbounded_fills_measures_empty() {
    let base = glyph('A');
    let composite = paint_composite(
        &paint_solid(0, 1.0),
        0, // CompositeMode::Clear
        &paint_solid(1, 1.0),
    );
    let font = font_with_color_tables(colr_v1_table(base, &composite), cpal_table(&[RED, GREEN]));
    assert_eq!(
        rasterize(&font, "synthetic:composite-clear", base, [0, 0, 0, 255]),
        None,
        "a clear with no bounded content shows nothing"
    );
}

/// A legal graph can fan out exponentially through the one `LayerList`;
/// ttf-parser bounds recursion depth and cycles but not the number of visits,
/// so the recorder has to stop instead of allocating millions of steps.
#[test]
fn layer_fan_out_is_capped() {
    /// `leaves` fills, then `levels` `PaintColrLayers`, each referencing
    /// every layer before it.  Nothing repeats on a path, so the graph is
    /// legal; the visit count doubles per level.
    fn fan_out(leaves: usize, levels: usize) -> (Vec<Vec<u8>>, Vec<u8>) {
        let mut layers: Vec<Vec<u8>> = (0..leaves)
            .map(|_| paint_glyph(glyph(BLOCK), &paint_solid(0, 1.0)))
            .collect();
        for _ in 0..levels {
            layers.push(paint_colr_layers(0, layers.len() as u8));
        }
        let root = paint_colr_layers(layers.len() as u32 - 1, 1);
        (layers, root)
    }

    let base = glyph('A');
    let (layers, root) = fan_out(3, 7);
    let modest = font_with_color_tables(
        colr_v1_table_with_layers(base, &root, &layers, None),
        cpal_table(&[RED]),
    );
    assert!(
        rasterize_at(
            &modest,
            "synthetic:fan-out-small",
            base,
            [0, 0, 0, 255],
            8.0
        )
        .is_some(),
        "a graph below the cap must keep painting"
    );

    let (layers, root) = fan_out(3, 20);
    let exploded = font_with_color_tables(
        colr_v1_table_with_layers(base, &root, &layers, None),
        cpal_table(&[RED]),
    );
    assert_eq!(
        rasterize_at(
            &exploded,
            "synthetic:fan-out-big",
            base,
            [0, 0, 0, 255],
            8.0
        ),
        None,
        "a graph past the cap must be dropped, not allocated"
    );
}

/// Nested transforms can push device coordinates far past `i32`; the
/// measurement must reject such a glyph instead of overflowing.
#[test]
fn huge_transforms_do_not_overflow_the_measurement() {
    let base = glyph(BLOCK);
    let huge = paint_transform(
        &paint_transform(
            &paint_glyph(base, &paint_solid(0, 1.0)),
            [32000.0, 0.0, 0.0, 32000.0, -32000.0, -32000.0],
        ),
        [32000.0, 0.0, 0.0, 32000.0, 0.0, 0.0],
    );
    let font = font_with_color_tables(colr_v1_table(base, &huge), cpal_table(&[RED]));
    assert_eq!(
        rasterize(&font, "synthetic:huge-transform", base, [0, 0, 0, 255]),
        None,
        "a glyph scaled past the size cap must not render"
    );
}

/// A non-source-over composite is clipped by its contents, not by a second
/// application of the clip mask at composite time: the composited artwork
/// must have the same edges as the plain fill.
#[test]
fn non_source_over_composites_do_not_erode_edges() {
    let base = glyph('O');
    let invisible = Rgba([0, 0, 0, 0]);
    let bounds = glyph_bounds('O');
    // The box's right edge must cut through the ring's ink: a box edge over
    // the counter would show no coverage anywhere and mask nothing.
    let clip_box = Some((
        bounds.x_min,
        bounds.y_min,
        bounds.x_min + 3 * (bounds.x_max - bounds.x_min) / 4,
        bounds.y_max,
    ));
    let plain = font_with_color_tables(
        colr_v1_table_with_clip_box(base, &paint_glyph(base, &paint_solid(0, 1.0)), clip_box),
        cpal_table(&[RED, invisible]),
    );
    let composited = font_with_color_tables(
        colr_v1_table_with_clip_box(
            base,
            &paint_composite(
                &paint_glyph(base, &paint_solid(1, 1.0)),
                1, // CompositeMode::Source
                &paint_glyph(base, &paint_solid(0, 1.0)),
            ),
            clip_box,
        ),
        cpal_table(&[RED, invisible]),
    );
    let plain = rasterize_at(&plain, "synthetic:source-plain", base, [0, 0, 0, 255], 96.0)
        .expect("the plain fill paints");
    let composited = rasterize_at(
        &composited,
        "synthetic:source-layer",
        base,
        [0, 0, 0, 255],
        96.0,
    )
    .expect("the composited fill paints");
    assert_eq!(
        (plain.width, plain.height),
        (composited.width, composited.height)
    );
    let worst = plain
        .rgba
        .iter()
        .zip(&composited.rgba)
        .map(|(plain, composited)| plain.abs_diff(*composited))
        .max()
        .expect("rasters have pixels");
    assert!(
        worst <= 2,
        "source compositing eroded the artwork: worst channel delta {worst}"
    );
}

/// Spec: when every stop shares one offset, the first colour holds below it
/// and the last holds at and above it — a hard cut, never a ramp.
#[test]
fn equal_offset_stops_cut_hard() {
    let base = glyph(BLOCK);
    let bounds = glyph_bounds(BLOCK);
    let gradient = paint_linear_gradient(
        bounds.x_min,
        bounds.y_min,
        bounds.x_max,
        bounds.y_min,
        bounds.x_min,
        bounds.y_max,
        &[(0.5, 0), (0.5, 1)],
    );
    let font = font_with_color_tables(
        colr_v1_table(base, &paint_glyph(base, &gradient)),
        cpal_table(&[RED, GREEN]),
    );
    let raster = rasterize_at(&font, "synthetic:hard-cut", base, [0, 0, 0, 255], 64.0)
        .expect("a hard-cut color line paints");
    let at = |fraction: f32| -> [u8; 4] {
        let x = ((raster.width as f32 - 1.0) * fraction).round() as u32;
        let y = raster.height / 2;
        let index = ((y * raster.width + x) * 4) as usize;
        raster.rgba[index..index + 4].try_into().expect("pixel")
    };
    let left = at(0.25);
    let right = at(0.75);
    assert!(
        left[0] > 200 && left[1] < 60,
        "below the offset the first colour holds: {left:?}"
    );
    assert!(
        right[1] > 150 && right[0] < 90,
        "above the offset the last colour holds: {right:?}"
    );
}

/// The same rule with every offset past the domain: the whole line is the
/// first colour, never a ramp.
#[test]
fn equal_offset_stops_outside_the_domain_hold_the_first_color() {
    let base = glyph(BLOCK);
    let bounds = glyph_bounds(BLOCK);
    let gradient = paint_linear_gradient(
        bounds.x_min,
        bounds.y_min,
        bounds.x_max,
        bounds.y_min,
        bounds.x_min,
        bounds.y_max,
        &[(2.0, 0), (2.0, 1)],
    );
    let font = font_with_color_tables(
        colr_v1_table(base, &paint_glyph(base, &gradient)),
        cpal_table(&[RED, GREEN]),
    );
    let raster = rasterize_at(&font, "synthetic:offset-cut", base, [0, 0, 0, 255], 64.0)
        .expect("the constant line paints");
    let opaque: Vec<[u8; 4]> = raster
        .rgba
        .chunks_exact(4)
        .filter(|pixel| pixel[3] > 200)
        .map(|pixel| pixel.try_into().expect("pixel"))
        .collect();
    assert!(opaque.len() > 20, "the fill must cover the block");
    assert!(
        opaque.iter().all(|pixel| pixel[0] > 200 && pixel[1] < 60),
        "every covered pixel must be the first colour"
    );
}

/// Spec: the sweep arc is given in counter-clockwise degrees (stored biased
/// by 180); outside the arc the nearest end colour holds.
#[test]
fn sweep_gradient_maps_its_arc_to_degrees() {
    let base = glyph(BLOCK);
    let bounds = glyph_bounds(BLOCK);
    let (cx, cy) = (
        i16::midpoint(bounds.x_min, bounds.x_max),
        i16::midpoint(bounds.y_min, bounds.y_max),
    );
    // A quarter-turn arc from +y to -y through -x, red to green.
    let sweep = paint_sweep_gradient(cx, cy, 90.0, 270.0, &[(0.0, 0), (1.0, 1)]);
    let font = font_with_color_tables(
        colr_v1_table(base, &paint_glyph(base, &sweep)),
        cpal_table(&[RED, GREEN]),
    );
    let raster = rasterize_at(&font, "synthetic:sweep-arc", base, [0, 0, 0, 255], 64.0)
        .expect("a sweep arc paints");
    let scale = raster.width as f32 / f32::from(bounds.x_max - bounds.x_min);
    let sample = |dx: f32, dy: f32| -> [u8; 4] {
        let x = ((f32::from(cx) + dx - f32::from(bounds.x_min)) * scale) as u32;
        let y = ((f32::from(bounds.y_max) - f32::from(cy) - dy) * scale) as u32;
        let index =
            ((y.min(raster.height - 1) * raster.width + x.min(raster.width - 1)) * 4) as usize;
        raster.rgba[index..index + 4].try_into().expect("pixel")
    };
    let reach = f32::from(bounds.x_max - bounds.x_min) * 0.35;
    let start = sample(reach * 0.02, reach); // the arc's start at +y
    let end = sample(reach * 0.02, -reach);
    let middle = sample(-reach, 0.0); // -x is the arc's midpoint
    // 30 degrees past the arc's end, far enough from the +x seam that the
    // probe lands on its own pixel row.
    let outside = sample(reach * 0.5, -reach * 0.866);
    assert!(
        start[0] > 200 && start[1] < 60,
        "the arc starts red at +y: {start:?}"
    );
    assert!(
        middle[0] > 90 && middle[1] > 90,
        "the arc's midpoint must interpolate: {middle:?}"
    );
    assert!(
        end[1] > 150 && end[0] < 90,
        "the arc ends green at -y: {end:?}"
    );
    assert!(
        outside[1] > 150 && outside[0] < 90,
        "after the arc the last colour holds: {outside:?}"
    );
}

/// Spec: a transform inside a `PaintGlyph` applies to the child paint, not to
/// the clip outline.  Scaling the paint around its own centre must not shrink
/// the region the paint is shown in.
#[test]
fn paint_transform_inside_paint_glyph_does_not_move_the_clip() {
    let (base, layer) = (glyph('A'), glyph('O'));
    let bounds = glyph_bounds('O');
    let scale = 0.6;
    let (cx, cy) = (
        f32::from(i16::midpoint(bounds.x_min, bounds.x_max)),
        f32::from(i16::midpoint(bounds.y_min, bounds.y_max)),
    );
    let about_center = [
        scale,
        0.0,
        0.0,
        scale,
        cx * (1.0 - scale),
        cy * (1.0 - scale),
    ];
    let plain = font_with_color_tables(
        colr_v1_table(base, &paint_glyph(layer, &paint_solid(0, 1.0))),
        cpal_table(&[RED]),
    );
    let transformed = font_with_color_tables(
        colr_v1_table(
            base,
            &paint_glyph(layer, &paint_transform(&paint_solid(0, 1.0), about_center)),
        ),
        cpal_table(&[RED]),
    );
    let plain = rasterize_at(&plain, "synthetic:clip-plain", base, [0, 0, 0, 255], 64.0)
        .expect("the plain fill paints");
    let transformed = rasterize_at(
        &transformed,
        "synthetic:clip-transformed",
        base,
        [0, 0, 0, 255],
        64.0,
    )
    .expect("the transformed fill paints");
    assert_eq!(
        (plain.width, plain.height),
        (transformed.width, transformed.height),
        "the transform must not resize the clip region"
    );
    let worst = plain
        .rgba
        .iter()
        .zip(&transformed.rgba)
        .map(|(plain, transformed)| plain.abs_diff(*transformed))
        .max()
        .expect("rasters have pixels");
    assert!(
        worst == 0,
        "the transform reached the clip outline: worst channel delta {worst}"
    );
}

/// A transform inside a `PaintGlyph` must move the gradient with the paint:
/// translating the paint by `dx` is the same graphic as defining the gradient
/// `dx` further along.
#[test]
fn paint_transform_inside_paint_glyph_moves_the_gradient() {
    let base = glyph(BLOCK);
    let bounds = glyph_bounds(BLOCK);
    let dx = 120i16;
    let gradient = |shift: i16| {
        paint_linear_gradient(
            bounds.x_min + shift,
            bounds.y_min,
            bounds.x_max + shift,
            bounds.y_min,
            bounds.x_min + shift,
            bounds.y_max,
            &[(0.0, 0), (1.0, 1)],
        )
    };
    let shifted = font_with_color_tables(
        colr_v1_table(base, &paint_glyph(base, &gradient(dx))),
        cpal_table(&[RED, GREEN]),
    );
    let translated = font_with_color_tables(
        colr_v1_table(
            base,
            &paint_glyph(
                base,
                &paint_transform(&gradient(0), [1.0, 0.0, 0.0, 1.0, f32::from(dx), 0.0]),
            ),
        ),
        cpal_table(&[RED, GREEN]),
    );
    let shifted = rasterize_at(
        &shifted,
        "synthetic:gradient-shifted",
        base,
        [0, 0, 0, 255],
        64.0,
    )
    .expect("the shifted gradient paints");
    let translated = rasterize_at(
        &translated,
        "synthetic:gradient-translated",
        base,
        [0, 0, 0, 255],
        64.0,
    )
    .expect("the translated gradient paints");
    assert_eq!(
        (shifted.width, shifted.height),
        (translated.width, translated.height)
    );
    let worst = shifted
        .rgba
        .iter()
        .zip(&translated.rgba)
        .map(|(shifted, translated)| shifted.abs_diff(*translated))
        .max()
        .expect("rasters have pixels");
    assert!(
        worst <= 2,
        "the transform did not reach the gradient: worst channel delta {worst}"
    );
}
