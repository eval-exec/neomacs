use super::*;
use crate::frame_glyphs::GlyphRowRole;
use crate::glyph_matrix::Glyph;
use crate::types::FaceId;

#[test]
fn visual_indices_and_materialized_columns_use_distinct_mappings() {
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    row.pixel_x = 10.0;
    let mut wide = Glyph::char('中', FaceId::new(0), 0).with_pixel_width(20.0);
    wide.wide = true;
    row.glyphs[GlyphArea::Text.index()] = vec![
        wide,
        Glyph::padding_for(FaceId::new(0), 0),
        Glyph::char('a', FaceId::new(0), 1).with_pixel_width(12.0),
    ];
    let geometry = TextRowGeometry::new(&row, Rect::new(4.0, 0.0, 80.0, 16.0), 8.0);
    assert_eq!(geometry.origin_x(), 14.0);
    assert_eq!(
        geometry.x_at_glyph(VisualTextGlyphIndex::new(1)),
        Some(34.0)
    );
    assert_eq!(geometry.x_at_slot(TextSlotColumn::new(1)), Some(14.0));
    assert_eq!(
        geometry.x_at_glyph(VisualTextGlyphIndex::new(2)),
        Some(34.0)
    );
    assert_eq!(geometry.x_at_slot(TextSlotColumn::new(2)), Some(34.0));
    assert_eq!(geometry.x_at_slot(TextSlotColumn::new(3)), Some(46.0));
    assert_eq!(geometry.x_at_glyph(VisualTextGlyphIndex::new(4)), None);
    assert_eq!(geometry.x_at_slot(TextSlotColumn::new(4)), None);
}

#[test]
fn reversed_geometry_right_aligns_the_same_advances() {
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    row.reversed_p = true;
    row.glyphs[GlyphArea::Text.index()] = vec![
        Glyph::char('ב', FaceId::new(0), 1).with_pixel_width(12.0),
        Glyph::char('א', FaceId::new(0), 0).with_pixel_width(20.0),
    ];
    let geometry = TextRowGeometry::new(&row, Rect::new(4.0, 0.0, 80.0, 16.0), 8.0);
    assert_eq!(geometry.origin_x(), 52.0);
    assert_eq!(
        geometry.x_at_glyph(VisualTextGlyphIndex::new(1)),
        Some(64.0)
    );
    assert_eq!(geometry.x_at_slot(TextSlotColumn::new(1)), Some(64.0));
}
