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

#[test]
fn gde_overlay_nontransitive_sorted_queries() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::runtime_startup_context();
    eval.set_lexical_binding(true);
    let result = eval.eval_str(
        r#"(with-temp-buffer (insert "abcdefghijkl")
(let ((a (make-overlay 1 10)) (b (make-overlay 2 8)) (c (make-overlay 3 11)))
(dolist (item (list (list a 'a '(0 . 2)) (list b 'b '(0 . 0)) (list c 'c '(0 . 1))))
(overlay-put (car item) 'tag (cadr item)) (overlay-put (car item) 'priority (nth 2 item)))
(list (mapcar (lambda (o) (overlay-get o 'tag)) (overlays-at 5 t))
(mapcar (lambda (o) (overlay-get o 'tag)) (overlays-at 5)))))"#,
    );
    let actual = crate::emacs_core::format_eval_result_with_eval(&eval, &result);
    let expected = r#"OK ((c b a) (a b c))"#;
    assert_eq!(actual, expected);
}

#[test]
fn gde_overlay_collapsed_start_query_order() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::runtime_startup_context();
    eval.set_lexical_binding(true);
    let result = eval.eval_str(
        r#"(let (out)
(dolist (text (list (make-string 60 ?a) (make-string 60 ?ж)
(apply #'unibyte-string (make-list 60 255))
(string-as-multibyte (apply #'unibyte-string (make-list 60 255)))))
(with-temp-buffer (set-buffer-multibyte (multibyte-string-p text)) (insert text)
(let ((a (make-overlay 10 30)) (b (make-overlay 20 30)) (c (make-overlay 5 30)))
(dolist (item (list (cons a 'a) (cons b 'b) (cons c 'c)))
(overlay-put (car item) 'tag (cdr item)))
(delete-region 5 25)
(push (list (mapcar (lambda (o) (overlay-get o 'tag)) (overlays-at 5))
(mapcar (lambda (o) (overlay-get o 'tag)) (overlays-in 5 6))
(mapcar (lambda (o) (overlay-get o 'tag)) (car (overlay-lists)))) out))))
(nreverse out))"#,
    );
    let actual = crate::emacs_core::format_eval_result_with_eval(&eval, &result);
    let expected = r#"OK (((c a b) (c a b) (c a b)) ((c a b) (c a b) (c a b)) ((c a b) (c a b) (c a b)) ((c a b) (c a b) (c a b)))"#;
    assert_eq!(actual, expected);
}

#[test]
fn gde_overlay_end_default_properties() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::runtime_startup_context();
    eval.set_lexical_binding(true);
    let result = eval.eval_str(
        r#"(let ((default-text-properties '(probe fallback front-sticky t)))
(list (let ((s "a")) (list (get-text-property 1 'probe s)
(get-char-property 1 'probe s) (get-char-property-and-overlay 1 'probe s)))
(with-temp-buffer (insert "ab")
(list (get-text-property (point-max) 'probe) (get-char-property (point-max) 'probe)
(get-char-property-and-overlay (point-max) 'probe)
(progn (overlay-put (make-overlay 1 3) 'face 'bold)
(list (get-char-property 3 'probe) (get-char-property-and-overlay 3 'probe)))
(save-restriction (narrow-to-region 1 2) (get-char-property (point-max) 'probe))))))"#,
    );
    let actual = crate::emacs_core::format_eval_result_with_eval(&eval, &result);
    let expected = r#"OK ((fallback fallback (fallback)) (fallback fallback (fallback) (fallback (fallback)) fallback))"#;
    assert_eq!(actual, expected);
}

#[test]
fn gde_overlay_sorted_category_and_window() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::runtime_startup_context();
    eval.set_lexical_binding(true);
    let result = eval.eval_str(
        r#"(save-window-excursion
(with-temp-buffer (insert "abcdef")
(let ((w1 (selected-window)) (w2 (split-window))
(a (make-overlay 1 6)) (b (make-overlay 1 6)) (c (make-overlay 1 6)))
(set-window-buffer w1 (current-buffer)) (set-window-buffer w2 (current-buffer))
(unwind-protect
(progn (put 'gde-sorted-category 'priority 5) (put 'gde-sorted-category 'window w1)
(overlay-put a 'category 'gde-sorted-category) (overlay-put a 'tag 'category)
(overlay-put b 'priority 9) (overlay-put b 'window w2) (overlay-put b 'tag 'window2)
(overlay-put c 'priority 1) (overlay-put c 'tag 'plain)
(mapcar (lambda (sorted) (mapcar (lambda (o) (overlay-get o 'tag)) (overlays-at 3 sorted)))
(list t w1 w2)))
(setplist 'gde-sorted-category nil)))))"#,
    );
    let actual = crate::emacs_core::format_eval_result_with_eval(&eval, &result);
    assert_eq!(
        actual,
        r#"OK ((window2 category plain) (category plain) (window2 plain))"#
    );
}

#[test]
fn gde_overlay_default_local_map_at_end() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::runtime_startup_context();
    eval.set_lexical_binding(true);
    let result = eval.eval_str(
        r#"(let ((property-map (make-sparse-keymap)) (buffer-map (make-sparse-keymap)))
  (define-key property-map "x" #'ignore)
  (define-key buffer-map "x" #'forward-char)
  (with-temp-buffer
    (insert "a")
    (goto-char (point-max))
    (use-local-map buffer-map)
    (let ((default-text-properties (list 'local-map property-map 'rear-nonsticky t)))
      (list (key-binding "x")
            (not (null (memq property-map (current-active-maps))))
            (null (get-pos-property (point-max) 'local-map))))))"#,
    );
    let actual = crate::emacs_core::format_eval_result_with_eval(&eval, &result);
    // Generated from GNU 31.1; get_local_map tries the character property first.
    assert_eq!(actual, r#"OK (ignore t t)"#);
}
