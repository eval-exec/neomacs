//! Strict combo oracle probes, batch 125: window pixel metrics after
//! text-scale, process filter with multibyte coding, combo syntax+fontlock+
//! textprop+indent, buffer-swap-text + undo + markers, and format-spec
//! with eval entries. Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_t9_window_pixel_after_text_scale() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((b (get-buffer-create " *probe-tscale*")))
  (unwind-protect
      (progn
        (delete-other-windows)
        (switch-to-buffer b)
        (let ((w0 (list (frame-char-width) (frame-char-height)
                        (window-pixel-width) (window-pixel-height)))
              (cw0 (window-body-width)))
          (text-scale-set 2)
          (list w0
                (frame-char-width)
                (frame-char-height)
                (window-body-width)
                cw0
                (window-pixel-width)
                (window-pixel-height))))
    (when (buffer-live-p b) (kill-buffer b))
    (delete-other-windows)))
"##;
    let expect = expect_test::expect![[r#""OK ((1 1 80 24) 1 1 80 80 80 24)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_t9_combo_syntax_fontlock_textprop_indent() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((log nil))
  (with-temp-buffer
    (emacs-lisp-mode)
    (insert "(defun foo (x y)\n  ;; a comment\n  (let ((z (+ x y)))\n    z))\n")
    (font-lock-fontify-buffer)
    (goto-char 1)
    (push (list 'font-lock (get-text-property 2 'face)) log)
    (push (list 'comment (nth 4 (syntax-pp 30))) log)
    (push (list 'depth (car (syntax-pp 45))) log)
    (indent-region (point-min) (point-max))
    (push (list 'indent (buffer-string)) log)
    (add-text-properties 1 5 '(face error))
    (push (list 'face-at-2 (get-text-property 2 'face)) log)
    (narrow-to-region 1 40)
    (push (list 'narrowed (buffer-string)) log)
    (widen)
    (push (list 'widened (buffer-string)) log)
    (nreverse log)))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function syntax-pp)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_t9_buffer_swap_text_undo_markers() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((a (generate-new-buffer " *probe-swap-a*"))
      (b (generate-new-buffer " *probe-swap-b*"))
      (ma nil) (mb nil))
  (unwind-protect
      (progn
        (with-current-buffer a
          (buffer-enable-undo)
          (insert "alpha-text")
          (setq ma (copy-marker 4)))
        (with-current-buffer b
          (buffer-enable-undo)
          (insert "BETA-DATA")
          (setq mb (copy-marker 2)))
        (with-current-buffer a (buffer-swap-text b))
        (list (with-current-buffer a (buffer-string))
              (with-current-buffer b (buffer-string))
              (marker-position ma)
              (eq (marker-buffer ma) a)
              (marker-position mb)
              (eq (marker-buffer mb) b)
              (with-current-buffer a
                (undo)
                (buffer-string))
              (with-current-buffer b
                (buffer-string))))
    (kill-buffer a)
    (kill-buffer b)))
"##;
    let expect = expect_test::expect![[r#""ERR (user-error \"No further undo information\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_t9_format_spec_with_eval_and_nested() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (format-spec "%a-%b-%c" '((?a . "x") (?b . "y") (?c . "z")))
      (format-spec "100%%" '((?x . "y")))
      (format-spec "[%5a]" '((?a . "hi")))
      (format-spec "nested: %a" '((?a . "outer")))
      (format-spec "" '((?a . "x")))
      (format-spec "%a" '((?a . "")))
      (format-spec "%a%b%a" '((?a . "x") (?b . "y")))
      (format-spec "%a-unknown-%z" '((?a . "x") (?z . "z"))))
"##;
    let expect = expect_test::expect![[
        r#""OK (\"x-y-z\" \"100%\" \"[   hi]\" \"nested: outer\" \"\" \"\" \"xyx\" \"x-unknown-z\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_t9_number_range_and_predicate_edge() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (<= most-negative-fixnum most-positive-fixnum)
      (< (1- most-negative-fixnum) most-negative-fixnum)
      (> (1+ most-positive-fixnum) most-positive-fixnum)
      (integerp (1+ most-positive-fixnum))
      (floatp (1+ most-positive-fixnum))
      (integerp (/ 1 3))
      (integerp (/ 6 3))
      (= 1.0 1)
      (eql 1.0 1)
      (= 0.0 -0.0)
      (eql 0.0 -0.0)
      (< 0.0 -0.0)
      (cl-plusp 0)
      (cl-minusp -1)
      (cl-plusp 0.1)
      (zerop -0.0)
      (cl-oddp (expt 2 64))
      (cl-evenp (expt 2 64)))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function cl-plusp)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
