use super::*;

#[test]
fn empty_narrowing_hides_text_properties_and_keeps_overlay_carriers() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::runtime_startup_context();
    let result = eval
        .eval_str(
            r#"(with-temp-buffer
  (insert "abcdef")
  (put-text-property 1 7 'face 'bold)
  (let ((ov (make-overlay 3 4))
        (default-text-properties '(face italic q fallback)))
    (overlay-put ov 'q 5)
    (narrow-to-region 3 3)
    (and (null (text-properties-at 3))
         (eq (get-text-property 3 'face) 'italic)
         (eq (get-char-property 3 'face) 'italic)
         (equal (get-char-property-and-overlay 3 'face) '(italic))
         (eq (get-text-property 3 'q) 'fallback)
         (= (get-char-property 3 'q) 5)
         (eq (cdr (get-char-property-and-overlay 3 'q)) ov)
         (progn (overlay-put ov 'face 'overlay)
                (eq (get-char-property 3 'face) 'overlay)))))"#,
        )
        .unwrap();
    assert_eq!(result, Value::T);
}

#[test]
fn empty_narrowing_preserves_widened_internal_keymap_lookup() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::runtime_startup_context();
    let result = eval
        .eval_str(
            r#"(with-temp-buffer
  (insert "abc")
  (let ((property-map (make-sparse-keymap)) (buffer-map (make-sparse-keymap)))
    (define-key property-map "x" #'ignore)
    (define-key buffer-map "x" #'forward-char)
    (put-text-property 1 4 'local-map property-map)
    (use-local-map buffer-map)
    (narrow-to-region 2 2)
    (goto-char 2)
    (and (null (get-text-property 2 'local-map))
         (eq (key-binding "x") #'ignore))))"#,
        )
        .unwrap();
    assert_eq!(result, Value::T);
}

#[test]
fn nonempty_narrowing_endpoint_keeps_next_character_properties() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::runtime_startup_context();
    let result = eval
        .eval_str(
            r#"(with-temp-buffer
  (insert "aж😀z")
  (put-text-property 1 5 'face 'bold)
  (let ((default-text-properties '(face italic)))
    (narrow-to-region 2 3)
    (and (equal (text-properties-at 3) '(face bold))
         (eq (get-text-property 3 'face) 'bold)
         (eq (get-char-property 3 'face) 'bold)
         (equal (get-char-property-and-overlay 3 'face) '(bold)))))"#,
        )
        .unwrap();
    assert_eq!(result, Value::T);
}
