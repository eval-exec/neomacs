//! One horizontal geometry for a row's visual TEXT_AREA glyphs.
//!
//! Cursor finalization uses glyph-array indices; replay uses materialized
//! columns. These are not interchangeable for wide glyphs, padding, or
//! compositions. Both mappings use the same origin and advances as paint.

use super::{Glyph, GlyphArea, GlyphRow, GlyphType, ResolvedBidiDirection};
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

/// Whether a geometry's advances are terminal cells or graphical pixels.
///
/// A terminal cell is one slot per character and a right-to-left run's visual
/// cells are its logical cells in reverse.  A GUI process lays a shaped RTL run
/// out as ONE composite glyph whose advance is the shaped run width, not
/// `cells * char_width`, so the within-glyph slot mirror below is only valid
/// for terminal geometry.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextGeometryUnits {
    /// Terminal cells: one slot per character cell.
    TerminalCells,
    /// Graphical pixels: advances are shaped glyph advances.
    #[default]
    Pixels,
}

/// A borrowed geometry view: the row cannot change while its mappings are
/// being consumed. Construct after bidi/decorations for final placement.
pub struct TextRowGeometry<'row> {
    row: &'row GlyphRow,
    char_width: f32,
    origin_x: f32,
    units: TextGeometryUnits,
}

impl<'row> TextRowGeometry<'row> {
    pub fn new(row: &'row GlyphRow, text_bounds: Rect, char_width: f32) -> Self {
        Self::new_with_units(row, text_bounds, char_width, TextGeometryUnits::Pixels)
    }

    pub fn new_with_units(
        row: &'row GlyphRow,
        text_bounds: Rect,
        char_width: f32,
        units: TextGeometryUnits,
    ) -> Self {
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
            units,
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

    /// Materialized slot of a glyph-array entry.
    ///
    /// `x_at_glyph` indexes the visually ordered glyph array; `x_at_slot`
    /// indexes materialized columns.  They are not interchangeable for wide
    /// glyphs, padding, or compositions, so the two newtypes convert here
    /// explicitly: sum the spans of the preceding non-padding glyphs.  A
    /// run-member padding maps to its position inside the owning run's span.
    pub fn slot_of_glyph(&self, index: VisualTextGlyphIndex) -> Option<TextSlotColumn> {
        let glyphs = &self.row.glyphs[GlyphArea::Text.index()];
        let target = usize::from(index.0);
        if target > glyphs.len() {
            return None;
        }
        let mut slot: u16 = 0;
        let mut owner: Option<(u16, u16)> = None;
        let mut pads_since_owner: u16 = 0;
        for (glyph_index, glyph) in glyphs.iter().enumerate() {
            if glyph_index == target {
                if glyph.padding
                    && let Some((owner_slot, span)) = owner
                {
                    let ordinal = pads_since_owner.saturating_add(1);
                    return Some(TextSlotColumn::new(
                        owner_slot.saturating_add(ordinal.min(span.saturating_sub(1))),
                    ));
                }
                return Some(TextSlotColumn::new(slot));
            }
            if glyph.padding {
                if owner.is_some() {
                    pads_since_owner = pads_since_owner.saturating_add(1);
                }
                continue;
            }
            let span = glyph.materialized_slot_span();
            owner = Some((slot, span));
            pads_since_owner = 0;
            slot = slot.saturating_add(span);
        }
        None
    }

    pub fn x_at_slot(&self, target: TextSlotColumn) -> Option<f32> {
        let mut x = self.origin_x;
        let mut col = 0_u16;
        for glyph in &self.row.glyphs[GlyphArea::Text.index()] {
            if glyph.padding {
                continue;
            }
            let span = glyph.materialized_slot_span();
            let next_col = col.saturating_add(span);
            if target.0 < next_col {
                // A TERMINAL run resolved right-to-left is stored with its
                // logical cells running into its visual cells backwards: the
                // first source character's terminal cell is the run's LAST
                // visual cell (GNU's terminal prepend, src/term.c:1563-1585,
                // and set_cursor_from_row's reversed-row rule).  A cursor
                // inside such a run therefore lands on the mirrored cell, not
                // on the glyph's left edge.  GUI geometry keeps the run's
                // left edge: its advance is the shaped cluster width, so a
                // char_width-grid offset would be a third wrong place.
                let within = target.0.saturating_sub(col);
                if self.units == TextGeometryUnits::TerminalCells
                    && glyph.resolved_bidi_direction() == ResolvedBidiDirection::RightToLeft
                    && within < span
                    && let Some(offset) = self.rtl_within_glyph_offset(glyph, within)
                {
                    return Some(x + offset);
                }
                return Some(x);
            }
            // A non-padding glyph always spans at least one slot, but keep the
            // zero-span ownership rule explicit: a glyph whose span is zero
            // owns its own column, so the cursor does not jump past it.
            if target.0 <= col {
                return Some(x);
            }
            x += glyph.materialized_pixel_advance(self.char_width);
            col = next_col;
        }
        (target.0 == col).then_some(x)
    }

    /// Pixel offset of the `within`-th logical cell inside a right-to-left
    /// glyph.  Automatic compositions carry their own cell widths; every other
    /// glyph is one cell per slot.
    fn rtl_within_glyph_offset(&self, glyph: &Glyph, within: u16) -> Option<f32> {
        let widths: Vec<u16> = match &glyph.glyph_type {
            GlyphType::AutomaticComposite { terminal, .. } => terminal
                .cells
                .iter()
                .map(|cell| u16::from(cell.width_cols.max(1)))
                .collect(),
            _ => vec![1; usize::from(glyph.materialized_slot_span())],
        };
        let within = usize::from(within);
        if widths.is_empty() || within >= widths.len() {
            return None;
        }
        // The run's visual cells are its logical cells in reverse order: the
        // logical cell at `within` sits after every LOGICAL cell that follows
        // it, so its x offset is the sum of those trailing widths.
        let offset: u16 = widths[within + 1..].iter().sum();
        Some(f32::from(offset) * self.char_width)
    }
}

#[cfg(test)]
#[path = "text_geometry/tests/text_geometry_test.rs"]
mod tests;
