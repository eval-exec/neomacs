use super::*;
use crate::frame_glyphs::PhysCursor;
use crate::glyph_matrix::Glyph;
use crate::glyph_matrix::{GlyphArea, GlyphMatrix, MatrixRow, WindowMatrixEntry};
use crate::types::Color;

const CHAR_W: f32 = 8.0;
const CHAR_H: f32 = 16.0;
const ASCENT: f32 = 12.0;
const WINDOW: i64 = 7;

/// Three 16 px text rows `a`, `b`, `c` in one window whose text area starts
/// at y=20, as a header line would leave it.
fn three_row_state() -> FrameDisplayState {
    let mut state = FrameDisplayState::new(4, 4, CHAR_W, CHAR_H);
    let mut matrix = GlyphMatrix::new(3, 4);
    for (index, ch) in ['a', 'b', 'c'].into_iter().enumerate() {
        let row = MatrixRow::make_mut(&mut matrix.rows[index]);
        row.enabled = true;
        row.displays_text = true;
        row.role = GlyphRowRole::Text;
        row.pixel_y = index as f32 * CHAR_H;
        row.height_px = CHAR_H;
        row.ascent_px = ASCENT;
        row.start_charpos = index * 2 + 1;
        row.end_charpos = index * 2 + 3;
        row.glyphs[GlyphArea::Text.index()].push(Glyph::char(ch, FaceId::new(0), index * 2 + 1));
    }
    state.window_matrices.push(WindowMatrixEntry {
        window_id: DisplayWindowId::new(WINDOW),
        matrix,
        pixel_bounds: Rect::new(0.0, 0.0, 32.0, 68.0),
        text_pixel_bounds: Rect::new(0.0, 20.0, 32.0, 48.0),
        text_clip_bounds: None,
        selected: true,
    });
    state
}

fn cursor_on(row: usize, slot_row: u32) -> PhysCursor {
    PhysCursor {
        window_id: DisplayWindowId::new(WINDOW),
        charpos: row * 2 + 1,
        row,
        col: 0,
        slot_id: DisplaySlotId {
            window_id: DisplayWindowId::new(WINDOW),
            row: slot_row,
            col: 0,
        },
        x: 0.0,
        y: 20.0 + row as f32 * CHAR_H,
        width: CHAR_W,
        height: CHAR_H,
        ascent: ASCENT,
        style: CursorStyle::FilledBox,
        color: Color::WHITE,
        cursor_fg: Color::BLACK,
    }
}

fn capture(state: &FrameDisplayState) -> CursorDrawGeometry {
    let frame = state.materialize();
    CursorDrawGeometry::capture(state, &frame).expect("an active cursor")
}

#[test]
fn a_cursor_drawn_over_its_own_row_reports_the_row_band_and_glyph() {
    let mut state = three_row_state();
    state.phys_cursor = Some(cursor_on(1, 1));

    let geometry = capture(&state);

    let row = geometry.row.expect("the cursor row");
    assert_eq!(
        (row.top, row.height),
        (36.0, CHAR_H),
        "text top 20 + pixel_y 16"
    );
    assert_eq!((row.start_charpos, row.end_charpos), (3, 5));
    assert_eq!(row.damage, RowDamage::New);
    let drawn = geometry.drawn.expect("a drawn rect");
    assert_eq!(drawn.y, 36.0, "baseline 48 - ascent 12");
    assert!(matches!(
        geometry.slot_glyph,
        CursorSlotGlyph::Char {
            ch: 'b',
            baseline: 48.0,
            ..
        }
    ));
    assert_eq!(geometry.drawn_in_row(), Some(true));
    let line = geometry.to_string();
    for field in [
        "stage=draw",
        "window=7",
        "charpos=3",
        "layout=(row=1 col=0 slot=1,0 x=0.0 y=36.0 w=8.0 h=16.0 ascent=12.0)",
        "drawn=(0.0,36.0 8.0x16.0)",
        "drawn_in_row=true",
        "text_area=(0.0,20.0 32.0x48.0)",
        "row=(role=Text top=36.0 h=16.0 pixel_y=16.0 ascent=12.0 spacing=0.0 chars=3..5",
        "glyph=char('b' ",
        "frame_char=8.0x16.0",
    ] {
        assert!(line.contains(field), "{field:?} missing from {line}");
    }
}

/// The #855 shape: layout's cursor row and the slot the renderer resolves
/// disagree, so the drawn cursor lands one row below layout's rectangle.
#[test]
fn a_slot_on_the_next_row_is_reported_as_drawn_outside_the_cursor_row() {
    let mut state = three_row_state();
    state.phys_cursor = Some(cursor_on(1, 2));

    let geometry = capture(&state);

    assert_eq!(geometry.layout.y, 36.0);
    assert_eq!(geometry.drawn.map(|rect| rect.y), Some(52.0));
    assert!(matches!(
        geometry.slot_glyph,
        CursorSlotGlyph::Char { ch: 'c', .. }
    ));
    assert_eq!(geometry.drawn_in_row(), Some(false));
    assert!(geometry.to_string().contains("drawn_in_row=false"));
}

#[test]
fn a_frame_without_an_active_cursor_has_no_record() {
    let state = three_row_state();
    let frame = state.materialize();
    assert_eq!(CursorDrawGeometry::capture(&state, &frame), None);
}

#[test]
fn a_cursor_on_a_row_outside_the_matrix_says_so() {
    let mut state = three_row_state();
    state.phys_cursor = Some(cursor_on(5, 5));

    let geometry = capture(&state);

    assert_eq!(geometry.row, None);
    assert_eq!(geometry.slot_glyph, CursorSlotGlyph::Missing);
    assert_eq!(geometry.drawn_in_row(), None);
    let line = geometry.to_string();
    assert!(line.contains("row=missing"), "{line}");
    assert!(line.contains("glyph=missing"), "{line}");
    assert!(line.contains("drawn_in_row=unknown"), "{line}");
}
