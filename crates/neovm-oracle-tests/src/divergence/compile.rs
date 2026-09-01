//! Compilation-mode coverage (thin area: ~9 prior files).
//!
//! The deterministic, batch-testable side of compile: parsing compiler/grep
//! output into compilation-message text properties (error type/line), error
//! counts, and derived-mode setup. Avoids next-error's source-file opening.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

fn _u() {}

#[test]
fn div_comp_mode_parse_error_lines() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    _u();
    let expect = expect_test::expect![[r#""OK 3""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn (require 'compile)
  (with-temp-buffer
    (insert "gcc -c foo.c\nfoo.c:10:5: error: expected semicolon\nfoo.c:20:1: note: defined here\nbar.c:3: warning: unused\n")
    (compilation-mode)
    (font-lock-fontify-buffer)
    (let ((count 0) (pt (point-min)))
      (while (setq pt (next-single-property-change pt 'compilation-message))
        (when (get-text-property pt 'compilation-message) (setq count (1+ count))))
      count)))
"##,
        expect,
    );
}

#[test]
fn div_comp_message_type_at_first_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK :none""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn (require 'compile)
  (with-temp-buffer
    (insert "foo.c:10:5: error: boom\n")
    (compilation-mode)
    (font-lock-fontify-buffer)
    (let* ((pt (next-single-property-change (point-min) 'compilation-message))
           (msg (and pt (get-text-property pt 'compilation-message))))
      (if msg (list (car msg) (nth 1 msg)) :none))))
"##,
        expect,
    );
}

#[test]
fn div_comp_grep_mode_parse() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn (require 'compile)
  (with-temp-buffer
    (insert "foo.c:10:hello\nbar.c:20:world\n")
    (grep-mode)
    (font-lock-fontify-buffer)
    (let ((count 0) (pt (point-min)))
      (while (setq pt (next-single-property-change pt 'compilation-message))
        (when (get-text-property pt 'compilation-message) (setq count (1+ count))))
      count)))
"##,
        expect,
    );
}

#[test]
fn div_comp_error_regexp_alist_count() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn (require 'compile)
  (list (> (length compilation-error-regexp-alist) 10)
        (> (length (compilation-mode-font-lock-keywords)) 0)))
"##,
        expect,
    );
}

#[test]
fn div_comp_mode_buffer_mode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn (require 'compile)
  (with-temp-buffer
    (compilation-mode)
    (list (eq major-mode 'compilation-mode)
          (boundp 'compilation-error)
          (boundp 'compilation-current-error))))
"##,
        expect,
    );
}

#[test]
fn div_comp_parse_filename_extraction() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK :no-loc""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn (require 'compile)
  (with-temp-buffer
    (insert "/abs/path/foo.c:42:7: error: x\n")
    (compilation-mode)
    (font-lock-fontify-buffer)
    (let* ((pt (next-single-property-change (point-min) 'compilation-message))
           (msg (and pt (get-text-property pt 'compilation-message)))
           (loc (and msg (cadr msg))))
      (if loc (list (nth 0 loc) (nth 1 loc)) :no-loc))))
"##,
        expect,
    );
}

#[test]
fn div_comp_warning_vs_error_classification() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (wrong-type-argument listp #s(compilation--message (1 1 ((\"b.c\" nil) nil (1 #2)) nil nil) 1 nil gnu))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn (require 'compile)
  (with-temp-buffer
    (insert "a.c:1:1: error: e\nb.c:1:1: warning: w\nc.c:1:1: note: n\n")
    (compilation-mode)
    (font-lock-fontify-buffer)
    (let (types pt)
      (setq pt (point-min))
      (while (setq pt (next-single-property-change pt 'compilation-message))
        (let ((msg (get-text-property pt 'compilation-message)))
          (when msg (push (car msg) types))))
      (reverse types))))
"##,
        expect,
    );
}

#[test]
fn div_comp_count_function() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (5 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn (require 'compile)
  (condition-case e
      (with-temp-buffer
        (insert "foo.c:1:1: error: e\nbar.c:1:1: error: e2\n")
        (compilation-mode)
        (font-lock-fontify-buffer)
        (list (length compilation-error) compilation-num-errors-found))
    (error (cons 'errored (car e)))))
"##,
        expect,
    );
}
