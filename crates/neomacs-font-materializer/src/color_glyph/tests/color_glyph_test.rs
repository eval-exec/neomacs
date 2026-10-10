use super::*;
use neomacs_display_protocol::font::{FontMemoryAsset, FontOutlineAsset};
use std::sync::Arc;

/// The reporter's font: COLR version 1 paint graphs, CPAL, an `SVG ` fallback,
/// and no outline at all under the emoji glyphs.
pub(super) fn colrv1_asset() -> FontOutlineAsset {
    let path = neomacs_test_fonts::noto_color_emoji_colrv1();
    let bytes = std::fs::read(path).expect("read COLRv1 fixture");
    FontOutlineAsset::Memory(
        FontMemoryAsset::new("test:colrv1-noto", Arc::new(bytes), 0).expect("memory asset"),
    )
}

pub(super) fn glyph_id(asset: &FontOutlineAsset, c: char) -> u16 {
    let face =
        ttf_parser::Face::parse(asset.bytes().expect("memory bytes"), 0).expect("fixture parses");
    face.glyph_index(c).expect("fixture maps the char").0
}

pub(super) fn request(glyph_id: u16, px_size: f32) -> ColorGlyphRequest<'static> {
    ColorGlyphRequest {
        glyph_id,
        px_size,
        ..ColorGlyphRequest::default()
    }
}

/// Opaque pixels of a straight-alpha RGBA raster.
fn opaque_pixels(raster: &ColorGlyphRaster) -> Vec<[u8; 4]> {
    raster
        .rgba
        .chunks_exact(4)
        .filter(|pixel| pixel[3] == 255)
        .map(|pixel| [pixel[0], pixel[1], pixel[2], pixel[3]])
        .collect()
}

fn mean(pixels: &[[u8; 4]]) -> [f32; 3] {
    let mut sum = [0f64; 3];
    for pixel in pixels {
        for channel in 0..3 {
            sum[channel] += f64::from(pixel[channel]);
        }
    }
    let count = pixels.len() as f64;
    [
        (sum[0] / count) as f32,
        (sum[1] / count) as f32,
        (sum[2] / count) as f32,
    ]
}

#[test]
fn colrv1_emoji_rasterizes_colored_pixels() {
    let asset = colrv1_asset();
    let mut rasterizer = ColorGlyphRasterizer::new();
    let glyph = glyph_id(&asset, '\u{1F347}');
    let raster = rasterizer
        .rasterize(&asset, &request(glyph, 32.0))
        .expect("COLRv1 source opens")
        .expect("grapes paint through the COLRv1 graph");

    assert_eq!(raster.source, ColorGlyphSource::Colr);
    assert_eq!(
        raster.rgba.len(),
        (raster.width * raster.height * 4) as usize
    );
    assert!(
        (24..=48).contains(&raster.width),
        "32 px emoji rasterized {} px wide",
        raster.width
    );
    assert!(
        (24..=48).contains(&raster.height),
        "32 px emoji rasterized {} px tall",
        raster.height
    );

    let opaque = opaque_pixels(&raster);
    let coverage = opaque.len() as f32 / (raster.width * raster.height) as f32;
    assert!(
        coverage > 0.3,
        "emoji covered only {:.1}% of its bitmap",
        coverage * 100.0
    );

    // A grape emoji is mostly purple: red and blue dominate green.
    let mean = mean(&opaque);
    assert!(
        mean[0] > mean[1] + 15.0 && mean[2] > mean[1] + 15.0,
        "expected purple grapes, got mean rgb {:?}",
        mean
    );
}

#[test]
fn colrv1_emoji_keeps_its_advance_scale_across_sizes() {
    let asset = colrv1_asset();
    let mut rasterizer = ColorGlyphRasterizer::new();
    let glyph = glyph_id(&asset, '\u{1F600}');
    let small = rasterizer
        .rasterize(&asset, &request(glyph, 16.0))
        .expect("color source")
        .expect("smiling face paints");
    let large = rasterizer
        .rasterize(&asset, &request(glyph, 64.0))
        .expect("color source")
        .expect("smiling face paints");
    // The face's emoji are square-ish; quadrupling the size must roughly
    // quadruple the raster, and the placement must scale with it.
    let ratio = large.width as f32 / small.width as f32;
    assert!(
        (3.0..=5.0).contains(&ratio),
        "16 px -> {} px, 64 px -> {} px",
        small.width,
        large.width
    );
    assert!(large.top > small.top);
    assert_eq!(small.rgba.len(), (small.width * small.height * 4) as usize);
}

#[test]
fn bitmap_emoji_faces_have_no_layered_color_source() {
    let path = neomacs_test_fonts::noto_color_emoji_2_051();
    let bytes = std::fs::read(path).expect("read CBDT fixture");
    let asset = FontOutlineAsset::Memory(
        FontMemoryAsset::new("test:cbdt-noto", Arc::new(bytes), 0).expect("memory asset"),
    );
    let mut rasterizer = ColorGlyphRasterizer::new();
    // Embedded color strikes belong to Swash's source chain; this rasterizer
    // must report "not mine" instead of painting from the wrong table.
    assert_eq!(
        rasterizer.rasterize(&asset, &request(0, 32.0)).unwrap(),
        None
    );
}

#[test]
fn faces_without_any_color_table_stay_on_the_outline_path() {
    let bytes =
        std::fs::read(neomacs_test_fonts::mplus_1_code_thin()).expect("read outline-only fixture");
    let asset = FontOutlineAsset::Memory(
        FontMemoryAsset::new("test:outline-only", Arc::new(bytes), 0).expect("memory asset"),
    );
    let mut rasterizer = ColorGlyphRasterizer::new();
    assert_eq!(
        rasterizer.rasterize(&asset, &request(0, 32.0)).unwrap(),
        None
    );
}

#[test]
fn rasterizing_is_deterministic_and_cached() {
    let asset = colrv1_asset();
    let mut rasterizer = ColorGlyphRasterizer::new();
    let glyph = glyph_id(&asset, '\u{1F347}');
    let first = rasterizer
        .rasterize(&asset, &request(glyph, 32.0))
        .unwrap()
        .unwrap();
    let second = rasterizer
        .rasterize(&asset, &request(glyph, 32.0))
        .unwrap()
        .unwrap();
    assert_eq!(first, second, "repeated rasterization must be identical");

    // A generation change must not serve stale bytes.
    rasterizer.clear();
    let third = rasterizer
        .rasterize(&asset, &request(glyph, 32.0))
        .unwrap()
        .unwrap();
    assert_eq!(first, third);
}

#[test]
fn zero_and_invalid_sizes_paint_nothing() {
    let asset = colrv1_asset();
    let mut rasterizer = ColorGlyphRasterizer::new();
    let glyph = glyph_id(&asset, '\u{1F347}');
    for px_size in [0.0, -8.0, f32::NAN, f32::INFINITY] {
        assert_eq!(
            rasterizer
                .rasterize(&asset, &request(glyph, px_size))
                .unwrap(),
            None,
            "size {px_size} must not rasterize"
        );
    }
}
