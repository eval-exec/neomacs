use super::*;
use neomacs_display_protocol::glyph_matrix::{
    Glyph, GlyphArea, GlyphMatrix, GlyphRow, MatrixRow, WindowMatrixEntry,
};
use neomacs_display_protocol::posn_object_extent::{PosnMatrixSnapshot, PosnObjectExtent};
use neomacs_display_protocol::{DisplayWindowId, FaceId, GlyphRowRole, Rect};
use neovm_core::window::{WindowDisplaySnapshot, WindowId};

fn completed_state() -> FrameDisplayState {
    let mut state = FrameDisplayState::new(120, 40, 1.0, 1.0);
    let mut matrix = GlyphMatrix::new(8, 12);
    for (index, role) in [
        (0, GlyphRowRole::TabLine),
        (1, GlyphRowRole::HeaderLine),
        (2, GlyphRowRole::Text),
        (7, GlyphRowRole::ModeLine),
    ] {
        let row = MatrixRow::make_mut(&mut matrix.rows[index]);
        *row = GlyphRow::new(role);
        row.enabled = true;
        row.mode_line = role != GlyphRowRole::Text;
        row.pixel_y = index as f32;
        row.height_px = 1.0;
        row.ascent_px = 1.0;
        row.glyphs[GlyphArea::Text.index()] = vec![Glyph::char('x', FaceId::new(0), 0)];
        row.glyphs[GlyphArea::LeftMargin.index()] = vec![Glyph::char('L', FaceId::new(0), 0)];
    }
    // An enabled surplus Text row in a protocol matrix cannot become a GNU
    // visible row outside the TTY adapter's canonical text clip.
    let mut surplus = GlyphRow::new(GlyphRowRole::Text);
    surplus.enabled = true;
    surplus.pixel_y = 6.0;
    surplus.height_px = 1.0;
    matrix.rows[6] = MatrixRow::new(surplus);
    state.window_matrices.push(WindowMatrixEntry {
        window_id: DisplayWindowId::new(3),
        matrix,
        pixel_bounds: Rect::new(40.0, 20.0, 12.0, 8.0),
        text_pixel_bounds: Rect::new(42.0, 20.0, 8.0, 8.0),
        text_clip_bounds: Some(Rect::new(42.0, 22.0, 8.0, 4.0)),
        selected: false,
    });
    state
}

#[test]
fn finalized_body_chrome_and_implicit_tail_capture_window_local_origins() {
    let state = completed_state();
    let mut publications = vec![WindowPresentationSnapshot::live(WindowDisplaySnapshot {
        window_id: WindowId(3),
        ..Default::default()
    })];
    capture_terminal_object_extents(&state, &mut publications);
    let snapshot = publications[0].display_snapshot();
    let captured = snapshot.posn_matrix.as_deref().unwrap();
    assert_eq!(captured.rows[0].y, 0);
    assert_eq!(captured.rows[1].y, 1);
    assert_eq!(captured.rows[2].y, 2);
    assert_eq!(captured.rows[7].y, 7);
    // The old raw content is one cell. A live stale walk reaching col7 must
    // still see the terminal's used default-face tail, not row fallback.
    assert_eq!(captured.at(2, 7, GlyphArea::Text).dimensions(), (1, 0));
    assert_eq!(captured.at(2, 8, GlyphArea::Text).dimensions(), (0, 1));
    assert_eq!(captured.at(7, 11, GlyphArea::Text).dimensions(), (1, 0));
    assert_eq!(captured.at(7, 12, GlyphArea::Text).dimensions(), (0, 1));
    assert_eq!(
        captured.at_y(2, 0, GlyphArea::LeftMargin).dimensions(),
        (1, 0)
    );
    assert_eq!(
        captured.at_y(22, 0, GlyphArea::LeftMargin),
        PosnObjectExtent::Undrawn
    );
    assert_eq!(
        captured.at(3, 0, GlyphArea::Text),
        PosnObjectExtent::Undrawn
    );
    assert_eq!(
        captured.at(6, 0, GlyphArea::Text),
        PosnObjectExtent::Undrawn
    );
}

#[test]
fn terminal_tail_owns_same_cells_for_default_and_nondefault_source_faces() {
    let mut state = completed_state();
    let before = PosnMatrixSnapshot::from_terminal_window(
        &state,
        &state.window_matrices[0],
        neovm_core::encoding::char_width,
    );
    let row = MatrixRow::make_mut(&mut state.window_matrices[0].matrix.rows[2]);
    row.glyphs[GlyphArea::Text.index()][0].face_id = FaceId::new(42);
    let after = PosnMatrixSnapshot::from_terminal_window(
        &state,
        &state.window_matrices[0],
        neovm_core::encoding::char_width,
    );
    assert_eq!(before, after);
    assert_eq!(after.at(2, 7, GlyphArea::Text).dimensions(), (1, 0));
    // A reserved right-border cell changes the TEXT fill boundary; it is
    // separately materialized in the right area, not counted as text glyph.
    MatrixRow::make_mut(&mut state.window_matrices[0].matrix.rows[2]).glyphs
        [GlyphArea::RightMargin.index()]
    .push(Glyph::char('|', FaceId::new(0), 0));
    let bordered = PosnMatrixSnapshot::from_terminal_window(
        &state,
        &state.window_matrices[0],
        neovm_core::encoding::char_width,
    );
    assert_eq!(bordered.at(2, 6, GlyphArea::Text).dimensions(), (1, 0));
    assert_eq!(bordered.at(2, 7, GlyphArea::Text).dimensions(), (0, 1));
    assert_eq!(
        bordered.at(2, 0, GlyphArea::RightMargin).dimensions(),
        (1, 0)
    );
}
