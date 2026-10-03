use super::*;

#[test]
fn full_mini_measurement_grows_emitted_rows_and_preserves_existing_rows() {
    let mut grid =
        OutputWindowRowGrid::new(1, 12).with_row_capacity(OutputWindowRowCapacity::Growing);
    grid.begin_row(OutputRowBeginRequest::text_at(
        0,
        crate::types::LayoutCharPos0::new(3),
    ));
    let first = grid.row(0).expect("first row").clone();
    grid.begin_row(OutputRowBeginRequest::text_at(
        7,
        crate::types::LayoutCharPos0::new(21),
    ));
    assert_eq!(grid.matrix.nrows, 8);
    assert_eq!(grid.matrix.ncols, 12);
    assert_eq!(grid.row(0).expect("preserved first row"), &first);
    assert_eq!(grid.row(7).expect("new measured row").start_charpos, 21);
    assert_eq!(grid.finalized_rows.len(), grid.matrix.nrows);
}

#[test]
fn ordinary_fixed_grid_never_grows_past_the_requested_viewport() {
    let mut grid = OutputWindowRowGrid::new(1, 12);
    grid.begin_row(OutputRowBeginRequest::text_at(
        7,
        crate::types::LayoutCharPos0::new(21),
    ));
    assert_eq!(grid.matrix.nrows, 1);
    assert!(grid.row(7).is_none());
}

#[test]
fn measurement_removes_only_final_inter_line_spacing() {
    let mut grid = OutputWindowRowGrid::new(2, 12);
    for index in 0..2 {
        let mut row = GlyphRow::new(GlyphRowRole::Text);
        row.enabled = true;
        row.pixel_y = index as f32 * 11.0;
        row.height_px = 11.0;
        row.line_spacing_px = 2.0;
        grid.replace_row(index, row);
    }
    assert_eq!(
        grid.mini_measurement_height_px(9.0),
        grid.content_height_px(9.0) - 2.0
    );
    assert_eq!(grid.row(0).expect("first line").height_px, 11.0);
}
