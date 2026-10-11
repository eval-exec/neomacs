use super::*;
use crate::window_output::DisplayRowTerminatorCell;

#[test]
fn unfinished_row_moves_to_worker_with_rewind_and_terminator_state() {
    fn require_send_sync<T: Send + Sync + 'static>() {}
    require_send_sync::<WindowRowGeometry>();
    let mut geometry = WindowRowGeometry::new(2, 8.0, 12.0);
    geometry.begin_current_row_progress(Some(0), 2, 0, 8, 0);
    geometry.note_row_walk_start(LispCharPos1::new(1));
    geometry.push_text_display_point(LispCharPos1::new(2), 16.0, 20.0, 8.0, 16.0, 0, 1);
    geometry.update_current_row_progress(2, 2, 8, 16);
    let checkpoint = geometry.display_point_len();
    let (first, last) = geometry.current_row_display_positions();
    // A rejected wrap candidate must not survive the handoff.
    geometry.push_text_display_point(LispCharPos1::new(99), 24.0, 20.0, 8.0, 16.0, 0, 2);
    geometry.truncate_display_points(checkpoint);
    geometry.restore_current_row_display_positions(first, last);
    geometry.note_row_terminator(DisplayRowTerminator::new(
        LispCharPos1::new(3),
        DisplayRowTerminatorCell::new(8.0, 16.0),
    ));
    geometry.note_display_string_wrap(LispCharPos1::new(3));
    let prepared = std::thread::spawn(move || {
        geometry.push_text_row(20.0, 16.0, 12.0);
        assert_eq!(geometry.row_metrics()[0].height(), 16.0);
        assert_eq!(geometry.row_metrics()[0].ascent(), 12.0);
        geometry.finish(8)
    })
    .join()
    .unwrap();
    assert_eq!(prepared.rows.len(), 1);
    let row = &prepared.rows[0];
    assert_eq!(row.start_buffer_pos, Some(LispCharPos1::new(1)));
    assert_eq!(row.end_buffer_pos, Some(LispCharPos1::new(3)));
    assert_eq!(row.end_source, DisplayRowEndSource::DisplayStringWrap);
    assert_eq!((row.row, row.y, row.end_x, row.end_col), (2, 8, 16, 2));
    let points = prepared
        .point_rows
        .as_ref()
        .map(|rows| rows.iter_points().collect::<Vec<_>>())
        .unwrap_or_else(|| prepared.points.clone());
    assert_eq!(points.len(), 2);
    assert_eq!(points[0].buffer_pos, LispCharPos1::new(2));
    assert_eq!(points[1].buffer_pos, LispCharPos1::new(3));
    assert_eq!((points[1].x, points[1].width), (16, 8));
    assert_eq!(prepared.body_rows.len(), 1);
    assert_eq!(prepared.body_rows[0].body_row, 0);
    assert_eq!(prepared.body_rows[0].body_y, 0);
}
