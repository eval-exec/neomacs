//! Oracle parity tests for GNU `pp.el` pretty-printing semantics.
//!
//! GNU `pp-to-string` dispatches through `pp-default-function`, binds printing
//! variables such as `print-escape-newlines`, and preserves the traditional
//! trailing newline behavior.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_prop_pp_to_string_basic_objects_and_trailing_newline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (require 'pp)
  (list
   (pp-to-string '(alpha beta gamma))
   (pp-to-string [1 2 (three . four)])
   (pp-to-string '(quote symbol))
   (string-suffix-p "\n" (pp-to-string '(a b)))))
"#;

    let expect = expect_test::expect![[
        r#""OK (\"(alpha beta gamma)\\n\" \"[1 2 (three . four)]\\n\" \"'symbol\\n\" t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_pp_escape_newlines_binding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (require 'pp)
  (let ((pp-escape-newlines t))
    (setq a (pp-to-string "a\nb")))
  (let ((pp-escape-newlines nil))
    (setq b (pp-to-string "a\nb")))
  (list a b))
"#;

    let expect = expect_test::expect![[r#""OK (\"\\\"a\\\\nb\\\"\\n\" \"\\\"a\\nb\\\"\\n\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_pp_to_string_custom_function_dispatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (require 'pp)
  (let ((calls nil))
    (list
     (pp-to-string '(one two)
                   (lambda (object)
                     (push (list 'object object) calls)
                     (prin1 (list 'custom object) (current-buffer))))
     calls))
"#;

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_pp_buffer_multiple_objects_and_comments() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (require 'pp)
  (with-temp-buffer
    (emacs-lisp-mode)
    (insert ";; first\n(a b c)\n;; second\n(d e f)")
    (pp-buffer)
    (buffer-string)))
"#;

    let expect = expect_test::expect![[r#""OK \";; first\\n(a b c)\\n;; second\\n(d e f)\\n\"""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
