//! Helpers shared by the color-glyph test modules: the outline fixture, a
//! synthetic-table serializer, and the rasterizer harness.

use super::*;
use crate::FontFileCache;
use neomacs_display_protocol::font::{FontMemoryAsset, FontOutlineAsset};
use std::sync::Arc;

/// The monochrome outline fixture every synthetic color table is attached to.
pub(super) fn outline_fixture() -> Vec<u8> {
    std::fs::read(neomacs_test_fonts::mplus_1_code_thin()).expect("read outline fixture")
}

/// (tag, payload) for every table in `bytes`.
pub(super) fn table_records(bytes: &[u8]) -> Vec<(u32, Vec<u8>)> {
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

/// The outline fixture with `tables` added, replacing same-tag tables.
pub(super) fn fixture_with_tables(tables: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
    let base = outline_fixture();
    let mut records = table_records(&base);
    // The fixture's signature covers the tables being replaced.
    records.retain(|(tag, _)| *tag != u32::from_be_bytes(*b"DSIG"));
    for (tag, payload) in tables {
        let tag = u32::from_be_bytes(**tag);
        records.retain(|(existing, _)| *existing != tag);
        records.push((tag, payload.clone()));
    }
    FontFileCache::standalone_sfnt_from_tables(records).expect("serialize synthetic font")
}

pub(super) fn asset(bytes: Vec<u8>, key: &str) -> FontOutlineAsset {
    FontOutlineAsset::Memory(FontMemoryAsset::new(key, Arc::new(bytes), 0).expect("memory asset"))
}

/// A glyph id from the outline fixture, which every synthetic font reuses.
pub(super) fn glyph(c: char) -> u16 {
    ttf_parser::Face::parse(&outline_fixture(), 0)
        .expect("outline fixture parses")
        .glyph_index(c)
        .expect("fixture maps the char")
        .0
}

/// The ink bounds of `c` in the outline fixture, in design units.
pub(super) fn glyph_bounds(c: char) -> ttf_parser::Rect {
    let bytes = outline_fixture();
    let face = ttf_parser::Face::parse(&bytes, 0).expect("outline fixture parses");
    let glyph = face.glyph_index(c).expect("fixture maps the char");
    face.glyph_bounding_box(glyph)
        .expect("fixture glyph has ink")
}

pub(super) fn rasterize(
    bytes: &[u8],
    key: &str,
    glyph: u16,
    foreground: [u8; 4],
) -> Option<ColorGlyphRaster> {
    rasterize_at(bytes, key, glyph, foreground, 48.0)
}

pub(super) fn rasterize_at(
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
