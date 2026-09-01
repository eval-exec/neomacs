//! Oracle parity tests for GNU `subr.el` readability and function aliases.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

#[test]
fn oracle_prop_gnu_subr_function_alias_p_and_readablep_contracts() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU subr.el:function-alias-p walks symbol-function links until a
    // non-symbol definition, and readablep returns either prin1 syntax or nil
    // through print-unreadable-function.
    let form = r#"(unwind-protect
    (progn
      (defalias 'neovm-function-alias-base (lambda (x) x))
      (defalias 'neovm-function-alias-a 'neovm-function-alias-base)
      (defalias 'neovm-function-alias-b 'neovm-function-alias-a)
      (defalias 'neovm-function-alias-subr '+)
      (with-temp-buffer
        (list
         (function-alias-p 'neovm-function-alias-base)
         (function-alias-p 'neovm-function-alias-a)
         (function-alias-p 'neovm-function-alias-b)
         (function-alias-p 'neovm-function-alias-subr)
         (function-alias-p (lambda (x) x))
         (function-alias-p 'neovm-function-alias-missing)
         (mapcar (lambda (x)
                   (let ((r (readablep x)))
                     (list (type-of x) (if r t nil) r)))
                 (list nil
                       t
                       42
                       "str"
                       'sym
                       [a b]
                       (make-symbol "uninterned")
                       (current-buffer)
                       (point-marker)
                       (make-hash-table)))))))
  (mapc #'fmakunbound
        '(neovm-function-alias-base
          neovm-function-alias-a
          neovm-function-alias-b
          neovm-function-alias-subr)))"#;
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 32 38)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_define_obsolete_function_alias_metadata_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU lisp/emacs-lisp/byte-run.el defines this macro as defalias plus
    // make-obsolete.  make-obsolete stores `(CURRENT nil WHEN)` in the
    // `byte-obsolete-info` property and rejects nil/t through a regular error,
    // while the defalias side still rejects nil as a function name constant.
    let form = r#"
(let ((old 'neomacs--oracle-obsolete-old)
      (new 'neomacs--oracle-obsolete-new))
  (dolist (sym (list old new))
    (ignore-errors (fmakunbound sym))
    (setplist sym nil))
  (unwind-protect
      (progn
        (fset new (lambda (x) (+ x 7)))
        (list
         (fboundp 'define-obsolete-function-alias)
         (fboundp 'make-obsolete)
         (macroexpand
          '(define-obsolete-function-alias
             'neomacs--oracle-obsolete-old
             'neomacs--oracle-obsolete-new
             "1.2"
             "Old doc."))
         (define-obsolete-function-alias old new "1.2" "Old doc.")
         (fboundp old)
         (symbol-function old)
         (function-alias-p old)
         (funcall old 5)
         (documentation old)
         (get old 'byte-obsolete-info)
         (make-obsolete old "use msg." "2.0")
         (get old 'byte-obsolete-info)
         (condition-case err
             (make-obsolete nil 'ignore "1")
           (error (list (car err) (cdr err))))
         (condition-case err
             (define-obsolete-function-alias nil new "3")
           (error (list (car err) (cdr err)))))))
    (dolist (sym (list old new))
      (ignore-errors (fmakunbound sym))
      (setplist sym nil))))
"#;

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 36 27)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
