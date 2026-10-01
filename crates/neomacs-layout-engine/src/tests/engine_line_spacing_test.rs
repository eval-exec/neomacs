//! Line spacing is row geometry, independent of point and region decoration.
use super::*;

#[test]
fn text_cursor_keeps_its_metrics_when_line_spacing_enlarges_the_row() {
    let (mut eval, frame, buffer, _window) = incr_editing_frame("alpha\nbeta\n", 800, 240);
    realize_test_gui_frame(&mut eval, frame);
    let mut engine = LayoutEngine::new();
    engine.enable_cosmic_metrics();
    let FrameLayoutAttempt::Prepared(unspaced) = engine.redisplay_frame_attempt(&mut eval, frame)
    else {
        panic!("unspaced frame should prepare");
    };
    eval.buffer_manager_mut()
        .get_mut(buffer)
        .unwrap()
        .set_buffer_local("line-spacing", Value::fixnum(5));
    let FrameLayoutAttempt::Prepared(spaced) = engine.redisplay_frame_attempt(&mut eval, frame)
    else {
        panic!("spaced frame should prepare");
    };
    let unspaced = unspaced.materialize();
    let spaced = spaced.materialize();
    let before = unspaced.active_cursor().expect("unspaced text cursor");
    let after = spaced.active_cursor().expect("spaced text cursor");
    let (_, row_y, _, row_height) = spaced
        .slot_glyph(after.slot_id)
        .unwrap()
        .cell_rect()
        .unwrap();
    assert_eq!(after.height, before.height, "spacing is not cursor height");
    assert_eq!(
        after.ascent, before.ascent,
        "spacing must not move the baseline"
    );
    assert_eq!(row_height, after.height + 5.0);
    assert_eq!(after.y, row_y);
}

#[test]
fn newline_spacing_is_preserved_when_point_enters_an_empty_line() {
    let (mut eval, frame, _buffer, window) = incr_editing_frame("alpha\n\nbeta\n", 800, 240);
    realize_test_gui_frame(&mut eval, frame);
    let mut engine = LayoutEngine::new();
    engine.enable_cosmic_metrics();
    let rows = |engine: &mut LayoutEngine, eval: &mut Context| {
        let FrameLayoutAttempt::Prepared(output) = engine.redisplay_frame_attempt(eval, frame)
        else {
            panic!("frame should prepare");
        };
        output
            .window_matrices
            .iter()
            .find(|entry| entry.window_id.get() == window.0 as i64)
            .unwrap()
            .matrix
            .rows
            .iter()
            .filter(|row| row.enabled && !row.mode_line)
            .take(3)
            .map(|row| (row.pixel_y, row.height_px))
            .collect::<Vec<_>>()
    };
    let unspaced = rows(&mut engine, &mut eval);
    eval.eval_str("(put-text-property 1 13 'line-spacing 5)")
        .unwrap();
    let before = rows(&mut engine, &mut eval);
    assert_eq!(before.len(), 3, "three text rows: {before:?}");
    eval.eval_str("(set-window-point (selected-window) 7)")
        .unwrap();
    let after = rows(&mut engine, &mut eval);
    let mut fresh = LayoutEngine::new();
    fresh.enable_cosmic_metrics();
    assert_eq!(
        after,
        rows(&mut fresh, &mut eval),
        "retained and fresh row geometry agree"
    );
    assert_eq!(
        before[0].1,
        unspaced[0].1 + 5.0,
        "line-spacing belongs to the logical row height: {before:?}"
    );
    assert_eq!(before, after, "moving point must not change line spacing");
    for pair in before.windows(2) {
        assert_eq!(
            pair[1].0 - pair[0].0,
            pair[0].1,
            "GNU row height includes spacing: adjacent backgrounds must meet"
        );
    }
}

#[test]
fn relative_default_line_spacing_uses_whole_pixels_without_moving_the_baseline() {
    let (mut eval, frame, _, window) = incr_editing_frame("alpha\n\nbeta\n", 800, 240);
    realize_test_gui_frame(&mut eval, frame);
    let mut engine = LayoutEngine::new();
    engine.enable_cosmic_metrics();
    let FrameLayoutAttempt::Prepared(unspaced) = engine.redisplay_frame_attempt(&mut eval, frame)
    else {
        panic!("unspaced frame should prepare");
    };
    let baseline = &unspaced
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == window.0 as i64)
        .unwrap()
        .matrix
        .rows[0];
    // Arrange 2.75px of requested spacing with whatever native font the
    // platform realizes. GNU contributes two pixels, without raising ascent.
    let factor = 2.75 / f64::from(baseline.height_px);
    eval.eval_str(&format!(
        "(setq default-text-properties '(line-spacing {factor}))"
    ))
    .unwrap();
    let mut engine = LayoutEngine::new();
    engine.enable_cosmic_metrics();
    let FrameLayoutAttempt::Prepared(output) = engine.redisplay_frame_attempt(&mut eval, frame)
    else {
        panic!("frame should prepare");
    };
    let rows = &output
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == window.0 as i64)
        .unwrap()
        .matrix
        .rows;
    assert_eq!(rows[0].height_px, baseline.height_px + 2.0);
    assert_eq!(rows[1].pixel_y - rows[0].pixel_y, baseline.height_px + 2.0);
    assert_eq!(rows[2].pixel_y - rows[1].pixel_y, baseline.height_px + 2.0);
    assert_eq!(rows[0].ascent_px, baseline.ascent_px);
}

#[test]
fn routed_row_keeps_tall_glyph_metrics_when_newline_returns_to_default_face() {
    let line = "tall text\n";
    let (mut eval, frame, buffer, _) = incr_editing_frame(&line.repeat(50), 800, 600);
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .window_system = Some(Value::symbol("neomacs"));
    eval.eval_str(&format!(
        "(put-text-property 1 {} 'face '(:height 150))",
        3 * line.len()
    ))
    .unwrap();
    eval.buffer_manager_mut()
        .get_mut(buffer)
        .unwrap()
        .goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(5 * line.len()));
    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame);
    let window = eval.frame_manager().get(frame).unwrap().selected_window;
    let retained = &engine.retained_window_matrices
        [&neomacs_display_protocol::types::DisplayWindowId::new(window.0 as i64)];
    let row = retained
        .matrix
        .rows
        .iter()
        .find(|row| row.enabled && row.start_charpos == 2 * line.len())
        .unwrap();
    let glyph_height = row
        .glyphs
        .iter()
        .flatten()
        .map(|glyph| glyph.pixel_height)
        .fold(0.0f32, f32::max);
    assert!(
        glyph_height > retained.key.char_height,
        "fixture must use a taller face"
    );
    assert!(
        row.height_px >= glyph_height,
        "row height {} discarded a {}px glyph",
        row.height_px,
        glyph_height
    );
}
