use super::*;

#[test]
fn category_word_wrap_fills_a_cjk_run_in_the_real_row_producer() {
    for enabled in [false, true] {
        let text = format!("supertag v0.4 {}", "提".repeat(60));
        let (mut eval, frame, _buffer, window) = incr_editing_frame(&text, 200, 240);
        eval.eval_str("(progn (setq mode-line-format nil header-line-format nil tab-line-format nil word-wrap t truncate-lines nil) (define-category ?| \"line breakable\") (modify-category-entry ?提 ?|))").unwrap();
        eval.set_variable("word-wrap-by-category", Value::bool_val(enabled));
        let mut engine = LayoutEngine::new_without_font_metrics();
        engine.layout_frame_rust(&mut eval, frame);
        let rows = &eval
            .frame_manager()
            .get(frame)
            .unwrap()
            .redisplay_snapshot(window)
            .unwrap()
            .rows;
        let next = rows
            .iter()
            .filter_map(|row| row.start_buffer_pos)
            .nth(1)
            .expect("continuation row")
            .as_i64();
        if enabled {
            assert!(
                next > 15,
                "category wrapping must fill the first row with CJK characters: next={next}"
            );
        } else {
            assert_eq!(
                next, 15,
                "the nil setting preserves whitespace-only wrapping"
            );
        }
    }
}

#[test]
fn category_word_wrap_tracks_live_options_and_buffer_category_tables() {
    let text = format!("supertag v0.4 {}", "提".repeat(60));
    let (mut eval, frame, _buffer, window) = incr_editing_frame(&text, 200, 240);
    eval.eval_str("(progn (setq mode-line-format nil header-line-format nil tab-line-format nil word-wrap t truncate-lines nil word-wrap-by-category nil) (define-category ?| \"line breakable\") (modify-category-entry ?提 ?|))").unwrap();
    let mut engine = LayoutEngine::new_without_font_metrics();
    let mut next = |eval: &mut Context| {
        engine.layout_frame_rust(eval, frame);
        eval.frame_manager()
            .get(frame)
            .unwrap()
            .redisplay_snapshot(window)
            .unwrap()
            .rows
            .iter()
            .filter_map(|row| row.start_buffer_pos)
            .nth(1)
            .unwrap()
            .as_i64()
    };
    assert_eq!(next(&mut eval), 15);
    eval.eval_str("(setq word-wrap-by-category t)").unwrap();
    assert!(next(&mut eval) > 15);
    eval.eval_str("(set-category-table (make-category-table))")
        .unwrap();
    assert_eq!(
        next(&mut eval),
        15,
        "a buffer-local table without | must replace the old rules"
    );
    eval.eval_str("(progn (define-category ?| \"line breakable\") (modify-category-entry ?提 ?|))")
        .unwrap();
    assert!(next(&mut eval) > 15);
    eval.eval_str("(aset (char-category-set ?提) ?| nil)")
        .unwrap();
    assert_eq!(
        next(&mut eval),
        15,
        "mutating the category set itself must invalidate retained rows"
    );
}
