use super::*;

fn pool() -> Arc<PosnFramePool> {
    Arc::new(PosnFramePool {
        columns: 12,
        lines: 5,
        partitions: Vec::new(),
        rows: (0..5)
            .map(|_| {
                let mut cells = vec![
                    PosnObjectExtent::Glyph {
                        width: 0,
                        height: 0,
                    };
                    12
                ];
                // GNU append_space_for_newline on a blank EOB row.
                cells[0] = PosnObjectExtent::Glyph {
                    width: 1,
                    height: 0,
                };
                PosnFramePoolRow {
                    enabled: true,
                    height: 1,
                    used_text: 12,
                    cells: Arc::from(cells),
                }
            })
            .collect(),
    })
}

fn lower(pool: Arc<PosnFramePool>) -> PosnFramePoolSlice {
    PosnFramePoolSlice {
        pool,
        first_row: 2,
        row_count: 3,
        first_columns: [0; GlyphArea::COUNT],
        used: [0, 12, 0],
    }
}

#[test]
fn cold_child_reads_exact_accepted_zero_cells_inside_full_frame_partition() {
    let accepted = pool();
    let child = lower(accepted.clone());
    assert_eq!(accepted.rows[2].used_text, 12);
    assert_eq!(child.at(0, 0, GlyphArea::Text).dimensions(), (1, 0));
    // Used count12 describes both complete partitions. An explicit
    // zero-metric space glyph is a stored cell, not a row-height fallback.
    assert_eq!(child.at(0, 1, GlyphArea::Text).dimensions(), (0, 0));
    assert_eq!(child.at(1, 3, GlyphArea::Text).dimensions(), (0, 0));
    assert_eq!(child.at(2, 0, GlyphArea::Text).dimensions(), (1, 0));
    assert_eq!(child.at(2, 12, GlyphArea::Text).dimensions(), (0, 1));
    assert!(Arc::ptr_eq(&child.pool, &accepted));
}

#[test]
fn disabled_or_missing_pool_rows_never_infer_drawn_padding() {
    let accepted = pool();
    let mut next = (*accepted).clone();
    next.rows[3].enabled = false;
    let child = lower(Arc::new(next));
    assert_eq!(child.at(1, 0, GlyphArea::Text), PosnObjectExtent::Undrawn);
    assert_eq!(child.at(3, 0, GlyphArea::Text), PosnObjectExtent::Undrawn);
    assert_eq!(child.at(-1, 0, GlyphArea::Text), PosnObjectExtent::Undrawn);
}

#[test]
fn glyph_metrics_row_fallback_and_captured_pool_are_independent() {
    let accepted = pool();
    let old = lower(accepted.clone());
    let mut next = (*accepted).clone();
    next.rows[2].height = 7;
    let mut cells = next.rows[2].cells.to_vec();
    cells[1] = PosnObjectExtent::Specialized {
        width: 2,
        height: 3,
    };
    next.rows[2].cells = Arc::from(cells);
    let new = lower(Arc::new(next));
    assert_eq!(old.at(0, 1, GlyphArea::Text).dimensions(), (0, 0));
    assert_eq!(new.at(0, 1, GlyphArea::Text).dimensions(), (2, 3));
    assert_eq!(new.at(0, 12, GlyphArea::Text).dimensions(), (0, 7));
    assert_eq!(old.at(0, 12, GlyphArea::Text).dimensions(), (0, 1));
}

#[test]
fn terminal_eob_rows_have_full_used_counts_and_reused_exact_metrics() {
    use crate::glyph_matrix::{
        FrameDisplayState, GlyphMatrix, GlyphRow, MatrixRow, WindowMatrixEntry,
    };
    use crate::{DisplayWindowId, GlyphRowRole, Rect};
    let mut state = FrameDisplayState::new(12, 5, 1.0, 1.0);
    let mut matrix = GlyphMatrix::new(5, 12);
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    row.enabled = true;
    row.ends_at_zv = true;
    row.height_px = 3.0;
    matrix.rows[0] = MatrixRow::new(row);
    state.window_matrices.push(WindowMatrixEntry {
        window_id: DisplayWindowId::new(1),
        matrix,
        pixel_bounds: Rect::new(0.0, 0.0, 12.0, 5.0),
        text_pixel_bounds: Rect::new(0.0, 0.0, 12.0, 5.0),
        text_clip_bounds: Some(Rect::new(0.0, 0.0, 12.0, 5.0)),
        selected: true,
    });
    let first = PosnFramePool::from_terminal_frame(&state, None, |_| 1);
    assert_eq!(first.rows[0].height, 3);
    assert_eq!(first.rows[0].used_text, 12);
    assert_eq!(first.rows[0].cells[0].dimensions(), (1, 0));
    assert_eq!(first.rows[0].cells[1].dimensions(), (0, 0));
    assert_eq!(first.rows[4].cells[0].dimensions(), (1, 0));
    assert_eq!(first.rows[4].cells[7].dimensions(), (0, 0));
    let mut old = first.clone();
    Arc::make_mut(&mut old.rows[4].cells)[7] = PosnObjectExtent::Specialized {
        width: 2,
        height: 3,
    };
    state.window_matrices[0]
        .matrix
        .set_row_damage(0, crate::glyph_matrix::RowDamage::Reused);
    let next = PosnFramePool::from_terminal_frame(&state, Some(&old), |_| 1);
    assert!(next.retains_same_observations(&old));
    for (new, old) in next.rows.iter().zip(&old.rows) {
        assert!(
            Arc::ptr_eq(&new.cells, &old.cells),
            "unchanged append must not allocate a new numeric row"
        );
    }
    assert_eq!(next.rows[4].cells[7].dimensions(), (2, 3));
    assert_eq!(next.rows[4].used_text, 12);
    assert_eq!(
        next.at_partition(2, 3, [0; 3], [0, 12, 0], 2, 7, GlyphArea::Text)
            .dimensions(),
        (2, 3)
    );
    assert_eq!(
        next.at_partition(2, 3, [0; 3], [0, 12, 0], 2, 12, GlyphArea::Text)
            .dimensions(),
        (0, 1)
    );
    assert_eq!(first.rows[4].cells[7].dimensions(), (0, 0));
    state.window_matrices.clear();
    let disabled = PosnFramePool::from_terminal_frame(&state, Some(&next), |_| 1);
    assert!(!disabled.rows[4].enabled);
    assert!(
        !disabled.retains_same_observations(&next),
        "changed enabled metadata cannot reuse the old accepted owner"
    );
    assert_eq!(disabled.rows[4].cells[7].dimensions(), (2, 3));
    assert_eq!(
        disabled.at_partition(2, 3, [0; 3], [0, 12, 0], 2, 7, GlyphArea::Text),
        PosnObjectExtent::Undrawn
    );
}

#[test]
fn new_terminal_partition_replaces_long_row_tail_with_explicit_zero_spaces() {
    use crate::glyph_matrix::{
        FrameDisplayState, GlyphMatrix, GlyphRow, MatrixRow, WindowMatrixEntry,
    };
    use crate::{DisplayWindowId, GlyphRowRole, Rect};
    let mut state = FrameDisplayState::new(12, 5, 1.0, 1.0);
    let mut matrix = GlyphMatrix::new(5, 12);
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    row.enabled = true;
    row.height_px = 1.0;
    matrix.rows[0] = MatrixRow::new(row);
    state.window_matrices.push(WindowMatrixEntry {
        window_id: DisplayWindowId::new(1),
        matrix,
        pixel_bounds: Rect::new(0.0, 0.0, 12.0, 5.0),
        text_pixel_bounds: Rect::new(0.0, 0.0, 12.0, 5.0),
        text_clip_bounds: Some(Rect::new(0.0, 0.0, 12.0, 5.0)),
        selected: true,
    });
    let long = PosnFramePool::from_terminal_frame(&state, None, |_| 1);
    assert!(
        long.rows[0]
            .cells
            .iter()
            .all(|cell| cell.dimensions() == (1, 0))
    );
    MatrixRow::make_mut(&mut state.window_matrices[0].matrix.rows[0]).ends_at_zv = true;
    let empty = PosnFramePool::from_terminal_frame(&state, Some(&long), |_| 1);
    assert_eq!(empty.rows[0].used_text, 12);
    assert_eq!(empty.rows[0].cells[0].dimensions(), (1, 0));
    assert_eq!(empty.rows[0].cells[1].dimensions(), (0, 0));
    assert_eq!(empty.rows[0].cells[11].dimensions(), (0, 0));
    assert_eq!(
        long.rows[0].cells[11].dimensions(),
        (1, 0),
        "old accepted capture is immutable"
    );
    let again = PosnFramePool::from_terminal_frame(&state, Some(&empty), |_| 1);
    assert!(again.retains_same_observations(&empty));
}

#[test]
fn terminal_reuse_and_shift_require_the_same_window_allocation_and_integral_source() {
    use crate::glyph_matrix::{
        FrameDisplayState, GlyphMatrix, GlyphRow, MatrixRow, RowDamage, WindowMatrixEntry,
    };
    use crate::types::Px;
    use crate::{DisplayWindowId, GlyphRowRole, Rect};
    let mut state = FrameDisplayState::new(12, 5, 1.0, 1.0);
    let mut matrix = GlyphMatrix::new(5, 12);
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    row.enabled = true;
    row.ends_at_zv = true;
    row.height_px = 1.0;
    matrix.rows[0] = MatrixRow::new(row);
    state.window_matrices.push(WindowMatrixEntry {
        window_id: DisplayWindowId::new(1),
        matrix,
        pixel_bounds: Rect::new(0.0, 0.0, 12.0, 5.0),
        text_pixel_bounds: Rect::new(0.0, 0.0, 12.0, 5.0),
        text_clip_bounds: Some(Rect::new(0.0, 0.0, 12.0, 5.0)),
        selected: true,
    });
    let mut accepted = PosnFramePool::from_terminal_frame(&state, None, |_| 1);
    Arc::make_mut(&mut accepted.rows[1].cells)[3] = PosnObjectExtent::Specialized {
        width: 2,
        height: 3,
    };
    state.window_matrices[0]
        .matrix
        .set_row_damage(0, RowDamage::Reused);
    let reused = PosnFramePool::from_terminal_frame(&state, Some(&accepted), |_| 1);
    assert!(reused.retains_same_observations(&accepted));
    state.window_matrices[0]
        .matrix
        .set_row_damage(0, RowDamage::ReusedShifted { dvpos: Px(-1.0) });
    let shifted = PosnFramePool::from_terminal_frame(&state, Some(&accepted), |_| 1);
    assert_eq!(shifted.rows[0].cells[3].dimensions(), (2, 3));
    assert!(Arc::ptr_eq(&shifted.rows[0].cells, &accepted.rows[1].cells));
    assert!(
        !shifted.rows[4].enabled,
        "typed shift cannot borrow beyond the accepted window partition"
    );
    state.window_matrices[0]
        .matrix
        .set_row_damage(0, RowDamage::ReusedShifted { dvpos: Px(0.5) });
    let fractional = PosnFramePool::from_terminal_frame(&state, Some(&accepted), |_| 1);
    assert!(!fractional.rows[0].enabled);
    state.window_matrices[0]
        .matrix
        .set_row_damage(0, RowDamage::Reused);
    state.window_matrices[0].window_id = DisplayWindowId::new(2);
    let sibling = PosnFramePool::from_terminal_frame(&state, Some(&accepted), |_| 1);
    assert!(
        !sibling.rows[0].enabled,
        "numeric coordinate coincidence is not same-window provenance"
    );
    state.window_matrices[0].window_id = DisplayWindowId::new(1);
    state.window_matrices[0].pixel_bounds.y = 1.0;
    state.window_matrices[0].text_pixel_bounds.y = 1.0;
    state.window_matrices[0]
        .text_clip_bounds
        .as_mut()
        .unwrap()
        .y = 1.0;
    let relocated = PosnFramePool::from_terminal_frame(&state, Some(&accepted), |_| 1);
    assert!(
        !relocated.rows[1].enabled,
        "changed vertical origin cannot authorize old row copy"
    );
    state.window_matrices[0]
        .matrix
        .set_row_damage(0, RowDamage::ReusedShifted { dvpos: Px(-1.0) });
    let relocated_shift = PosnFramePool::from_terminal_frame(&state, Some(&accepted), |_| 1);
    assert!(
        !relocated_shift.rows[1].enabled,
        "typed row shift does not prove partition relocation"
    );
    state.window_matrices[0]
        .matrix
        .set_row_damage(0, RowDamage::Reused);
    state.window_matrices[0].pixel_bounds.y = 0.0;
    state.window_matrices[0].text_pixel_bounds.y = 0.0;
    state.window_matrices[0]
        .text_clip_bounds
        .as_mut()
        .unwrap()
        .y = 0.0;
    state.window_matrices[0].pixel_bounds.height = 4.0;
    let resized = PosnFramePool::from_terminal_frame(&state, Some(&accepted), |_| 1);
    assert!(
        !resized.rows[0].enabled,
        "changed allocation height cannot authorize old row copy"
    );
    state.window_matrices[0].pixel_bounds.height = 5.0;
    state.window_matrices[0].pixel_bounds.width = 6.0;
    state.window_matrices[0].text_pixel_bounds.width = 6.0;
    let horizontal = PosnFramePool::from_terminal_frame(&state, Some(&accepted), |_| 1);
    assert!(
        !horizontal.rows[0].enabled,
        "accepted horizontal border metrics have not been captured"
    );
}
