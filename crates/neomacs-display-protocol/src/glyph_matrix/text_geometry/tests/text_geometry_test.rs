use super::*;
use crate::frame_glyphs::GlyphRowRole;
use crate::glyph_matrix::Glyph;
use crate::types::FaceId;

/// The test stand-in for GNU `CHARACTER_WIDTH`: RLM/LRM and the other bidi
/// formatting controls are zero-width; everything used here is one cell.
fn terminal_widths() -> TerminalCharWidths {
    fn cells(ch: char) -> usize {
        match ch as u32 {
            0x200B..=0x200F | 0x202A..=0x202E | 0x2060..=0x206F => 0,
            _ => 1,
        }
    }
    TerminalCharWidths::new(cells)
}

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

/// The first source character of a right-to-left run sits in the run's LAST
/// visual cell, so a cursor inside the run must land there, not on the run's
/// left edge (GNU's terminal prepend and set_cursor_from_row's reversed-row
/// rule).  The row here is the reporter's shape: a line-end blank followed by
/// the Arabic automatic composition.
#[test]
fn rtl_composite_slot_uses_the_mirrored_visual_cell() {
    use crate::glyph_matrix::{GlyphType, TerminalComposition, TerminalCompositionCell};
    use std::sync::Arc;

    let cell = |base: char| TerminalCompositionCell {
        base,
        extenders: "".into(),
        width_cols: 1,
        source_char_len: 1,
    };
    let terminal = Arc::new(TerminalComposition {
        cells: vec![cell('\u{062c}'), cell('\u{0627}')].into(),
        width_cols: 2,
    });
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    row.reversed_p = true;
    row.glyphs[GlyphArea::Text.index()] = vec![
        Glyph::char(' ', FaceId::new(0), 0).with_pixel_width(8.0),
        Glyph {
            glyph_type: GlyphType::AutomaticComposite {
                text: "\u{062c}\u{0627}".into(),
                terminal,
            },
            face_id: FaceId::new(0),
            box_vertical_edges: Default::default(),
            provenance: crate::glyph_matrix::GlyphProvenance::buffer(0),
            bidi_level: 1,
            wide: false,
            pixel_width: 16.0,
            pixel_height: 0.0,
            pixel_ascent: 0.0,
            vertical_offset_px: 0.0,
            padding: false,
            pointer_appearance: None,
        },
    ];
    let geometry = TextRowGeometry::new_with_units(
        &row,
        Rect::new(0.0, 0.0, 80.0, 16.0),
        8.0,
        TextGeometryUnits::TerminalCells,
        terminal_widths(),
    );
    // used = 8 (line-end blank) + 16 (two-cell run) -> origin 56.
    assert_eq!(geometry.origin_x(), 56.0);
    assert_eq!(geometry.x_at_slot(TextSlotColumn::new(0)), Some(56.0));
    // First logical character = rightmost cell of the run.
    assert_eq!(geometry.x_at_slot(TextSlotColumn::new(1)), Some(72.0));
    // Second logical character = the run's left cell.
    assert_eq!(geometry.x_at_slot(TextSlotColumn::new(2)), Some(64.0));

    // The GUI keeps the run's left edge for every slot: its shaped advance is
    // not a char-width grid (N-gui-rtl-cursor).
    let pixels = TextRowGeometry::new(&row, Rect::new(0.0, 0.0, 80.0, 16.0), 8.0);
    assert_eq!(pixels.x_at_slot(TextSlotColumn::new(1)), Some(64.0));
    assert_eq!(pixels.x_at_slot(TextSlotColumn::new(2)), Some(64.0));
}

/// A zero-span glyph (a composition padding member) never owns a cursor slot,
/// even when layout gave the padding a pixel advance; the cursor belongs to
/// the next real glyph.  This pins the `target <= col` ownership rule that the
/// mirrored-slot change keeps for zero-span glyphs.
#[test]
fn zero_span_padding_glyph_does_not_own_a_cursor_slot() {
    let mut pad = Glyph::padding_for(FaceId::new(0), 0);
    pad.pixel_width = 8.0;
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    // TextRowGeometry derives the right-aligned origin (width minus the
    // non-padding advances) only for a reversed row; the zero-span ownership
    // rule under test is exercised on that right-aligned row.
    row.reversed_p = true;
    row.glyphs[GlyphArea::Text.index()] = vec![
        pad,
        Glyph::char('a', FaceId::new(0), 1).with_pixel_width(8.0),
    ];
    let geometry = TextRowGeometry::new(&row, Rect::new(0.0, 0.0, 80.0, 16.0), 8.0);
    assert_eq!(geometry.origin_x(), 72.0);
    assert_eq!(geometry.x_at_slot(TextSlotColumn::new(0)), Some(72.0));
    assert_eq!(geometry.x_at_slot(TextSlotColumn::new(1)), Some(80.0));
}

/// Glyph-array indices and materialized slots are different coordinate spaces:
/// a Stretch (tab), image, wide glyph or multi-cell composition makes them
/// diverge.  The TTY cursor remap passes a glyph index, so it must convert
/// through `slot_of_glyph` first; feeding the raw index to `x_at_slot` lands
/// inside the tab (the v4 regression this row pins).
#[test]
fn glyph_index_converts_to_materialized_slot_across_a_stretch() {
    use crate::glyph_matrix::GlyphType;

    let mut row = GlyphRow::new(GlyphRowRole::Text);
    row.reversed_p = true;
    row.glyphs[GlyphArea::Text.index()] = vec![
        Glyph::char('a', FaceId::new(0), 0).with_pixel_width(8.0),
        Glyph {
            glyph_type: GlyphType::Stretch { width_cols: 4 },
            face_id: FaceId::new(0),
            box_vertical_edges: Default::default(),
            provenance: crate::glyph_matrix::GlyphProvenance::buffer(1),
            bidi_level: 0,
            wide: false,
            pixel_width: 32.0,
            pixel_height: 0.0,
            pixel_ascent: 0.0,
            vertical_offset_px: 0.0,
            padding: false,
            pointer_appearance: None,
        },
        Glyph::char('b', FaceId::new(0), 2).with_pixel_width(8.0),
    ];
    let geometry = TextRowGeometry::new_with_units(
        &row,
        Rect::new(0.0, 0.0, 80.0, 16.0),
        8.0,
        TextGeometryUnits::TerminalCells,
        terminal_widths(),
    );
    let cursor_glyph = VisualTextGlyphIndex::new(2);
    let x_at_glyph = geometry.x_at_glyph(cursor_glyph).expect("cursor glyph");
    let slot = geometry
        .slot_of_glyph(cursor_glyph)
        .expect("cursor glyph has a slot");
    assert_eq!(slot, TextSlotColumn::new(5));
    assert_eq!(geometry.x_at_slot(slot), Some(x_at_glyph));
    assert_ne!(
        geometry.x_at_slot(TextSlotColumn::new(2)),
        Some(x_at_glyph),
        "the raw glyph index must not be used as a slot"
    );
}

/// A zero-width character consumes no terminal cell, so it must not shift a
/// right-aligned row's origin.  Before the shared cell rule, the terminal
/// geometry counted it as one cell (the rasterizer counted zero), and the
/// cursor origin sat a column left of the painted text.
#[test]
fn terminal_origin_ignores_zero_width_characters_like_the_rasterizer() {
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    row.reversed_p = true;
    row.glyphs[GlyphArea::Text.index()] = vec![
        Glyph::char('\u{200f}', FaceId::new(0), 0), // RLM
        Glyph::char('a', FaceId::new(0), 1).with_pixel_width(8.0),
    ];
    let geometry = TextRowGeometry::new_with_units(
        &row,
        Rect::new(0.0, 0.0, 80.0, 16.0),
        8.0,
        TextGeometryUnits::TerminalCells,
        terminal_widths(),
    );
    // One painted cell (the RLM writes nothing) in an 80px row: 72.
    assert_eq!(geometry.origin_x(), 72.0);
    assert_eq!(geometry.x_at_slot(TextSlotColumn::new(0)), Some(72.0));
    assert_eq!(geometry.x_at_slot(TextSlotColumn::new(1)), Some(80.0));
    // The RLM shares the 'a' cell, so the glyph-array mapping agrees.
    assert_eq!(
        geometry.x_at_glyph(VisualTextGlyphIndex::new(1)),
        Some(72.0)
    );
    assert_eq!(
        geometry.slot_of_glyph(VisualTextGlyphIndex::new(1)),
        Some(TextSlotColumn::new(0))
    );
}

/// A contextual-shaping run is a base Composite plus one run-member padding
/// per following grapheme; the terminal paints one cell per grapheme, so the
/// run's materialized span is its cell count, not one.  An RTL run paints its
/// graphemes visually reversed, and each source character's cursor slot must
/// land on the cell the rasterizer writes for it.
#[test]
fn terminal_composite_run_members_own_their_painted_cells() {
    let padding = || {
        let mut glyph = Glyph::char('\u{0628}', FaceId::new(0), 1);
        glyph.padding = true;
        glyph
    };
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    row.reversed_p = true;
    row.glyphs[GlyphArea::Text.index()] = vec![
        Glyph {
            glyph_type: GlyphType::Composite {
                text: "\u{0627}\u{0628}".into(),
            },
            face_id: FaceId::new(0),
            box_vertical_edges: Default::default(),
            provenance: crate::glyph_matrix::GlyphProvenance::buffer(0),
            bidi_level: 1,
            wide: false,
            pixel_width: 0.0,
            pixel_height: 0.0,
            pixel_ascent: 0.0,
            vertical_offset_px: 0.0,
            padding: false,
            pointer_appearance: None,
        },
        padding(),
    ];
    let geometry = TextRowGeometry::new_with_units(
        &row,
        Rect::new(0.0, 0.0, 80.0, 16.0),
        8.0,
        TextGeometryUnits::TerminalCells,
        terminal_widths(),
    );
    // Two painted cells, so the right-aligned origin is 80 - 16 = 64.
    assert_eq!(geometry.origin_x(), 64.0);
    // The base Composite holds the first source character (alef); an RTL run
    // paints it in the run's rightmost cell.
    assert_eq!(
        geometry.slot_of_glyph(VisualTextGlyphIndex::new(0)),
        Some(TextSlotColumn::new(0))
    );
    assert_eq!(geometry.x_at_slot(TextSlotColumn::new(0)), Some(72.0));
    // The run-member padding is the second source character (beh), painted in
    // the run's left cell.
    assert_eq!(
        geometry.slot_of_glyph(VisualTextGlyphIndex::new(1)),
        Some(TextSlotColumn::new(1))
    );
    assert_eq!(geometry.x_at_slot(TextSlotColumn::new(1)), Some(64.0));
}
