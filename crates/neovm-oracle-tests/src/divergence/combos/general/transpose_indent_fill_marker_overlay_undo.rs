//! Deep combo: transpose-chars × transpose-words × transpose-lines ×
//! indent-region × indent-for-tab-command × fill-paragraph ×
//! center-line × marker × overlay × textprop × undo × buffer-local ×
//! narrow.
//!
//! Stresses text manipulation commands with buffer state: transposition,
//! indentation, filling, and centering. These commands are tricky
//! because they modify buffer content in complex ways and must correctly
//! track markers, overlays, text properties, and undo state.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_transpose_chars_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (1 . 1) 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-tc")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'grp 'a)
      (put-text-property 6 10 'grp 'b)
      (put-text-property 11 15 'grp 'c)
      (put-text-property 16 20 'grp 'd)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 10 t))
            (ov (make-overlay 1 20)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (goto-char 5)
        (transpose-chars)
        (goto-char 12)
        (transpose-chars)
        (let ((after (list (buffer-string)
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'grp)
                           (get-text-property 5 'grp)
                           (get-text-property 10 'grp)
                           (get-text-property 15 'grp))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                (marker-position m1)
                                (marker-position m2)
                                (overlay-start ov) (overlay-end ov)
                                (get-text-property 1 'grp)
                                (get-text-property 6 'grp)
                                (get-text-property 11 'grp)
                                (get-text-property 16 'grp))))
            (kill-buffer buf)
            (list after restored))))))) "#,
        expect,
    );
}

#[test]
fn combo_transpose_words_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 27 35)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-tw")))
    (with-current-buffer buf
      (insert "alpha beta gamma delta epsilon")
      (put-text-property 1 6 'word 'a)
      (put-text-property 7 12 'word 'b)
      (put-text-property 13 19 'word 'c)
      (put-text-property 20 26 'word 'd)
      (put-text-property 27 35 'word 'e)
      (let ((m1 (copy-marker 6 nil))
            (m2 (copy-marker 12 t))
            (ov (make-overlay 1 35)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (goto-char 7)
        (transpose-words)
        (goto-char 20)
        (transpose-words)
        (let ((after (list (buffer-string)
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'word)
                           (get-text-property 7 'word)
                           (get-text-property 13 'word)
                           (get-text-property 20 'word)
                           (get-text-property 27 'word))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                (marker-position m1)
                                (marker-position m2)
                                (overlay-start ov) (overlay-end ov)
                                (get-text-property 1 'word)
                                (get-text-property 7 'word)
                                (get-text-property 13 'word)
                                (get-text-property 20 'word)
                                (get-text-property 27 'word))))
            (kill-buffer buf)
            (list after restored))))))) "#,
        expect,
    );
}

#[test]
fn combo_transpose_lines_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (1 . 1) 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-tl")))
    (with-current-buffer buf
      (insert "line1\nline2\nline3\nline4\nline5")
      (put-text-property 1 6 'line 'l1)
      (put-text-property 7 12 'line 'l2)
      (put-text-property 13 18 'line 'l3)
      (put-text-property 19 24 'line 'l4)
      (put-text-property 25 30 'line 'l5)
      (let ((m1 (copy-marker 6 nil))
            (m2 (copy-marker 12 t))
            (ov (make-overlay 1 30)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (goto-char 7)
        (transpose-lines)
        (goto-char 19)
        (transpose-lines)
        (let ((after (list (buffer-string)
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'line)
                           (get-text-property 7 'line)
                           (get-text-property 13 'line)
                           (get-text-property 19 'line)
                           (get-text-property 25 'line))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                (marker-position m1)
                                (marker-position m2)
                                (overlay-start ov) (overlay-end ov)
                                (get-text-property 1 'line)
                                (get-text-property 7 'line)
                                (get-text-property 13 'line)
                                (get-text-property 19 'line)
                                (get-text-property 25 'line))))
            (kill-buffer buf)
            (list after restored))))))) "#,
        expect,
    );
}

#[test]
fn combo_indent_region_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-ir")))
    (with-current-buffer buf
      (emacs-lisp-mode)
      (insert "(defun test ()\n(let ((x 1))\n(+ x 2)))")
      (put-text-property 1 15 'code 'defun)
      (put-text-property 16 30 'code 'let)
      (put-text-property 31 40 'code 'add)
      (let ((m1 (copy-marker 15 nil))
            (m2 (copy-marker 30 t))
            (ov (make-overlay 1 40)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (indent-region 1 40 nil)
        (let ((after (list (buffer-string)
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'code)
                           (get-text-property 16 'code)
                           (get-text-property 31 'code))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                (marker-position m1)
                                (marker-position m2)
                                (overlay-start ov) (overlay-end ov)
                                (get-text-property 1 'code)
                                (get-text-property 16 'code)
                                (get-text-property 31 'code))))
            (kill-buffer buf)
            (list after restored)))))) "#,
        expect,
    );
}

#[test]
fn combo_indent_narrow_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-inar")))
    (with-current-buffer buf
      (emacs-lisp-mode)
      (insert "AAAA\nBBBB\nCCCC\nDDDD\nEEEE")
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
        (indent-region (point-min) (point-max) nil)
        (widen)
        (let ((after (list (buffer-string)
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'sect)
                           (get-text-property 6 'sect)
                           (get-text-property 16 'sect)
                           (get-text-property 21 'sect))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                (marker-position m1)
                                (marker-position m2)
                                (overlay-start ov) (overlay-end ov)
                                (get-text-property 1 'sect)
                                (get-text-property 6 'sect)
                                (get-text-property 11 'sect)
                                (get-text-property 16 'sect)
                                (get-text-property 21 'sect))))
            (kill-buffer buf)
            (list after restored)))))) "#,
        expect,
    );
}
