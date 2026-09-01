//! Oracle parity tests for GNU `substitute-in-file-name` semantics.
//!
//! GNU implements the path discard rules in `src/fileio.c` and delegates
//! environment-variable syntax to `lisp/env.el`.  Undefined variables remain
//! unchanged for this API, `$$` becomes `$`, and embedded absolute file names
//! such as `//...`, `/~`, or a substituted absolute path discard the prefix.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_substitute_in_file_name_env_and_embedded_absolute_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((process-environment process-environment))
  (setenv "NEOMACS_ORACLE_SUBST" "value")
  (setenv "NEOMACS_ORACLE_ABS_SUBST" "/abs/value")
  (setenv "NEOMACS_ORACLE_EMPTY_SUBST" "")
  (setenv "NEOMACS_ORACLE_UNDEF_SUBST" nil)
  (list
   (substitute-in-file-name "$NEOMACS_ORACLE_SUBST/end")
   (substitute-in-file-name "${NEOMACS_ORACLE_SUBST}/end")
   (substitute-in-file-name "$NEOMACS_ORACLE_SUBST_suffix")
   (substitute-in-file-name "${NEOMACS_ORACLE_SUBST}_suffix")
   (substitute-in-file-name "$NEOMACS_ORACLE_EMPTY_SUBST/end")
   (substitute-in-file-name "$NEOMACS_ORACLE_UNDEF_SUBST/end")
   (substitute-in-file-name "$$NEOMACS_ORACLE_SUBST")
   (substitute-in-file-name "$")
   (substitute-in-file-name "$-literal")
   (substitute-in-file-name "${}")
   (substitute-in-file-name "${NEOMACS_ORACLE_SUBST")
   (substitute-in-file-name "prefix//tail")
   (substitute-in-file-name "prefix/~user/tail")
   ;; Absolute results from variable substitution discard the prefix.
   (substitute-in-file-name "prefix/$NEOMACS_ORACLE_ABS_SUBST/tail")
   (condition-case err
       (substitute-in-file-name)
     (error (list (car err) (cdr err))))
   (condition-case err
       (substitute-in-file-name 42)
     (error (list (car err) (cdr err))))))
"#;

    let expect = expect_test::expect![[
        r#""OK (\"value/end\" \"value/end\" \"$NEOMACS_ORACLE_SUBST_suffix\" \"value_suffix\" \"/end\" \"$NEOMACS_ORACLE_UNDEF_SUBST/end\" \"$NEOMACS_ORACLE_SUBST\" \"$\" \"$-literal\" \"${}\" \"${NEOMACS_ORACLE_SUBST\" \"/tail\" \"prefix/~user/tail\" \"/abs/value/tail\" (wrong-number-of-arguments (substitute-in-file-name 0)) (wrong-type-argument (stringp 42)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_substitute_in_file_name_handler_validation_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (defun neomacs--oracle-subst-bad-handler (operation &rest args)
    42)
  (unwind-protect
      (let ((file-name-handler-alist
             '(("\\`/subst-bad:" . neomacs--oracle-subst-bad-handler))))
        (list
         ;; GNU checks the argument type before looking up handlers.
         (condition-case err
             (substitute-in-file-name 42)
           (error (list (car err) (cdr err))))
         ;; GNU requires substitute-in-file-name handlers to return strings.
         (condition-case err
             (substitute-in-file-name "/subst-bad:path")
           (error (list (car err) (cdr err))))
         ;; String handler results are returned directly.
         (let ((file-name-handler-alist
                '(("\\`/subst-good:" . (lambda (&rest _) "handled")))))
           (substitute-in-file-name "/subst-good:path"))))
    (fmakunbound 'neomacs--oracle-subst-bad-handler)))
"#;

    let expect = expect_test::expect![[
        r#""OK ((wrong-type-argument (stringp 42)) (error (\"Invalid handler in ‘file-name-handler-alist’\")) \"handled\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
