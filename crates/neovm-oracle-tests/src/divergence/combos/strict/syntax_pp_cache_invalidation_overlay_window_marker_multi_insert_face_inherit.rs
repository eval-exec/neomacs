//! Strict combo oracle probes, batch 136: syntax-pp cache invalidation,
//! overlay :window parameter, marker insertion-type through multiple
//! insertions, face :inherit chain depth, and cl-defmethod :extra
//! qualifier.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v0_syntax_pp_cache_after_modifications() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert "(defun foo ()\n  (bar) \"comment\")\n(baz)")
  (let ((p1 (car (syntax-pp 5)))
        (p2 (car (syntax-pp 20)))
        (p3 (car (syntax-pp 30))))
    (goto-char 10)
    (insert "XXX")
    (let ((p1b (car (syntax-pp 5)))
          (p2b (car (syntax-pp 23)))
          (p3b (car (syntax-pp 33))))
      (list p1 p2 p3 p1b p2b p3b
            (eq p1 p1b)
            (buffer-string)))))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function syntax-pp)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v0_overlay_window_parameter() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((b (get-buffer-create " *probe-owp*")))
  (unwind-protect
      (progn
        (delete-other-windows)
        (switch-to-buffer b)
        (with-current-buffer b (insert "abcdefghij"))
        (let ((w2 (split-window nil nil 'right))
              (o nil))
          (set-window-buffer w2 b)
          (with-current-buffer b
            (setq o (make-overlay 2 5))
            (overlay-put o 'window w2))
          (select-window w2)
          (let ((visible-here (get-char-property 3 'face))
                (ov-here (overlays-at 3)))
            (overlay-put o 'face 'probe-face)
            (select-window (window-parent w2))
            (let ((visible-parent (get-char-property 3 'face)))
              (list (eq (overlay-get o 'window) w2)
                    visible-here
                    (length ov-here)
                    (overlay-get o 'face)
                    visible-parent
                    (length (overlays-at 3)))))))
    (kill-buffer b)
    (delete-other-windows)))
"##;
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument window-live-p #<window 3>)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v0_marker_insertion_type_multiple() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((m-before (make-marker))
      (m-after (make-marker)))
  (with-temp-buffer
    (insert "ABCDEF")
    (set-marker m-before 3)
    (set-marker-insertion-type m-before nil)
    (set-marker m-after 3)
    (set-marker-insertion-type m-after t)
    (goto-char 3)
    (insert "X")
    (let ((p1 (marker-position m-before))
          (p2 (marker-position m-after)))
      (goto-char 3)
      (insert "Y")
      (let ((p3 (marker-position m-before))
            (p4 (marker-position m-after)))
        (list p1 p2 p3 p4
              (marker-insertion-type m-before)
              (marker-insertion-type m-after)
              (buffer-string))))))
"##;
    let expect = expect_test::expect![[r#""OK (3 4 3 5 nil t \"ABYXCDEF\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v0_face_inherit_chain_depth() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(progn
  (make-face 'probe-f1)
  (make-face 'probe-f2)
  (make-face 'probe-f3)
  (make-face 'probe-f4)
  (set-face-attribute 'probe-f1 nil :foreground "red")
  (set-face-attribute 'probe-f2 nil :inherit 'probe-f1 :weight 'bold)
  (set-face-attribute 'probe-f3 nil :inherit 'probe-f2 :slant 'italic)
  (set-face-attribute 'probe-f4 nil :inherit 'probe-f3 :underline t)
  (list (face-attribute 'probe-f1 :foreground nil 'default)
        (face-attribute 'probe-f4 :foreground nil 'default)
        (face-attribute 'probe-f4 :weight nil 'default)
        (face-attribute 'probe-f4 :slant nil 'default)
        (face-attribute 'probe-f4 :underline nil 'default)
        (face-attribute 'probe-f3 :foreground nil 'default)
        (face-attribute 'probe-f2 :foreground nil 'default)))
"##;
    let expect = expect_test::expect![[r#""OK (\"red\" \"red\" bold italic t \"red\" \"red\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v0_cl_defmethod_extra_qualifier() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((log nil))
  (cl-defgeneric probe-eq (obj))
  (cl-defmethod probe-eq ((obj integer))
    'int)
  (cl-defmethod probe-eq ((obj integer) "extra")
    (push 'extra-int log)
    'extra-int)
  (cl-defmethod probe-eq :extra "extra" ((obj integer))
    (push 'before-extra log))
  (list (probe-eq 42)
        (nreverse log)))
"##;
    let expect = expect_test::expect![[r#""ERR (error \"Invalid argument name: extra\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
