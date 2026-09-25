//! Oracle parity tests for eager macro-expansion failures during `load` and
//! `eval-buffer` (P4.1 S1.0, tests T1.1-T1.2).
//!
//! GNU `readevalloop_eager_expand_eval` (src/lread.c:2135-2152) has no
//! fallback: `internal-macroexpand-for-load` (lisp/emacs-lisp/macroexp.el:
//! 915-948) turns an expander error into `Eager macro-expansion failure` and a
//! detected cycle into `Eager macro-expansion skipped due to cycle`, and the
//! error aborts the load at the failing form.  A `throw` out of an expander
//! is not an error and reaches the enclosing `catch`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

/// T1.1: a macro signals while a `defun` body is eagerly expanded, in a
/// loaded file and in `eval-buffer` of a plain buffer.
#[test]
fn oracle_eager_expansion_failure_aborts_load_and_eval_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r##"
(let* ((dir (make-temp-file "neo-eager-" t))
       (file (expand-file-name "bad-expand.el" dir)))
  (unwind-protect
      (progn
        (with-temp-file file
          (insert ";;; -*- lexical-binding: t; -*-\n"
                  "(defmacro bad-m (x) (if (eq x 'boom) (error \"boom in expander\") `(list ,x)))\n"
                  "(defvar bad-ran nil)\n"
                  "(defun bad-f () (bad-m boom))\n"
                  "(setq bad-ran t)\n"))
        (list (condition-case err (progn (load file nil t) 'loaded) (error err))
              (and (boundp 'bad-ran) bad-ran)
              (fboundp 'bad-f)
              (with-temp-buffer
                (insert "(defmacro bad-m2 (x) (if (eq x 'boom) (error \"boom2\") `(list ,x)))\n"
                        "(setq bad-ran2 'before)\n"
                        "(defun bad-f2 () (bad-m2 boom))\n"
                        "(setq bad-ran2 t)\n")
                (list (condition-case err (progn (eval-buffer) 'evaluated) (error err))
                      (and (boundp 'bad-ran2) bad-ran2)
                      (fboundp 'bad-f2)))))
    (delete-directory dir t)))
"##;

    let expect = expect_test::expect![[
        r#""OK ((error \"Eager macro-expansion failure: (error \\\"boom in expander\\\")\") nil nil ((error \"Eager macro-expansion failure: (error \\\"boom2\\\")\") before nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

/// T1.2: the expander loads the file being expanded; the nested load detects
/// the cycle and the outer expansion wraps the cycle error.
#[test]
fn oracle_eager_expansion_cycle_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r##"
(let* ((dir (make-temp-file "neo-eager-cycle-" t))
       (file (expand-file-name "cyc-a.el" dir)))
  (unwind-protect
      (progn
        (with-temp-file file
          (insert ";;; -*- lexical-binding: t; -*-\n"
                  "(defvar cyc-loads 0)\n"
                  "(setq cyc-loads (1+ cyc-loads))\n"
                  "(defmacro cyc-m () (load load-file-name nil t) 1)\n"
                  "(defvar cyc-x (cyc-m))\n"
                  "(setq cyc-done t)\n"))
        (defvar cyc-loads)
        (defvar cyc-done)
        (list (condition-case err (progn (load file nil t) 'loaded)
                (error (list (car err)
                             (replace-regexp-in-string (regexp-quote dir) "DIR" (cadr err)))))
              (and (boundp 'cyc-loads) cyc-loads)
              (and (boundp 'cyc-done) cyc-done)
              (and (boundp 'cyc-x) cyc-x)
              macroexp--pending-eager-loads))
    (delete-directory dir t)))
"##;

    let expect = expect_test::expect![[
        r#""OK ((error \"Eager macro-expansion failure: (error \\\"Eager macro-expansion skipped due to cycle:\\n  … => (load \\\\\\\"cyc-a.el\\\\\\\") => (macroexpand-all (defvar cyc-x …)) => (load \\\\\\\"cyc-a.el\\\\\\\")\\\")\") 1 nil nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

/// A `throw` out of an eager expander reaches the enclosing `catch`; the
/// rest of the file never runs.
#[test]
fn oracle_eager_expansion_throw_reaches_enclosing_catch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r##"
(let* ((dir (make-temp-file "neo-eager-throw-" t))
       (file (expand-file-name "thr.el" dir)))
  (unwind-protect
      (progn
        (with-temp-file file
          (insert ";;; -*- lexical-binding: t; -*-\n"
                  "(defmacro thr-m () (throw 'neo-eager-tag 'thrown))\n"
                  "(defun thr-f () (thr-m))\n"
                  "(setq thr-after t)\n"))
        (defvar thr-after)
        (list (catch 'neo-eager-tag (load file nil t) 'loaded)
              (and (boundp 'thr-after) thr-after)
              (fboundp 'thr-f)))
    (delete-directory dir t)))
"##;

    let expect = expect_test::expect![[r#""OK (thrown nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
