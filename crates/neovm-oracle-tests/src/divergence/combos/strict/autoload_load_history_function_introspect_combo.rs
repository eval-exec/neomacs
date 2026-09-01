//! Strict combo oracle probes, batch 303: autoload + load-history + function
//! introspection deep. autoloadp, autoload-do-load, load-history structure,
//! find-function-search-for-symbol, and subrp / byte-code-function-p shape.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_autoloadp_do_load_introspect() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (autoloadp (symbol-function 'find-file))
      (autoloadp (symbol-function 'car))
      (subrp (symbol-function 'car))
      (subrp (symbol-function 'cons))
      (byte-code-function-p (symbol-function 'car))
      (let ((sfn (symbol-function 'car)))
        (or (subrp sfn) (byte-code-function-p sfn)))
      (commandp 'forward-char)
      (commandp 'car)
      (interactive-p))
"##;
    let expect = expect_test::expect![[r#""OK (nil nil t t nil t t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_load_history_feature_file_lookup() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'loadhist)
(list (consp load-history)
      (> (length load-history) 0)
      (featurep 'subr)
      (stringp (feature-file 'subr))
      (consp (feature-symbols 'subr))
      (assoc (feature-file 'subr) load-history))
"##;
    let expect =
        expect_test::expect![[r#""ERR (error \"subr is not a currently loaded feature\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_function_documentation_arglist_help() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(progn
  (defun probe-ahd (a b &optional c) "Probe with optional." (+ a b (or c 0)))
  (list (documentation 'probe-ahd)
        (documentation 'car)
        (help-function-arglist 'probe-ahd)
        (subr-arity (symbol-function 'car))
        (functionp 'probe-ahd)
        (fboundp 'probe-ahd)
        (help-function-arglist 'car)))
"##;
    let expect = expect_test::expect![[
        r#""OK (\"Probe with optional.\" \"Return the car of LIST.  If LIST is nil, return nil.\\nError if LIST is not nil and not a cons cell.  See also ‘car-safe’.\\n\\nSee Info node ‘(elisp)Cons Cells’ for a discussion of related basic\\nLisp concepts such as car, cdr, cons cell and list.\\n\\n(fn LIST)\" (a b &optional c) (1 . 1) t t (arg1))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
