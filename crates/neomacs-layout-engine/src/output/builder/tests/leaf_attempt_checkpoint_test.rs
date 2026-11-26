use super::*;

fn begin_leaf(builder: &mut DisplayOutputBuilder, window_id: u64, ch: char) {
    builder.begin_window(window_id, 1, 8, Rect::new(0.0, 0.0, 64.0, 16.0), false);
    builder.begin_row(0, GlyphRowRole::Text);
    builder
        .edit_current_row_for_test(|row| {
            crate::glyph_row_writer::push_char_to_row(row, ch, FaceId::new(0), 0, 8.0);
        })
        .expect("open leaf row");
    builder.end_row();
}

#[test]
fn leaf_checkpoint_discards_closed_and_open_grids_preserving_completed_sibling() {
    let mut builder = DisplayOutputBuilder::new();
    begin_leaf(&mut builder, 1, 's');
    builder.end_window();
    builder.add_output_background(Rect::new(0.0, 0.0, 64.0, 16.0), Color::BLACK);

    // A rejected tail can already have closed its grid. A later rejection can
    // instead leave its grid open; neither may displace the accepted sibling.
    for close_grid in [true, false, true] {
        let checkpoint = builder.capture_leaf_attempt_checkpoint(DisplayWindowId::new(2));
        begin_leaf(&mut builder, 2, 'x');
        builder.add_output_background(Rect::new(0.0, 16.0, 64.0, 16.0), Color::WHITE);
        if close_grid {
            builder.end_window();
        }
        builder.restore_leaf_attempt_checkpoint(checkpoint);
        assert!(builder.unpublished_window_grid_owner().is_none());
    }

    begin_leaf(&mut builder, 2, 'a');
    builder.end_window();
    let state = builder.finish(8, 2, 8.0, 16.0);
    assert_eq!(state.backgrounds.len(), 1);
    assert_eq!(state.window_matrices.len(), 2);
    for (entry, (window_id, ch)) in state.window_matrices.iter().zip([(1, 's'), (2, 'a')]) {
        assert_eq!(entry.window_id, DisplayWindowId::new(window_id));
        assert_eq!(
            entry.matrix.rows[0].glyphs[GlyphArea::Text.index()][0].glyph_type,
            GlyphType::Char { ch },
        );
    }
}

#[test]
#[should_panic(expected = "source horizon retry must precede completed window publication")]
fn source_checkpoint_still_refuses_completed_grid_rollback() {
    let mut builder = DisplayOutputBuilder::new();
    let checkpoint = builder.capture_source_attempt_checkpoint(DisplayWindowId::new(1));
    begin_leaf(&mut builder, 1, 'x');
    builder.end_window();
    builder.restore_source_attempt_checkpoint(checkpoint);
}
