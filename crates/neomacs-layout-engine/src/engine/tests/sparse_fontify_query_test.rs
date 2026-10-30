//! A position query must fontify every visible span it found before retrying.
//!
//! GNU's display iterator fontifies lazily as it visits glyphs.  Neomacs walks
//! immutably, collects the visible positions that still need `fontified`, and
//! retries the frame so the walk can see the resulting faces.  The corpus here
//! has sparse visible runs separated by invisible text (the shape of a folded
//! Org buffer): the walk reaches only a dozen visible characters per run, so
//! the plan contains far more spans than the retry budget.  Fontifying one
//! span per attempt can therefore never converge.
use super::*;
use neovm_core::window::WindowLayoutQueryScope;

#[test]
fn sparse_visible_fontification_converges_with_more_spans_than_retries() {
    let blocks = 80usize;
    let visible = 12usize;
    let hidden = 1530usize;
    let step = visible + hidden;
    let mut text = String::new();
    for _ in 0..blocks {
        text.push_str(&"V".repeat(visible));
        text.push_str(&"h".repeat(hidden));
    }
    let (mut eval, frame, buffer_id, window) = incr_editing_frame(&text, 200, 240);
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .set_window_system(Some(Value::symbol("neomacs")));
    eval.eval_str(&format!(
        r#"(progn
             (setq buffer-invisibility-spec t)
             (let ((pos {}) (n 0))
               (while (< n {blocks})
                 (put-text-property pos (+ pos {hidden}) 'invisible t)
                 (setq pos (+ pos {step}) n (1+ n))))
             (setq fontification-functions
                   (list (lambda (pos)
                           (put-text-property pos (min (point-max) (+ pos 64))
                                              'fontified t)))))"#,
        visible + 1,
    ))
    .unwrap();
    let end = eval
        .buffer_manager()
        .get(buffer_id)
        .unwrap()
        .point_max_lisp_char_pos();
    let scope = WindowLayoutQueryScope::Position { target: end };
    let result = WindowLayoutQueryEngine::new_without_font_metrics()
        .query_window_layout(&mut eval, frame, window, scope);
    assert!(
        result
            .as_ref()
            .is_ok_and(|query| query.geometry().is_some()),
        "sparse visible fontification did not converge: {result:?}"
    );
}
