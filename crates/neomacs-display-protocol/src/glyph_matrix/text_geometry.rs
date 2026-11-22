//! One horizontal geometry for a row's visual TEXT_AREA glyphs.
//!
//! Cursor finalization uses glyph-array indices; replay uses materialized
//! columns. These are not interchangeable for wide glyphs, padding, or
//! compositions. Both mappings use the same origin and advances as paint.
//!
//! Terminal cells are not the same coordinate space as graphical pixels.  A
//! terminal row's cell accounting is owned by [`super::terminal_text_area_cells`]
//! (the same rules the TTY rasterizer consumes); a GUI row measures shaped
//! glyph advances.  The caller must therefore say which units the row is in,
//! and terminal geometry must be given GNU's `CHARACTER_WIDTH` function.

use super::{
    Glyph, GlyphArea, GlyphRow, GlyphType, ResolvedBidiDirection, TerminalCharWidths,
    terminal_glyph_cells, terminal_text_area_cells,
};
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
    /// GNU `CHARACTER_WIDTH`; only read for [`TextGeometryUnits::TerminalCells`].
    terminal_widths: TerminalCharWidths,
}

/// Pixel geometry never consults the char-width table.
fn pixels_only(_ch: char) -> usize {
    1
}

impl<'row> TextRowGeometry<'row> {
    pub fn new(row: &'row GlyphRow, text_bounds: Rect, char_width: f32) -> Self {
        Self::new_with_units(
            row,
            text_bounds,
            char_width,
            TextGeometryUnits::Pixels,
            TerminalCharWidths::new(pixels_only),
        )
    }

    /// Build a geometry for `units`.
    ///
    /// `terminal_widths` must be GNU's `CHARACTER_WIDTH` table for
    /// [`TextGeometryUnits::TerminalCells`]; it is ignored for
    /// [`TextGeometryUnits::Pixels`].  Supplying the same function the TTY
    /// rasterizer uses is what keeps a right-aligned terminal row's origin and
    /// its painted cells equal for zero-width characters and composite run
    /// members.
    pub fn new_with_units(
        row: &'row GlyphRow,
        text_bounds: Rect,
        char_width: f32,
        units: TextGeometryUnits,
        terminal_widths: TerminalCharWidths,
    ) -> Self {
        let char_width = char_width.max(1.0);
        let text_glyphs = &row.glyphs[GlyphArea::Text.index()];
        let offset = if row.reversed_p {
            match units {
                TextGeometryUnits::TerminalCells => {
                    // Match the rasterizer's column arithmetic exactly: it
                    // rounds the area to whole cells before subtracting the
                    // cells the row occupies.  A zero-width RLM/LRM or a
                    // composite's run-member padding must shift both the same
                    // way, so both sides read terminal_text_area_cells.
                    let area_cells = (text_bounds.width / char_width).round().max(0.0);
                    let used_cells =
                        f32::from(terminal_text_area_cells(text_glyphs, terminal_widths));
                    (area_cells - used_cells).max(0.0) * char_width
                }
                TextGeometryUnits::Pixels => {
                    let used_width: f32 = text_glyphs
                        .iter()
                        .filter(|glyph| !glyph.padding)
                        .map(|glyph| glyph.materialized_pixel_advance(char_width))
                        .sum();
                    (text_bounds.width - used_width).max(0.0)
                }
            }
        } else {
            row.pixel_x.max(0.0)
        };
        Self {
            row,
            char_width,
            origin_x: text_bounds.x + offset,
            units,
            terminal_widths,
        }
    }

    pub fn origin_x(&self) -> f32 {
        self.origin_x
    }

    pub fn x_at_glyph(&self, index: VisualTextGlyphIndex) -> Option<f32> {
        let glyphs = &self.row.glyphs[GlyphArea::Text.index()];
        let target = usize::from(index.0);
        glyphs.get(..target)?;
        match self.units {
            TextGeometryUnits::Pixels => Some(
                self.origin_x
                    + glyphs[..target]
                        .iter()
                        .filter(|glyph| !glyph.padding)
                        .map(|glyph| glyph.materialized_pixel_advance(self.char_width))
                        .sum::<f32>(),
            ),
            TextGeometryUnits::TerminalCells => {
                // A run's following padding entries are owned by the base
                // glyph; a target inside that run shares the run's start.
                let mut x = self.origin_x;
                let mut glyph_index = 0usize;
                while glyph_index < target {
                    let glyph_cells =
                        terminal_glyph_cells(glyphs, glyph_index, self.terminal_widths)?;
                    let consumed = glyph_cells.consumed_paddings.saturating_add(1);
                    if glyph_index.saturating_add(consumed) > target {
                        break;
                    }
                    x += f32::from(glyph_cells.cells) * self.char_width;
                    glyph_index += consumed;
                }
                Some(x)
            }
        }
    }

    /// Materialized slot of a glyph-array entry.
    ///
    /// `x_at_glyph` indexes the visually ordered glyph array; `x_at_slot`
    /// indexes materialized columns.  They are not interchangeable for wide
    /// glyphs, padding, or compositions, so the two newtypes convert here
    /// explicitly.  Terminal cells use the rasterizer's own per-glyph spans
    /// (zero for a zero-width character, the whole run for a composite); GUI
    /// pixels sum their shaped advances.
    pub fn slot_of_glyph(&self, index: VisualTextGlyphIndex) -> Option<TextSlotColumn> {
        let glyphs = &self.row.glyphs[GlyphArea::Text.index()];
        let target = usize::from(index.0);
        if target > glyphs.len() {
            return None;
        }
        match self.units {
            TextGeometryUnits::Pixels => {
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
            TextGeometryUnits::TerminalCells => {
                let mut slot: u16 = 0;
                let mut glyph_index = 0usize;
                while glyph_index < glyphs.len() {
                    let glyph_cells =
                        terminal_glyph_cells(glyphs, glyph_index, self.terminal_widths)?;
                    let consumed = glyph_cells.consumed_paddings.saturating_add(1);
                    let run_end = glyph_index.saturating_add(consumed);
                    if target == glyph_index {
                        return Some(TextSlotColumn::new(slot));
                    }
                    if target > glyph_index && target < run_end {
                        // A run member is addressed by its LOGICAL ordinal
                        // inside the run; x_at_slot mirrors that ordinal into
                        // the run's visual cells for an RTL glyph, exactly as
                        // the rasterizer reverses the graphemes it paints.
                        let ordinal = u16::try_from(target - glyph_index).ok()?;
                        return Some(TextSlotColumn::new(
                            slot.saturating_add(ordinal.min(glyph_cells.cells.saturating_sub(1))),
                        ));
                    }
                    slot = slot.saturating_add(glyph_cells.cells);
                    glyph_index = run_end;
                }
                (target == glyphs.len()).then_some(TextSlotColumn::new(slot))
            }
        }
    }

    pub fn x_at_slot(&self, target: TextSlotColumn) -> Option<f32> {
        let glyphs = &self.row.glyphs[GlyphArea::Text.index()];
        match self.units {
            TextGeometryUnits::Pixels => {
                let mut x = self.origin_x;
                let mut col = 0_u16;
                for glyph in glyphs {
                    if glyph.padding {
                        continue;
                    }
                    let span = glyph.materialized_slot_span();
                    let next_col = col.saturating_add(span);
                    if target.0 < next_col {
                        return Some(x);
                    }
                    // A non-padding glyph always spans at least one slot, but
                    // keep the zero-span ownership rule explicit: a glyph
                    // whose span is zero owns its own column, so the cursor
                    // does not jump past it.
                    if target.0 <= col {
                        return Some(x);
                    }
                    x += glyph.materialized_pixel_advance(self.char_width);
                    col = next_col;
                }
                (target.0 == col).then_some(x)
            }
            TextGeometryUnits::TerminalCells => {
                let mut x = self.origin_x;
                let mut col = 0_u16;
                let mut glyph_index = 0usize;
                while let Some(glyph_cells) =
                    terminal_glyph_cells(glyphs, glyph_index, self.terminal_widths)
                {
                    let glyph = &glyphs[glyph_index];
                    let span = glyph_cells.cells;
                    let next_col = col.saturating_add(span);
                    if target.0 < next_col {
                        // A TERMINAL run resolved right-to-left is stored with
                        // its logical cells running into its visual cells
                        // backwards: the first source character's terminal
                        // cell is the run's LAST visual cell (GNU's terminal
                        // prepend, src/term.c:1563-1585, and
                        // set_cursor_from_row's reversed-row rule).  A cursor
                        // inside such a run therefore lands on the mirrored
                        // cell, not on the glyph's left edge.  GUI geometry
                        // keeps the run's left edge: its advance is the shaped
                        // cluster width, so a char_width-grid offset would be
                        // a third wrong place.
                        let within = target.0.saturating_sub(col);
                        if glyph.resolved_bidi_direction() == ResolvedBidiDirection::RightToLeft
                            && within < span
                            && let Some(offset) = self.rtl_within_glyph_offset(glyph, within, span)
                        {
                            return Some(x + offset);
                        }
                        return Some(x);
                    }
                    // A zero-width character owns its own column without
                    // advancing: the cursor stays at the boundary.
                    if target.0 <= col {
                        return Some(x);
                    }
                    x += f32::from(span) * self.char_width;
                    col = next_col;
                    glyph_index += 1 + glyph_cells.consumed_paddings;
                }
                (target.0 == col).then_some(x)
            }
        }
    }

    /// Pixel offset of the `within`-th logical terminal cell inside a
    /// right-to-left glyph.  Automatic compositions carry their own cell
    /// widths; a contextual-shaping run and every other glyph is one cell per
    /// slot.
    fn rtl_within_glyph_offset(&self, glyph: &Glyph, within: u16, span: u16) -> Option<f32> {
        let widths: Vec<u16> = match &glyph.glyph_type {
            GlyphType::AutomaticComposite { terminal, .. } => terminal
                .cells
                .iter()
                .map(|cell| u16::from(cell.width_cols.max(1)))
                .collect(),
            _ => vec![1; usize::from(span)],
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
