//! Strict combo oracle probes, batch 63: more loaded-library coverage —
//! checkdoc (elisp docstring/style checker), custom (defgroup/defcustom
//! introspection), eldoc (argstring), and find-func (definition location).
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity_with_load;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_n3_checkdoc_clean_defun() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert "(defun probe-fn (arg1 arg2)\n  \"Doc string.\"\n  (+ arg1 arg2))\n")
  (goto-char (point-min))
  (list (checkdoc-current-buffer t)
        (checkdoc-rogue-spaces 1 20)))
"##,
        &["emacs-lisp/checkdoc.el"],
        expect,
    );
}

#[test]
fn div_n3_custom_introspection() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (wrong-type-argument symbolp \"probe-cust-var\")""#]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(progn
  (defgroup probe-cust nil "Probe group" :group 'test)
  (defcustom probe-cust-var 42 "Doc" :type 'integer)
  (list (custom-variable-p 'probe-cust-var)
        (default-value 'probe-cust-var)
        (car (get 'probe-cust-var 'standard-value))
        (get 'probe-cust-var 'custom-type)
        (custom-unlispify-tag-name "probe-cust-var")))
"##,
        &["cus-edit.el"],
        expect,
    );
}

#[test]
fn div_n3_eldoc_argstring() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function eldoc-function-argstring)""#]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(progn
  (defun probe-eldoc-fn (a b &optional c) "Doc." (+ a b))
  (list (eldoc-function-argstring 'probe-eldoc-fn)
        (help-function-arglist 'probe-eldoc-fn)
        (help-function-arglist 'car)))
"##,
        &["emacs-lisp/eldoc.el"],
        expect,
    );
}

#[test]
fn div_n3_find_func_search() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (error \"Don’t know where ‘car’ is defined\")""#]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(let ((buf (find-function-search-for-symbol 'car nil nil)))
  (list (bufferp (car buf))
        (consp buf)))
"##,
        &["emacs-lisp/find-func.el"],
        expect,
    );
}

#[test]
fn div_n3_help_split_string_and_key() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (nil #(\"C-c C-d\" 0 7 (font-lock-face help-key-binding face help-key-binding)) t)""#
    ]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(list (help-split-fundoc "DOC BODY.\n(fn ARG1 ARG2)" nil)
      (help--key-description-fontified (kbd "C-c C-d"))
      (subrp (symbol-function 'car)))
"##,
        &["help-fns.el"],
        expect,
    );
}
