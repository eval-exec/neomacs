use super::*;

#[test]
fn gde_overlay_cycle_and_property_filtering() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::runtime_startup_context();
    eval.set_lexical_binding(true);
    let result = eval.eval_str(
        r#"(with-temp-buffer
  (insert "abcdefghijkl")
  (let ((a (make-overlay 1 10)) (b (make-overlay 2 8)) (c (make-overlay 3 11)) out)
    (dolist (item (list (list a 'a '(0 . 2)) (list b 'b '(0 . 0)) (list c 'c '(0 . 1))))
      (overlay-put (nth 0 item) 'tag (nth 1 item))
      (overlay-put (nth 0 item) 'probe (nth 1 item))
      (overlay-put (nth 0 item) 'priority (nth 2 item)))
    (dolist (disabled '(nil a b c))
      (overlay-put a 'probe (unless (eq disabled 'a) 'a))
      (overlay-put b 'probe (unless (eq disabled 'b) 'b))
      (overlay-put c 'probe (unless (eq disabled 'c) 'c))
      (let* ((pair (get-char-property-and-overlay 5 'probe)) (winner (cdr pair)))
        (push (list disabled (get-char-property 5 'probe) (car pair)
                    (and winner (overlay-get winner 'tag))) out)))
    (list (mapcar (lambda (ov) (overlay-get ov 'tag)) (overlays-at 5)) (nreverse out))))"#,
    );
    let actual = crate::emacs_core::format_eval_result_with_eval(&eval, &result);
    let expected = r#"OK ((a b c) ((nil c c c) (a c c c) (b a a a) (c b b b)))"#;
    assert_eq!(actual, expected);
}
