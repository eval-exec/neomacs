//! Deep combo: syntax-table × syntax-ppss × modify-syntax-entry ×
//! marker × overlay × text-prop × undo × buffer-local × narrow ×
//! forward-comment × backward-comment × forward-sexp × backward-sexp.
//!
//! Stresses syntax table interaction with buffer state: syntax parsing,
//! syntax modifications, and sexp navigation. Syntax tables are tricky
//! because they affect parsing globally and must interact correctly
//! with buffer-local overrides.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_syntax_ppss_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-sppss")))
    (with-current-buffer buf
      (emacs-lisp-mode)
      (insert "(defun hello (x)\n  (+ x 1))")
      (put-text-property 1 25 'lang 'elisp)
      (let ((m1 (copy-marker 7 nil))
            (m2 (copy-marker 13 t))
            (ov (make-overlay 1 25)))
        (overlay-put ov 'scope 'all)
        (let ((ppss-before (syntax-ppss 7)))
          (undo-boundary)
          (goto-char 7)
          (insert "world-")
          (let ((ppss-after (syntax-ppss 13))
                (after (list (buffer-string)
                             (marker-position m1)
                             (marker-position m2)
                             (overlay-start ov) (overlay-end ov)
                             (get-text-property 1 'lang))))
            (primitive-undo 1 buffer-undo-list)
            (let ((ppss-restored (syntax-ppss 7))
                  (restored (list (buffer-string)
                                  (marker-position m1)
                                  (marker-position m2)
                                  (overlay-start ov) (overlay-end ov)
                                  (get-text-property 1 'lang))))
              (kill-buffer buf)
              (list ppss-before after ppss-after ppss-restored restored)))))))) "#,
        expect,
    );
}

#[test]
fn combo_modify_syntax_entry_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((#(\"AAAA-BBBB-CCCC\" 0 4 (zone a) 5 9 (zone b) 10 14 (zone c)) 5 10 1 15 a b 119) (32 5 10 1 15 a b c))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-mse")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 10 t))
            (ov (make-overlay 1 15))
            (saved-syntax (syntax-table)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (modify-syntax-entry ?- "w")
        (let ((after-mod (list (buffer-string)
                               (marker-position m1)
                               (marker-position m2)
                               (overlay-start ov) (overlay-end ov)
                               (get-text-property 1 'zone)
                               (get-text-property 6 'zone)
                               (char-syntax ?-))))
          (modify-syntax-entry ?- " ")
          (let ((after-restore (list (char-syntax ?-)
                                     (marker-position m1)
                                     (marker-position m2)
                                     (overlay-start ov) (overlay-end ov)
                                     (get-text-property 1 'zone)
                                     (get-text-property 6 'zone)
                                     (get-text-property 11 'zone))))
            (kill-buffer buf)
            (list after-mod after-restore))))))) "#,
        expect,
    );
}

#[test]
fn combo_forward_sexp_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-fsexp")))
    (with-current-buffer buf
      (emacs-lisp-mode)
      (insert "(aaa (bbb (ccc))) (ddd)")
      (put-text-property 1 18 'tree 'nested)
      (put-text-property 20 24 'tree 'flat)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 18 t))
            (ov (make-overlay 1 24)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (goto-char 1)
        (forward-sexp)
        (let ((pos-after-fwd (point)))
          (goto-char (point-max))
          (backward-sexp)
          (let ((pos-after-bwd (point))
                (after (list (buffer-string)
                             (marker-position m1)
                             (marker-position m2)
                             (overlay-start ov) (overlay-end ov)
                             (get-text-property 1 'tree)
                             (get-text-property 20 'tree))))
            (primitive-undo 1 buffer-undo-list)
            (let ((restored (list (buffer-string)
                                  (marker-position m1)
                                  (marker-position m2)
                                  (overlay-start ov) (overlay-end ov)
                                  (get-text-property 1 'tree)
                                  (get-text-property 20 'tree))))
              (kill-buffer buf)
              (list pos-after-fwd pos-after-bwd after restored)))))))) "#,
        expect,
    );
}

#[test]
fn combo_forward_comment_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 31 34)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-fcomm")))
    (with-current-buffer buf
      (emacs-lisp-mode)
      (insert "aaa ;; comment\nbbb ;; other\nccc")
      (put-text-property 1 4 'word 'aaa)
      (put-text-property 16 19 'word 'bbb)
      (put-text-property 31 34 'word 'ccc)
      (let ((m1 (copy-marker 4 nil))
            (m2 (copy-marker 19 t))
            (ov (make-overlay 1 34)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (goto-char 1)
        (forward-comment 1)
        (let ((pos-after-fwd (point)))
          (goto-char (point-max))
          (backward-comment 1)
          (let ((pos-after-bwd (point))
                (after (list (buffer-string)
                             (marker-position m1)
                             (marker-position m2)
                             (overlay-start ov) (overlay-end ov)
                             (get-text-property 1 'word)
                             (get-text-property 16 'word)
                             (get-text-property 31 'word))))
            (primitive-undo 1 buffer-undo-list)
            (let ((restored (list (buffer-string)
                                  (marker-position m1)
                                  (marker-position m2)
                                  (overlay-start ov) (overlay-end ov)
                                  (get-text-property 1 'word)
                                  (get-text-property 16 'word)
                                  (get-text-property 31 'word))))
              (kill-buffer buf)
              (list pos-after-fwd pos-after-bwd after restored)))))))) "#,
        expect,
    );
}

#[test]
fn combo_syntax_table_buffer_local_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-stbl")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 10 t))
            (ov (make-overlay 1 15))
            (local-table (make-syntax-table)))
        (overlay-put ov 'scope 'all)
        (modify-syntax-entry ?- "w" local-table)
        (set-syntax-table local-table)
        (undo-boundary)
        (goto-char 1)
        (forward-word)
        (let ((pos-after-fwd (point))
              (after (list (buffer-string)
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'zone)
                           (get-text-property 6 'zone))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                (marker-position m1)
                                (marker-position m2)
                                (overlay-start ov) (overlay-end ov)
                                (get-text-property 1 'zone)
                                (get-text-property 6 'zone)
                                (get-text-property 11 'zone))))
            (kill-buffer buf)
            (list pos-after-fwd after restored))))))) "#,
        expect,
    );
}
