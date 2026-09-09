//! One horizontal geometry for a row's visual TEXT_AREA glyphs.
//!
//! Cursor finalization uses glyph-array indices; replay uses materialized
//! columns. These are not interchangeable for wide glyphs, padding, or
//! compositions. Both mappings use the same origin and advances as paint.

use super::{GlyphArea, GlyphRow};
use crate::types::Rect;

/// Index in the visually ordered TEXT_AREA array, including padding entries.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VisualTextGlyphIndex(u16);

impl VisualTextGlyphIndex {
    pub const fn new(index: u16) -> Self {
        Self(index)
    }
}

/// Materialized column relative to TEXT_AREA, excluding structural margins.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TextSlotColumn(u16);

impl TextSlotColumn {
    pub const fn new(column: u16) -> Self {
        Self(column)
    }
}

/// A borrowed geometry view: the row cannot change while its mappings are
/// being consumed. Construct after bidi/decorations for final placement.
pub struct TextRowGeometry<'row> {
    row: &'row GlyphRow,
    char_width: f32,
    origin_x: f32,
}

impl<'row> TextRowGeometry<'row> {
    pub fn new(row: &'row GlyphRow, text_bounds: Rect, char_width: f32) -> Self {
        let char_width = char_width.max(1.0);
        let offset = if row.reversed_p {
            let used_width: f32 = row.glyphs[GlyphArea::Text.index()]
                .iter()
                .filter(|glyph| !glyph.padding)
                .map(|glyph| glyph.materialized_pixel_advance(char_width))
                .sum();
            (text_bounds.width - used_width).max(0.0)
        } else {
            row.pixel_x.max(0.0)
        };
        Self {
            row,
            char_width,
            origin_x: text_bounds.x + offset,
        }
    }

    pub fn origin_x(&self) -> f32 {
        self.origin_x
    }

    pub fn x_at_glyph(&self, index: VisualTextGlyphIndex) -> Option<f32> {
        let preceding = self.row.glyphs[GlyphArea::Text.index()].get(..usize::from(index.0))?;
        Some(
            self.origin_x
                + preceding
                    .iter()
                    .filter(|glyph| !glyph.padding)
                    .map(|glyph| glyph.materialized_pixel_advance(self.char_width))
                    .sum::<f32>(),
        )
    }

    pub fn x_at_slot(&self, target: TextSlotColumn) -> Option<f32> {
        let mut x = self.origin_x;
        let mut col = 0_u16;
        for glyph in &self.row.glyphs[GlyphArea::Text.index()] {
            if glyph.padding {
                continue;
            }
            let next_col = col.saturating_add(glyph.materialized_slot_span());
            if target.0 <= col || target.0 < next_col {
                return Some(x);
            }
            x += glyph.materialized_pixel_advance(self.char_width);
            col = next_col;
        }
        (target.0 == col).then_some(x)
    }
}

#[cfg(test)]
#[path = "text_geometry_test.rs"]
mod tests;
