//! Strict combo oracle probes, batch 85: COMPLEX multi-subsystem interactions
//! — syntax text-property affecting parse-partial-sexp + indent-region, overlay
//! modification-hooks + text-properties + delete-region, nested
//! catch/condition-case/unwind-protect/dotimes, and narrowing + marker +
//! overlay + text-properties + buffer-substring.
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_p9_syntax_textprop_parse_and_indent() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (2 nil \"(defun foo ()\\n  (let ((x (bar 1 2)))\\n    x))\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert "(defun foo ()\n  (let ((x (bar 1 2)))\n    x))\n")
  (put-text-property 10 15 'syntax-table (string-to-syntax "_"))
  (list (nth 0 (parse-partial-sexp 1 20))
        (nth 3 (parse-partial-sexp 1 20))
        (progn (indent-region (point-min) (point-max)) (buffer-string))))
"##,
        expect,
    );
}

#[test]
fn div_p9_overlay_mod_hooks_textprop_delete() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#(\"abcghij\" 2 3 (face bold)) nil 2 5 ((mod nil 4 7) (mod t 4 4)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((log nil))
  (with-temp-buffer
    (insert "abcdefghij")
    (add-text-properties 3 6 '(face bold))
    (let ((o (make-overlay 2 8)))
      (overlay-put o 'modification-hooks
                   (list (lambda (ov after beg end &optional len)
                           (push (list 'mod after beg end) log))))
      (delete-region 4 7)
      (list (buffer-string)
            (text-properties-at 2)
            (overlay-start o)
            (overlay-end o)
            (nreverse log)))))
"##,
        expect,
    );
}

#[test]
fn div_p9_nested_catch_condition_unwind_dolist() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 1 2 cleanup)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((result nil))
  (catch 'outer
    (condition-case err
        (unwind-protect
            (dotimes (i 5)
              (when (= i 3)
                (throw 'outer 'caught-at-3))
              (push i result))
          (push 'cleanup result))
      (error (push 'caught-error result))))
  (nreverse result))
"##,
        expect,
    );
}

#[test]
fn div_p9_narrow_marker_overlay_textprop_substring() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#(\"CDEFG\" 0 1 (category cat-a)) \"CDEFG\" 5 t 3 7 (category cat-a))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((b1 (generate-new-buffer " *probe-combo-n*")))
  (unwind-protect
      (with-current-buffer b1
        (insert "ABCDEFGHIJ")
        (let ((m (set-marker (make-marker) 5))
              (o (make-overlay 3 7)))
          (overlay-put o 'face 'bold)
          (add-text-properties 2 4 '(category cat-a))
          (narrow-to-region 3 8)
          (list (buffer-substring (point-min) (point-max))
                (buffer-substring-no-properties (point-min) (point-max))
                (marker-position m)
                (eq (marker-buffer m) b1)
                (overlay-start o)
                (overlay-end o)
                (text-properties-at (point-min)))))
    (kill-buffer b1)))
"##,
        expect,
    );
}

#[test]
fn div_p9_process_buffer_undo_fontification_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"before output\\n\" t t \"Undo\" \"before \")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (generate-new-buffer " *probe-combo-p*")))
  (unwind-protect
      (with-current-buffer buf
        (buffer-enable-undo)
        (insert "before ")
        (undo-boundary)
        (let ((proc (make-process :name "probe-combo-proc"
                                  :command (list shell-file-name shell-command-switch "echo output")
                                  :buffer buf
                                  :sentinel (lambda (&rest _) nil))))
          (set-process-query-on-exit-flag proc nil)
          (accept-process-output proc 1))
        (undo-boundary)
        (list (buffer-string)
              (buffer-modified-p)
              (consp buffer-undo-list)
              (undo)
              (buffer-string)))
    (kill-buffer buf)))
"##,
        expect,
    )
}
