//! Deep mega combo: combining many subsystems in a single evaluation chain.
//!
//! marker × overlay × text-prop × undo × buffer-local × narrow ×
//! insert × delete × replace-match × regex × match-data × syntax-ppss ×
//! forward-sexp × window × process × timer × advice × pcase × cl-lib ×
//! coding-system × font-lock × register × thing-at-point × undo-boundary.
//!
//! This is a stress test that exercises the interaction between many
//! subsystems in a single evaluation chain. If any subsystem has a
//! subtle bug that only manifests when combined with others, this test
//! should catch it.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn mega_combo_all_subsystems_stress() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 74 78)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (require 'cl-lib)
  (let ((buf (generate-new-buffer " mega-all")))
    (with-current-buffer buf
      (emacs-lisp-mode)
      (make-local-variable 'mega-local)
      (setq mega-local 'buffer-specific)
      ;; Initial content
      (insert "(defun test-func ()\n  (let ((x 1))\n    (+ x 2)))\n\n;; comment\nAAAA-BBBB-CCCC")
      ;; Text properties
      (put-text-property 1 30 'code 'defun)
      (put-text-property 32 45 'code 'let)
      (put-text-property 47 56 'code 'add)
      (put-text-property 58 67 'comment 'header)
      (put-text-property 69 73 'data 'a)
      (put-text-property 74 78 'data 'b)
      (put-text-property 79 83 'data 'c)
      ;; Markers with different insertion types
      (let ((m1 (copy-marker 18 nil))  ;; end of defun line
            (m2 (copy-marker 30 t))     ;; end of defun block
            (m3 (copy-marker 56 nil))   ;; end of add
            (m4 (copy-marker 67 t))     ;; end of comment
            (m5 (copy-marker 78 nil))   ;; end of BBBB
            ;; Overlays with priorities
            (ov-code (make-overlay 1 56))
            (ov-comment (make-overlay 58 67))
            (ov-data (make-overlay 69 83)))
        (overlay-put ov-code 'kind 'code)
        (overlay-put ov-code 'priority 1)
        (overlay-put ov-comment 'kind 'comment)
        (overlay-put ov-comment 'priority 2)
        (overlay-put ov-data 'kind 'data)
        (overlay-put ov-data 'priority 3)
        ;; Save register
        (point-to-register ?r)
        ;; First undo boundary
        (undo-boundary)
        ;; Narrow and edit
        (narrow-to-region 30 56)
        (goto-char (point-min))
        (insert "  (setq y 3)\n")
        (widen)
        ;; Second undo boundary
        (undo-boundary)
        ;; Regex replace in data section
        (goto-char 69)
        (while (re-search-forward "AAAA\\|BBBB\\|CCCC" nil t)
          (replace-match "XX"))
        ;; Third undo boundary
        (undo-boundary)
        ;; Timer modification
        (let ((done nil))
          (run-with-timer 0.05 nil
            (lambda ()
              (with-current-buffer buf
                (goto-char (point-max))
                (insert "\n;; timer-added")
                (setq done t))))
          (while (not done) (accept-process-output nil 0.05))
          (sit-for 0.1))
        ;; Record state
        (let ((state-3 (list (buffer-string)
                             mega-local
                             (marker-position m1)
                             (marker-position m2)
                             (marker-position m3)
                             (marker-position m4)
                             (marker-position m5)
                             (overlay-get (car (overlays-at 1)) 'kind)
                             (overlay-get (car (overlays-at 60)) 'kind)
                             (overlay-get (car (overlays-at 75)) 'kind)
                             (get-text-property 1 'code)
                             (get-text-property 58 'comment)
                             (get-text-property 69 'data)
                             (syntax-ppss 35)
                             (thing-at-point 'defun))))
          ;; Undo timer addition
          (primitive-undo 1 buffer-undo-list)
          (let ((state-2 (list (buffer-string)
                               (marker-position m1)
                               (marker-position m2)
                               (marker-position m3)
                               (marker-position m4)
                               (marker-position m5)
                               (overlay-get (car (overlays-at 1)) 'kind)
                               (overlay-get (car (overlays-at 60)) 'kind)
                               (overlay-get (car (overlays-at 75)) 'kind))))
            ;; Undo regex replace
            (primitive-undo 1 buffer-undo-list)
            (let ((state-1 (list (buffer-string)
                                 (marker-position m1)
                                 (marker-position m2)
                                 (marker-position m3)
                                 (marker-position m4)
                                 (marker-position m5)
                                 (get-text-property 69 'data)
                                 (get-text-property 74 'data)
                                 (get-text-property 79 'data))))
              ;; Undo narrow edit
              (primitive-undo 1 buffer-undo-list)
              (let ((state-0 (list (buffer-string)
                                   mega-local
                                   (marker-position m1)
                                   (marker-position m2)
                                   (marker-position m3)
                                   (marker-position m4)
                                   (marker-position m5)
                                   (overlay-get (car (overlays-at 1)) 'kind)
                                   (overlay-get (car (overlays-at 35)) 'kind)
                                   (overlay-get (car (overlays-at 60)) 'kind)
                                   (overlay-get (car (overlays-at 75)) 'kind)
                                   (get-text-property 1 'code)
                                   (get-text-property 32 'code)
                                   (get-text-property 58 'comment)
                                   (get-text-property 69 'data)
                                   (syntax-ppss 35)
                                   (thing-at-point 'defun))))
                (kill-buffer buf)
                (list state-3 state-2 state-1 state-0))))))))) "#,
        expect,
    );
}

#[test]
fn mega_combo_marker_overlay_undo_regex_narrow_textprop() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " mega-moun")))
    (with-current-buffer buf
      (insert "alpha:100 beta:200 gamma:300 delta:400 epsilon:500")
      (put-text-property 1 10 'grp 'g1)
      (put-text-property 11 20 'grp 'g2)
      (put-text-property 21 30 'grp 'g3)
      (put-text-property 31 40 'grp 'g4)
      (put-text-property 41 51 'grp 'g5)
      (let ((m1 (copy-marker 10 nil))
            (m2 (copy-marker 20 t))
            (m3 (copy-marker 30 nil))
            (m4 (copy-marker 40 t))
            (ov1 (make-overlay 1 30))
            (ov2 (make-overlay 21 51)))
        (overlay-put ov1 'zone 'first)
        (overlay-put ov1 'priority 1)
        (overlay-put ov2 'zone 'second)
        (overlay-put ov2 'priority 2)
        ;; First edit: narrow + insert
        (undo-boundary)
        (narrow-to-region 11 40)
        (goto-char (point-min))
        (insert "XX-")
        (goto-char (point-max))
        (insert "-YY")
        (widen)
        ;; Second edit: regex replace
        (undo-boundary)
        (goto-char 1)
        (while (re-search-forward "\\([a-z]+\\):\\([0-9]+\\)" nil t)
          (replace-match "\\1=\\2" t))
        ;; Record state
        (let ((state-2 (list (buffer-string)
                             (marker-position m1)
                             (marker-position m2)
                             (marker-position m3)
                             (marker-position m4)
                             (overlay-start ov1) (overlay-end ov1)
                             (overlay-start ov2) (overlay-end ov2)
                             (overlay-get (car (overlays-at 1)) 'zone)
                             (overlay-get (car (overlays-at 25)) 'zone)
                             (overlay-get (car (overlays-at 45)) 'zone)
                             (get-text-property 1 'grp)
                             (get-text-property 11 'grp)
                             (get-text-property 21 'grp)
                             (get-text-property 31 'grp)
                             (get-text-property 41 'grp))))
          ;; Undo regex replace
          (primitive-undo 1 buffer-undo-list)
          (let ((state-1 (list (buffer-string)
                               (marker-position m1)
                               (marker-position m2)
                               (marker-position m3)
                               (marker-position m4)
                               (overlay-start ov1) (overlay-end ov1)
                               (overlay-start ov2) (overlay-end ov2)
                               (overlay-get (car (overlays-at 1)) 'zone)
                               (overlay-get (car (overlays-at 25)) 'zone)
                               (overlay-get (car (overlays-at 45)) 'zone)
                               (get-text-property 1 'grp)
                               (get-text-property 11 'grp)
                               (get-text-property 21 'grp)
                               (get-text-property 31 'grp)
                               (get-text-property 41 'grp))))
            ;; Undo narrow edit
            (primitive-undo 1 buffer-undo-list)
            (let ((state-0 (list (buffer-string)
                                 (marker-position m1)
                                 (marker-position m2)
                                 (marker-position m3)
                                 (marker-position m4)
                                 (overlay-start ov1) (overlay-end ov1)
                                 (overlay-start ov2) (overlay-end ov2)
                                 (overlay-get (car (overlays-at 1)) 'zone)
                                 (overlay-get (car (overlays-at 25)) 'zone)
                                 (overlay-get (car (overlays-at 45)) 'zone)
                                 (get-text-property 1 'grp)
                                 (get-text-property 11 'grp)
                                 (get-text-property 21 'grp)
                                 (get-text-property 31 'grp)
                                 (get-text-property 41 'grp))))
              (kill-buffer buf)
              (list state-2 state-1 state-0)))))))) "#,
        expect,
    );
}

#[test]
fn mega_combo_cl_letf_pcase_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (require 'cl-lib)
  (let ((buf (generate-new-buffer " mega-clpc")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'grp 'a)
      (put-text-property 6 10 'grp 'b)
      (put-text-property 11 15 'grp 'c)
      (put-text-property 16 20 'grp 'd)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 10 t))
            (m3 (copy-marker 15 nil))
            (ov (make-overlay 1 20)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        ;; cl-letf with pcase inside
        (cl-letf (((symbol-function 'my-insert)
                   (lambda (text)
                     (pcase text
                       ("X" (insert "-X-"))
                       ("Y" (insert "-Y-"))
                       ("Z" (insert "-Z-"))
                       (_ (insert text))))))
          (goto-char 5)
          (funcall 'my-insert "X")
          (goto-char 13)
          (funcall 'my-insert "Y")
          (goto-char (point-max))
          (funcall 'my-insert "Z"))
        (let ((after (list (buffer-string)
                           (marker-position m1)
                           (marker-position m2)
                           (marker-position m3)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'grp)
                           (get-text-property 6 'grp)
                           (get-text-property 12 'grp)
                           (get-text-property 18 'grp))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                (marker-position m1)
                                (marker-position m2)
                                (marker-position m3)
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
fn mega_combo_register_window_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 6 6)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " mega-regwin")))
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
        ;; Save to register
        (copy-to-register ?r 1 5)
        (point-to-register ?p)
        (undo-boundary)
        ;; Split window and edit
        (let* ((win1 (selected-window))
               (win2 (split-window win1 nil 'below)))
          (unwind-protect
              (with-selected-window win2
                (goto-char 10)
                (insert-register ?r)
                (let ((after (list (buffer-string)
                                   (marker-position m1)
                                   (marker-position m2)
                                   (overlay-start ov) (overlay-end ov)
                                   (window-point win1)
                                   (window-point win2)
                                   (get-text-property 1 'grp)
                                   (get-text-property 6 'grp)
                                   (get-text-property 11 'grp)
                                   (get-text-property 16 'grp))))
                  (primitive-undo 1 buffer-undo-list)
                  (let ((restored (list (buffer-string)
                                        (marker-position m1)
                                        (marker-position m2)
                                        (overlay-start ov) (overlay-end ov)
                                        (window-point win1)
                                        (window-point win2)
                                        (get-text-property 1 'grp)
                                        (get-text-property 6 'grp)
                                        (get-text-property 11 'grp)
                                        (get-text-property 16 'grp))))
                    (list after restored))))
            (delete-window win2)
            (kill-buffer buf))))))) "#,
        expect,
    );
}

#[test]
fn mega_combo_coding_fontlock_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 37 46)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " mega-codfl")))
    (with-current-buffer buf
      (emacs-lisp-mode)
      (make-local-variable 'mega-coding-local)
      (setq mega-coding-local 'active)
      (insert "(defun αβγ ()\n  (let ((x 1))\n    (+ x 2)))")
      (put-text-property 1 20 'code 'defun)
      (put-text-property 22 35 'code 'let)
      (put-text-property 37 46 'code 'add)
      (let ((m1 (copy-marker 14 nil))
            (m2 (copy-marker 20 t))
            (ov (make-overlay 1 46)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        ;; Insert multibyte at marker position
        (goto-char 14)
        (insert "δε")
        ;; Regex replace
        (goto-char 1)
        (while (re-search-forward "x" nil t)
          (replace-match "var"))
        (let ((after (list (buffer-string)
                           mega-coding-local
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'code)
                           (get-text-property 22 'code)
                           (get-text-property 37 'code)
                           (syntax-ppss 20))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                mega-coding-local
                                (marker-position m1)
                                (marker-position m2)
                                (overlay-start ov) (overlay-end ov)
                                (get-text-property 1 'code)
                                (get-text-property 22 'code)
                                (get-text-property 37 'code)
                                (syntax-ppss 20))))
            (kill-buffer buf)
            (list after restored))))))) "#,
        expect,
    );
}
