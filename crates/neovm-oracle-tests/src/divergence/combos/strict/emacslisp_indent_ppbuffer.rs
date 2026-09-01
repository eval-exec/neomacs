//! Strict combo oracle probes, batch 75: emacs-lisp-mode indentation (the
//! lisp-indent-function rules for defun/let/progn/if/cond bodies) and
//! pp-buffer (pretty-print into a buffer).
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_o9_emacs_lisp_indent_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"(defun foo ()\\n  (let ((x 1))\\n    (+ x 2)))\\n\" \"(let ((x 1)\\n      (y 2))\\n  (+ x y))\\n\" \"(progn\\n  (foo)\\n  (bar)\\n  (baz))\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (with-temp-buffer
        (emacs-lisp-mode)
        (insert "(defun foo ()\n(let ((x 1))\n(+ x 2)))\n")
        (indent-region (point-min) (point-max))
        (buffer-string))
      (with-temp-buffer
        (emacs-lisp-mode)
        (insert "(let ((x 1)\n(y 2))\n(+ x y))\n")
        (indent-region (point-min) (point-max))
        (buffer-string))
      (with-temp-buffer
        (emacs-lisp-mode)
        (insert "(progn\n(foo)\n(bar)\n(baz))\n")
        (indent-region (point-min) (point-max))
        (buffer-string)))
"##,
        expect,
    );
}

#[test]
fn div_o9_emacs_lisp_indent_control_flow() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"(if x\\n    (then)\\n  (else))\\n\" \"(cond\\n ((= x 1)\\n  'one)\\n ((= x 2)\\n  'two))\\n\" \"(while x\\n  (foo)\\n  (bar))\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (with-temp-buffer
        (emacs-lisp-mode)
        (insert "(if x\n(then)\n(else))\n")
        (indent-region (point-min) (point-max))
        (buffer-string))
      (with-temp-buffer
        (emacs-lisp-mode)
        (insert "(cond\n((= x 1)\n'one)\n((= x 2)\n'two))\n")
        (indent-region (point-min) (point-max))
        (buffer-string))
      (with-temp-buffer
        (emacs-lisp-mode)
        (insert "(while x\n(foo)\n(bar))\n")
        (indent-region (point-min) (point-max))
        (buffer-string)))
"##,
        expect,
    );
}

#[test]
fn div_o9_pp_buffer_pretty_print() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"((a . 1) (b . (2 3)) (c . 4) (d . ((nested . val))))\\n\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "((a . 1) (b . (2 3)) (c . 4) (d . ((nested . val))))")
  (pp-buffer)
  (buffer-string))
"##,
        expect,
    );
}

#[test]
fn div_o9_emacs_lisp_indent_keyword_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"(make-process :name \\\"foo\\\"\\n\t      :command '(\\\"echo\\\" \\\"hi\\\")\\n\t      :buffer buf)\\n\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert "(make-process :name \"foo\"\n:command '(\"echo\" \"hi\")\n:buffer buf)\n")
  (indent-region (point-min) (point-max))
  (buffer-string))
"##,
        expect,
    );
}
