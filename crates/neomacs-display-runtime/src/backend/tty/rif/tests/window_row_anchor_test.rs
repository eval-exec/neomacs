//! The sparse path must retain TTY matrix coordinates at non-unit metrics.
use super::*;

fn row(role: GlyphRowRole, ch: char) -> MatrixRow {
    let mut row = GlyphRow::new(role);
    // These GUI metrics deliberately disagree with terminal matrix indices.
    row.pixel_y = 100.0;
    row.height_px = 13.0;
    row.ascent_px = 10.0;
    row.glyphs[GlyphArea::Text.index()].push(Glyph::char(ch, FaceId::new(0), 0));
    MatrixRow::new(row)
}

#[test]
fn sparse_damage_keeps_fractional_window_origins_and_matrix_row_indices() {
    for (role, first_row, column) in [
        (
            GlyphRowRole::Text,
            TerminalGridRow(3),
            TerminalGridColumn(2),
        ),
        (
            GlyphRowRole::ModeLine,
            TerminalGridRow(1),
            TerminalGridColumn(0),
        ),
    ] {
        let mut state = FrameDisplayState::new(12, 24, 2.0, 3.0);
        let mut matrix = GlyphMatrix::new(3, 12);
        for (index, ch) in ['A', 'B', 'C'].into_iter().enumerate() {
            matrix.rows[index] = row(role, ch);
        }
        state.window_matrices.push(WindowMatrixEntry {
            window_id: DisplayWindowId::new(1),
            matrix,
            pixel_bounds: Rect::new(0.0, 1.5, 24.0, 24.0),
            text_pixel_bounds: Rect::new(4.0, 7.5, 20.0, 9.0),
            text_clip_bounds: None,
            selected: true,
        });
        let mut full = TtyRif::new(12, 24);
        full.set_damage_mode(damage::TtyDamageMode::Off);
        let mut sparse = TtyRif::new(12, 24);
        sparse.set_damage_mode(damage::TtyDamageMode::On);
        for rif in [&mut full, &mut sparse] {
            rif.rasterize(&state);
            rif.diff_and_render();
        }

        // Only the middle row changes. Both neighboring rows retain their
        // identities and must survive the early sparse-placement rejection.
        state.window_matrices[0].matrix.rows[1] = row(role, 'Z');
        for rif in [&mut full, &mut sparse] {
            rif.rasterize(&state);
            rif.diff_and_render();
        }
        assert!(sparse.frame_stats().damage_frame, "{role:?}");
        assert_eq!(sparse.frame_stats().rows_repainted, 1, "{role:?}");
        assert_eq!(sparse.current.cells, full.current.cells, "{role:?}");
        let first_row = usize::try_from(first_row.get()).unwrap();
        let column = usize::try_from(column.get()).unwrap();
        for (index, ch) in ['A', 'Z', 'C'].into_iter().enumerate() {
            assert_eq!(
                sparse.current.cells[(first_row + index) * 12 + column].ch(),
                ch,
                "{role:?}"
            );
        }
    }
}
