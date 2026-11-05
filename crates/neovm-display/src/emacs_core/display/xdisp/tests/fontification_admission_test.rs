use super::*;

#[test]
fn fontification_without_callback_reports_native_inspection() {
    let mut eval = Context::new();
    eval.eval_str("(insert \"x\") (setq fontification-functions nil)")
        .unwrap();
    let buffer = eval.buffers.current_buffer_id().unwrap();
    let outcome = ensure_fontified_for_redisplay(&mut eval, buffer, 0, 1).unwrap();
    assert_eq!(outcome, RedisplayFontificationOutcome::Unchanged);
    assert!(!outcome.entered_callbacks());
}

#[test]
fn nonmarking_and_signaling_fontifiers_remain_lisp_boundaries() {
    for signal in [false, true] {
        let mut eval = Context::new();
        eval.eval_str(&format!("(insert \"x\") (setq fontification-test-calls nil fontification-functions (list (lambda (p) (setq fontification-test-calls (cons p fontification-test-calls) truncate-lines t) {})))", if signal {"(error \"fontify failure\")"} else {"nil"})).unwrap();
        let buffer = eval.buffers.current_buffer_id().unwrap();
        let outcome = ensure_fontified_for_redisplay(&mut eval, buffer, 0, 1).unwrap();
        assert_eq!(outcome, RedisplayFontificationOutcome::CallbacksUnchanged);
        assert!(outcome.entered_callbacks());
        assert!(!outcome.requires_layout_retry());
        assert_eq!(
            super::super::super::print::print_value(
                &eval.eval_str("fontification-test-calls").unwrap()
            ),
            "(1)"
        );
    }
}

#[test]
fn fontification_merge_preserves_callback_admission_after_a_marked_position() {
    use RedisplayFontificationOutcome::{CallbacksUnchanged, Fontified, Unchanged};
    assert_eq!(Unchanged.merge(CallbacksUnchanged), CallbacksUnchanged);
    assert_eq!(CallbacksUnchanged.merge(Unchanged), CallbacksUnchanged);
    assert_eq!(CallbacksUnchanged.merge(Fontified), Fontified);
    assert_eq!(Fontified.merge(CallbacksUnchanged), Fontified);
}
