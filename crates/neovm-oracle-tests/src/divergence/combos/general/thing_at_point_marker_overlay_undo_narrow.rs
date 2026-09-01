//! Deep combo: thing-at-point × marker × overlay × undo × text-prop ×
//! buffer-local × narrow × bounds-of-thing-at-point × forward-thing.
//!
//! Stresses thing-at-point with buffer state: getting/bounds of things
//! (sexp, symbol, word, line, sentence, defun) with markers and overlays.
//! thing-at-point is tricky because it uses syntax tables, text properties,
//! and buffer positions.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_thing_at_point_sexp_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    // sexp at point with markers/overlays; undo after edit.
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-tap")))
    (with-current-buffer buf
      (emacs-lisp-mode)
      (insert "(defun hello (x) (+ x 1))")
      (put-text-property 1 25 'lang 'elisp)
      (let ((m1 (copy-marker 7 nil))
            (m2 (copy-marker 13 t))
            (ov (make-overlay 1 25)))
        (overlay-put ov 'scope 'all)
        (let ((sexp-before (thing-at-point 'sexp))
              (bounds-before (bounds-of-thing-at-point 'sexp)))
          (undo-boundary)
          (goto-char 7)
          (insert "world-")
          (let ((sexp-after (thing-at-point 'sexp))
                (bounds-after (bounds-of-thing-at-point 'sexp))
                (m1-after (marker-position m1))
                (m2-after (marker-position m2)))
            (primitive-undo 1 buffer-undo-list)
            (let ((sexp-restored (thing-at-point 'sexp))
                  (bounds-restored (bounds-of-thing-at-point 'sexp))
                  (m1-restored (marker-position m1))
                  (m2-restored (marker-position m2)))
              (kill-buffer buf)
              (list sexp-before bounds-before
                    sexp-after bounds-after m1-after m2-after
                    sexp-restored bounds-restored m1-restored m2-restored)))))))) "#,
        expect,
    );
}

#[test]
fn combo_thing_at_point_symbol_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 23 31)""#]];
    // symbol at point with markers/overlays; undo after edit.
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-tapsym")))
    (with-current-buffer buf
      (insert "hello-world foo-bar baz-qux")
      (put-text-property 1 11 'sym 'hello-world)
      (put-text-property 13 21 'sym 'foo-bar)
      (put-text-property 23 31 'sym 'baz-qux)
      (let ((m1 (copy-marker 11 nil))
            (m2 (copy-marker 21 t))
            (ov (make-overlay 1 31)))
        (overlay-put ov 'scope 'all)
        (goto-char 5)
        (let ((sym-before (thing-at-point 'symbol))
              (bounds-before (bounds-of-thing-at-point 'symbol)))
          (undo-boundary)
          (goto-char 13)
          (insert "NEW-")
          (let ((sym-after (thing-at-point 'symbol))
                (bounds-after (bounds-of-thing-at-point 'symbol))
                (m1-after (marker-position m1))
                (m2-after (marker-position m2)))
            (primitive-undo 1 buffer-undo-list)
            (let ((sym-restored (thing-at-point 'symbol))
                  (bounds-restored (bounds-of-thing-at-point 'symbol))
                  (m1-restored (marker-position m1))
                  (m2-restored (marker-position m2)))
              (kill-buffer buf)
              (list sym-before bounds-before
                    sym-after bounds-after m1-after m2-after
                    sym-restored bounds-restored m1-restored m2-restored)))))))) "#,
        expect,
    );
}

#[test]
fn combo_thing_at_point_word_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 27 35)""#]];
    // word at point with markers/overlays; undo after edit.
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-tapword")))
    (with-current-buffer buf
      (insert "alpha beta gamma delta epsilon")
      (put-text-property 1 6 'word 'alpha)
      (put-text-property 7 12 'word 'beta)
      (put-text-property 13 19 'word 'gamma)
      (put-text-property 20 26 'word 'delta)
      (put-text-property 27 35 'word 'epsilon)
      (let ((m1 (copy-marker 6 nil))
            (m2 (copy-marker 12 t))
            (ov (make-overlay 1 35)))
        (overlay-put ov 'scope 'all)
        (goto-char 8)
        (let ((word-before (thing-at-point 'word))
              (bounds-before (bounds-of-thing-at-point 'word)))
          (undo-boundary)
          (goto-char 13)
          (insert "INSERTED-")
          (let ((word-after (thing-at-point 'word))
                (bounds-after (bounds-of-thing-at-point 'word))
                (m1-after (marker-position m1))
                (m2-after (marker-position m2)))
            (primitive-undo 1 buffer-undo-list)
            (let ((word-restored (thing-at-point 'word))
                  (bounds-restored (bounds-of-thing-at-point 'word))
                  (m1-restored (marker-position m1))
                  (m2-restored (marker-position m2)))
              (kill-buffer buf)
              (list word-before bounds-before
                    word-after bounds-after m1-after m2-after
                    word-restored bounds-restored m1-restored m2-restored)))))))) "#,
        expect,
    );
}

#[test]
fn combo_thing_at_point_line_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    // line at point with markers/overlays; undo after edit.
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-tapline")))
    (with-current-buffer buf
      (insert "first line\nsecond line\nthird line")
      (put-text-property 1 11 'line 'first)
      (put-text-property 12 23 'line 'second)
      (put-text-property 24 34 'line 'third)
      (let ((m1 (copy-marker 11 nil))
            (m2 (copy-marker 23 t))
            (ov (make-overlay 1 34)))
        (overlay-put ov 'scope 'all)
        (goto-char 15)
        (let ((line-before (thing-at-point 'line))
              (bounds-before (bounds-of-thing-at-point 'line)))
          (undo-boundary)
          (goto-char 12)
          (insert "INSERTED-")
          (let ((line-after (thing-at-point 'line))
                (bounds-after (bounds-of-thing-at-point 'line))
                (m1-after (marker-position m1))
                (m2-after (marker-position m2)))
            (primitive-undo 1 buffer-undo-list)
            (let ((line-restored (thing-at-point 'line))
                  (bounds-restored (bounds-of-thing-at-point 'line))
                  (m1-restored (marker-position m1))
                  (m2-restored (marker-position m2)))
              (kill-buffer buf)
              (list line-before bounds-before
                    line-after bounds-after m1-after m2-after
                    line-restored bounds-restored m1-restored m2-restored)))))))) "#,
        expect,
    );
}

#[test]
fn combo_thing_at_point_narrow_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    // thing-at-point in narrowed buffer; undo after edit.
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-tapnarrow")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
      (put-text-property 1 5 'sect 'a)
      (put-text-property 6 10 'sect 'b)
      (put-text-property 11 15 'sect 'c)
      (put-text-property 16 20 'sect 'd)
      (put-text-property 21 25 'sect 'e)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 10 t))
            (ov (make-overlay 6 20)))
        (overlay-put ov 'zone 'middle)
        (undo-boundary)
        (narrow-to-region 6 20)
        (goto-char (point-min))
        (let ((sexp-narrowed (thing-at-point 'sexp))
              (bounds-narrowed (bounds-of-thing-at-point 'sexp)))
          (insert "XX-")
          (let ((sexp-after (thing-at-point 'sexp))
                (bounds-after (bounds-of-thing-at-point 'sexp))
                (m1-after (marker-position m1))
                (m2-after (marker-position m2)))
            (widen)
            (primitive-undo 1 buffer-undo-list)
            (let ((sexp-restored (thing-at-point 'sexp))
                  (bounds-restored (bounds-of-thing-at-point 'sexp))
                  (m1-restored (marker-position m1))
                  (m2-restored (marker-position m2)))
              (kill-buffer buf)
              (list sexp-narrowed bounds-narrowed
                    sexp-after bounds-after m1-after m2-after
                    sexp-restored bounds-restored m1-restored m2-restored)))))))) "#,
        expect,
    );
}
