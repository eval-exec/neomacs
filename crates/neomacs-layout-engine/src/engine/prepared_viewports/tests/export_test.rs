use super::*;
use neovm_core::emacs_core::{Context, Value};

#[test]
fn conflicting_worker_paragraph_does_not_revoke_other_prepared_rows() {
    let mut eval = Context::new();
    let buffer = eval.buffer_manager().current_buffer().unwrap().id();
    let line = "ordinary offscreen text\n";
    eval.buffer_manager_mut()
        .get_mut(buffer)
        .unwrap()
        .insert(&line.repeat(300));
    let frame = eval
        .frame_manager_mut()
        .create_frame("coverage-conflict", 800, 600, buffer);
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .window_system = Some(Value::symbol("neomacs"));
    let window = eval.frame_manager().get(frame).unwrap().selected_window;
    eval.eval_str("(goto-char 1) (set-window-start nil 1 t)")
        .unwrap();
    let owner = DisplayWindowId::new(window.0 as i64);
    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame);
    let current = engine.retained_window_matrices[&owner].clone();
    let body: Vec<_> = current
        .matrix
        .rows
        .iter()
        .filter(|row| row.enabled && row.role == GlyphRowRole::Text)
        .collect();
    let target = body[body.len() - 2].start_charpos;
    engine
        .request_scroll_coverage(&eval, frame, window, CharPos0::new(target))
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !engine
        .scroll_coverage
        .drain(&mut engine.prepared_viewports)
        .unwrap()
    {
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    engine.layout_frame_rust(&mut eval, frame);
    assert_eq!(
        engine
            .last_frame_display_state
            .as_ref()
            .unwrap()
            .scroll_coverage
            .len(),
        1
    );
    let worker = engine
        .prepared_viewports
        .entries
        .iter_mut()
        .find(|entry| entry.computed)
        .unwrap();
    let index = worker
        .retained
        .matrix
        .rows
        .iter()
        .position(|row| row.start_charpos == target)
        .unwrap();
    // Model the real conflict: the same source anchor has a different wrap
    // end in a worker captured from a different start within its paragraph.
    let mut conflicting = worker.retained.matrix.rows[index].as_ref().clone();
    conflicting.end_charpos -= 1;
    conflicting.continued = true;
    worker.retained.matrix.rows[index] = MatrixRow::new(conflicting);
    engine.layout_frame_rust(&mut eval, frame);
    let state = engine.last_frame_display_state.as_ref().unwrap();
    let coverage = state
        .scroll_coverage
        .first()
        .expect("compatible paragraphs must remain available");
    let chosen = coverage
        .content
        .matrix
        .rows
        .iter()
        .find(|row| row.start_charpos == target)
        .unwrap();
    let expected = body.iter().find(|row| row.start_charpos == target).unwrap();
    assert_eq!(chosen.end_charpos, expected.end_charpos);
    assert_eq!(chosen.continued, expected.continued);
    assert!(
        coverage.content.matrix.rows.last().unwrap().start_charpos
            > body.last().unwrap().start_charpos
    );
}
