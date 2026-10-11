use super::*;

#[test]
fn buffer_edit_without_tree_does_not_track_pending_edit() {
    crate::test_utils::init_test_tracing();
    let mut manager = TreeSitterManager::new();
    let buffer_id = BufferId(7);
    let mut buffer = Buffer::new(
        buffer_id,
        Value::string("test"),
        crate::buffer::shared::SavedPointBeforeCommand::new_editor_global(),
    );
    buffer.insert("alpha\nbeta\ngamma\n");
    let end = buffer.accessible_emacs_byte_range().end();

    manager.begin_buffer_edit(buffer_id, &buffer, EmacsByteRange::new(end, end));

    assert!(manager.pending_edits.is_empty());
}

/// GNU clips a recorded change into the **parser's** `visible_beg`/`visible_end`
/// (`src/treesit.c:1420-1435`), not into whatever restriction the buffer
/// happens to carry while the edit runs.
#[test]
fn parser_edit_positions_are_relative_to_the_parser_window() {
    crate::test_utils::init_test_tracing();
    let buffer_id = BufferId(7);
    let mut buffer = Buffer::new(
        buffer_id,
        Value::string("test"),
        crate::buffer::shared::SavedPointBeforeCommand::new_editor_global(),
    );
    buffer.insert("hidden\nalpha\nbeta\nhidden");
    // The buffer is wide open; only this parser's tree is restricted.
    let window = EmacsByteRange::from_usize(7, 17);

    let edit = PendingBufferEdit::for_buffer(
        &buffer,
        EmacsByteRange::from_usize(13, 17),
        ParserPointTracking::LineAndColumn,
        [(1u64, window)].into_iter(),
    );
    let relative = edit.for_window(&buffer, 1, window, EmacsBytePos::new(17));

    assert_eq!(relative.edit.start_byte, 6);
    assert_eq!(relative.edit.old_end_byte, 10);
    assert_eq!(relative.edit.start_position, Point::new(1, 0));
    assert_eq!(relative.edit.old_end_position, Point::new(1, 4));
    assert_eq!(relative.new_visible, window);
}

#[test]
fn byte_only_parser_edit_preparation_does_not_scan_for_line_columns() {
    crate::test_utils::init_test_tracing();
    let buffer_id = BufferId(7);
    let mut buffer = Buffer::new(
        buffer_id,
        Value::string("test"),
        crate::buffer::shared::SavedPointBeforeCommand::new_editor_global(),
    );
    buffer.insert("alpha\nbeta\ngamma\n");
    let window = EmacsByteRange::from_usize(0, 17);

    let edit = PendingBufferEdit::for_buffer(
        &buffer,
        EmacsByteRange::from_usize(11, 11),
        ParserPointTracking::BytesOnly,
        [(1u64, window)].into_iter(),
    );

    assert!(
        edit.points.is_empty(),
        "a byte-only edit must not scan the buffer for line and column"
    );
    let relative = edit.for_window(&buffer, 1, window, EmacsBytePos::new(11));
    assert_eq!(relative.edit.start_byte, 11);
    assert_eq!(relative.edit.start_position, Point::new(1, 0));
    assert_eq!(relative.edit.old_end_position, Point::new(1, 0));
}
