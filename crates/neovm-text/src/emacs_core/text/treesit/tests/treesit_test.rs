use super::*;

#[test]
fn buffer_edit_without_tree_does_not_track_pending_edit() {
    crate::test_utils::init_test_tracing();
    let mut manager = TreeSitterManager::new();
    let buffer_id = BufferId(7);
    let mut buffer = Buffer::new(
        buffer_id,
        Value::string("test"),
        neovm_edit::buffer::shared::SavedPointBeforeCommand::new_editor_global(),
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
        neovm_edit::buffer::shared::SavedPointBeforeCommand::new_editor_global(),
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
        neovm_edit::buffer::shared::SavedPointBeforeCommand::new_editor_global(),
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

#[test]
fn walk_edit_node_collection_filters_stale_reparse_epoch_and_tree_ptr() {
    crate::test_utils::init_test_tracing();
    let mut manager = TreeSitterManager::new();
    let live_tree_ptr = std::ptr::dangling::<ffi::TSTree>();
    let stale_tree_ptr = (live_tree_ptr as usize + 64) as *const ffi::TSTree;
    let raw_for = |tree: *const ffi::TSTree, id_addr: usize| ffi::TSNode {
        context: [0; 4],
        id: id_addr as *const std::ffi::c_void,
        tree,
    };

    manager.nodes.insert(
        10,
        NodeEntry {
            parser_id: 1,
            raw: raw_for(live_tree_ptr, 0x100),
            reparse_epoch: 2,
        },
    );
    manager.nodes.insert(
        11,
        NodeEntry {
            parser_id: 1,
            raw: raw_for(live_tree_ptr, 0x200),
            reparse_epoch: 1, // stale reparse_epoch
        },
    );
    manager.nodes.insert(
        12,
        NodeEntry {
            parser_id: 1,
            raw: raw_for(stale_tree_ptr, 0x300),
            reparse_epoch: 2, // stale tree pointer
        },
    );
    manager.nodes.insert(
        13,
        NodeEntry {
            parser_id: 2, // unrelated parser
            raw: raw_for(stale_tree_ptr, 0x400),
            reparse_epoch: 1,
        },
    );

    let (live_nodes, mut stale_removals) =
        manager.collect_nodes_for_walk_edit(1, 2, live_tree_ptr as usize);
    stale_removals.sort_by_key(|(id, _)| *id);

    assert_eq!(live_nodes.len(), 1);
    assert_eq!(live_nodes[0].0, 10);
    assert_eq!(live_nodes[0].1.id, 0x100 as *const std::ffi::c_void);
    assert_eq!(stale_removals.len(), 2);
    assert_eq!(stale_removals[0].0, 11);
    assert!(stale_removals[0].1.is_none());
    assert_eq!(stale_removals[1].0, 12);
    assert!(stale_removals[1].1.is_none());
}
