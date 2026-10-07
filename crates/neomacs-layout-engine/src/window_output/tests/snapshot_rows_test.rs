use super::*;
use neovm_core::buffer::LispCharPos1;
use neovm_core::window::DisplayPointRole;

#[test]
fn worker_finalization_preserves_source_order_and_revisited_row_geometry() {
    fn require_send_sync<T: Send + Sync + 'static>() {}
    require_send_sync::<PreparedWindowRows>();
    let points: Vec<_> = [(4, 3, 64), (2, 2, 48), (1, 3, 60), (3, 2, 48)]
        .into_iter()
        .map(|(pos, row, y)| DisplayPointSnapshot {
            role: DisplayPointRole::Glyph,
            buffer_pos: LispCharPos1::new(pos),
            x: 0,
            y,
            width: 8,
            height: 16,
            row,
            col: pos,
        })
        .collect();
    // This is the previous canonical algorithm: first source-ordered
    // point wins when several positions describe the same output row.
    let mut ordered = points.clone();
    ordered.sort_by_key(|point| (point.buffer_pos, point.row, point.col, point.x));
    let mut expected: Vec<_> = ordered
        .iter()
        .map(|point| PresentedBodyRowSnapshot {
            output_row: point.row,
            body_row: point.row.saturating_sub(2),
            body_y: point.y.saturating_sub(16),
        })
        .collect();
    expected.sort_by_key(|row| row.output_row);
    expected.dedup_by_key(|row| row.output_row);
    let result = std::thread::spawn(move || PreparedWindowRows::new(points, Vec::new(), 2, 16))
        .join()
        .unwrap();
    assert_eq!(result.points, ordered);
    assert_eq!(result.body_rows, expected);
    assert_eq!(result.body_rows.len(), 2);
    assert_eq!(result.body_rows[1].body_y, 44);
}
