//! Line spacing is row geometry, independent of point and region decoration.
use super::*;

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
