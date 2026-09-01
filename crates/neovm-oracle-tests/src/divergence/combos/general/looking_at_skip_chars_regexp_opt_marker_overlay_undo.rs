//! Deep combo: looking-at × skip-chars-forward × skip-chars-backward ×
//! skip-syntax-forward × skip-syntax-backward × regexp-opt ×
//! marker × overlay × textprop × undo × buffer-local × narrow.
//!
//! Stresses scanning commands with buffer state: looking-at, skip-chars,
//! skip-syntax, and regexp-opt must interact correctly with markers,
//! overlays, text properties, and undo. Scanning commands are tricky
//! because they move point and must track marker positions correctly.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_looking_at_skip_chars_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-lasc")))
    (with-current-buffer buf
      (insert "aaa bbb ccc ddd eee")
      (put-text-property 1 4 'word 'a)
      (put-text-property 5 8 'word 'b)
      (put-text-property 9 12 'word 'c)
      (put-text-property 13 16 'word 'd)
      (put-text-property 17 20 'word 'e)
      (let ((m1 (copy-marker 4 nil))
            (m2 (copy-marker 8 t))
            (ov (make-overlay 1 20)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (goto-char 1)
        (let ((la1 (looking-at "[a-z]+"))
              (sc1 (progn (skip-chars-forward "a-z") (point)))
              (sc2 (progn (skip-chars-forward " ") (point)))
              (la2 (looking-at "[a-z]+"))
              (sc3 (progn (skip-chars-forward "a-z") (point))))
          (goto-char 5)
          (insert "XX")
          (let ((after (list (buffer-string)
                             la1 sc1 la2 sc3
                             (marker-position m1)
                             (marker-position m2)
                             (overlay-start ov) (overlay-end ov)
                             (get-text-property 1 'word)
                             (get-text-property 5 'word)
                             (get-text-property 10 'word))))
            (primitive-undo 1 buffer-undo-list)
            (let ((restored (list (buffer-string)
                                  (marker-position m1)
                                  (marker-position m2)
                                  (overlay-start ov) (overlay-end ov)
                                  (get-text-property 1 'word)
                                  (get-text-property 5 'word)
                                  (get-text-property 9 'word)
                                  (get-text-property 13 'word)
                                  (get-text-property 17 'word))))
              (kill-buffer buf)
              (list after restored)))))))) "#,
        expect,
    );
}

#[test]
fn combo_skip_syntax_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-ssyn")))
    (with-current-buffer buf
      (emacs-lisp-mode)
      (insert "(defun hello ()\n  (message \"hi\"))")
      (put-text-property 1 30 'code 'defun)
      (let ((m1 (copy-marker 7 nil))
            (m2 (copy-marker 16 t))
            (ov (make-overlay 1 30)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (goto-char 1)
        (let ((ss1 (progn (skip-syntax-forward "(") (point)))
              (ss2 (progn (skip-syntax-forward "w_") (point)))
              (ss3 (progn (skip-syntax-forward " ") (point))))
          (goto-char 7)
          (insert "world-")
          (let ((after (list (buffer-string)
                             ss1 ss2 ss3
                             (marker-position m1)
                             (marker-position m2)
                             (overlay-start ov) (overlay-end ov)
                             (get-text-property 1 'code)
                             (get-text-property 7 'code))))
            (primitive-undo 1 buffer-undo-list)
            (let ((restored (list (buffer-string)
                                  (marker-position m1)
                                  (marker-position m2)
                                  (overlay-start ov) (overlay-end ov)
                                  (get-text-property 1 'code)
                                  (get-text-property 7 'code)
                                  (get-text-property 14 'code))))
              (kill-buffer buf)
              (list after restored)))))))) "#,
        expect,
    );
}

#[test]
fn combo_regexp_opt_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-ropt"))
        (pattern (regexp-opt '("defun" "defvar" "defconst" "defcustom"))))
    (with-current-buffer buf
      (emacs-lisp-mode)
      (insert "(defun foo () nil)\n(defvar x 1)\n(defconst y 2)")
      (put-text-property 1 18 'kind 'defun)
      (put-text-property 20 34 'kind 'defvar)
      (put-text-property 36 51 'kind 'defconst)
      (let ((m1 (copy-marker 18 nil))
            (m2 (copy-marker 34 t))
            (ov (make-overlay 1 51)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (goto-char 1)
        (let ((matches nil))
          (while (re-search-forward pattern nil t)
            (push (list (match-string 0) (match-beginning 0) (match-end 0))
                  matches))
          (setq matches (nreverse matches)))
        (goto-char 18)
        (insert "\n(defmacro bar () nil)")
        (let ((after (list (buffer-string)
                           matches
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'kind)
                           (get-text-property 20 'kind)
                           (get-text-property 36 'kind))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                (marker-position m1)
                                (marker-position m2)
                                (overlay-start ov) (overlay-end ov)
                                (get-text-property 1 'kind)
                                (get-text-property 20 'kind)
                                (get-text-property 36 'kind)
                                (get-text-property 38 'kind))))
            (kill-buffer buf)
            (list after restored)))))) "#,
        expect,
    );
}

#[test]
fn combo_skip_chars_narrow_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-scnar")))
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
        (let ((sc1 (progn (skip-chars-forward "A-Z") (point)))
              (sc2 (progn (skip-chars-forward "-") (point)))
              (sc3 (progn (skip-chars-forward "A-Z") (point))))
          (goto-char (point-min))
          (insert "XX-")
          (widen)
          (let ((after (list (buffer-string)
                             sc1 sc2 sc3
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
              (list after restored)))))))) "#,
        expect,
    );
}

#[test]
fn combo_looking_at_buffer_local_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-labl")))
    (with-current-buffer buf
      (make-local-variable 'la-local)
      (setq la-local 'buffer-specific)
      (insert "alpha:100 beta:200 gamma:300")
      (put-text-property 1 10 'grp 'g1)
      (put-text-property 11 20 'grp 'g2)
      (put-text-property 21 29 'grp 'g3)
      (let ((m1 (copy-marker 10 nil))
            (m2 (copy-marker 20 t))
            (ov (make-overlay 1 29)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (goto-char 1)
        (let ((la1 (looking-at "[a-z]+:[0-9]+"))
              (sc1 (progn (skip-chars-forward "a-z:") (point)))
              (la2 (looking-at "[0-9]+"))
              (sc2 (progn (skip-chars-forward "0-9") (point))))
          (goto-char 10)
          (insert "-INSERTED-")
          (let ((after (list (buffer-string)
                             la1 sc1 la2 sc2
                             la-local
                             (marker-position m1)
                             (marker-position m2)
                             (overlay-start ov) (overlay-end ov)
                             (get-text-property 1 'grp)
                             (get-text-property 11 'grp)
                             (get-text-property 21 'grp))))
            (primitive-undo 1 buffer-undo-list)
            (let ((restored (list (buffer-string)
                                  la-local
                                  (marker-position m1)
                                  (marker-position m2)
                                  (overlay-start ov) (overlay-end ov)
                                  (get-text-property 1 'grp)
                                  (get-text-property 11 'grp)
                                  (get-text-property 21 'grp))))
              (kill-buffer buf)
              (list after restored)))))))) "#,
        expect,
    );
}
