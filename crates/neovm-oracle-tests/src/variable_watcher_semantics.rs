//! Oracle parity tests for GNU variable watcher semantics.
//!
//! GNU implements `add-variable-watcher`, `remove-variable-watcher`,
//! `get-variable-watchers`, and watcher notification in `src/data.c`.
//! These tests cover the exact callback operation symbols, pre-write old
//! value visibility, alias forwarding, and buffer-local WHERE reporting.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_variable_watchers_report_set_let_unlet_and_makunbound() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((log nil))
  (defvar neomacs--oracle-vw-basic 'initial)
  (fset 'neomacs--oracle-vw-basic-watch
        (lambda (symbol newval operation where)
          (setq log
                (cons
                 (list symbol
                       (if (boundp symbol) (symbol-value symbol) :unbound)
                       newval operation (bufferp where))
                 log))))
  (add-variable-watcher 'neomacs--oracle-vw-basic 'neomacs--oracle-vw-basic-watch)
  (unwind-protect
      (list
       (get-variable-watchers 'neomacs--oracle-vw-basic)
       (set 'neomacs--oracle-vw-basic 'set-value)
       (let ((neomacs--oracle-vw-basic 'let-value))
         neomacs--oracle-vw-basic)
       (makunbound 'neomacs--oracle-vw-basic)
       (nreverse log)
       (boundp 'neomacs--oracle-vw-basic))
    (remove-variable-watcher 'neomacs--oracle-vw-basic
                             'neomacs--oracle-vw-basic-watch)
    (fmakunbound 'neomacs--oracle-vw-basic-watch)
    (when (boundp 'neomacs--oracle-vw-basic)
      (makunbound 'neomacs--oracle-vw-basic))))
"#;

    let expect = expect_test::expect![[
        r#""OK ((neomacs--oracle-vw-basic-watch) set-value let-value neomacs--oracle-vw-basic ((neomacs--oracle-vw-basic initial set-value set nil) (neomacs--oracle-vw-basic set-value let-value let nil) (neomacs--oracle-vw-basic let-value set-value unlet nil) (neomacs--oracle-vw-basic set-value nil makunbound nil)) nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_variable_watcher_callbacks_do_not_reenter_recursively() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((log nil))
  (defvar neomacs--oracle-vw-reentrant 0)
  (fset 'neomacs--oracle-vw-reentrant-watch
        (lambda (symbol newval operation where)
          (setq log (cons (list operation newval (symbol-value symbol)) log))
          (when (eq newval 1)
            (set symbol 2))))
  (add-variable-watcher 'neomacs--oracle-vw-reentrant
                        'neomacs--oracle-vw-reentrant-watch)
  (unwind-protect
      (list
       (set 'neomacs--oracle-vw-reentrant 1)
       neomacs--oracle-vw-reentrant
       (nreverse log))
    (remove-variable-watcher 'neomacs--oracle-vw-reentrant
                             'neomacs--oracle-vw-reentrant-watch)
    (fmakunbound 'neomacs--oracle-vw-reentrant-watch)
    (makunbound 'neomacs--oracle-vw-reentrant)))
"#;

    let expect = expect_test::expect![[r#""OK (1 1 ((set 1 0)))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_variable_watchers_follow_defvaralias_base_variable() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((log nil))
  (defvar neomacs--oracle-vw-base 'base)
  (defvaralias 'neomacs--oracle-vw-alias 'neomacs--oracle-vw-base)
  (fset 'neomacs--oracle-vw-alias-watch
        (lambda (symbol newval operation where)
          (setq log (cons (list symbol newval operation) log))))
  (add-variable-watcher 'neomacs--oracle-vw-alias 'neomacs--oracle-vw-alias-watch)
  (unwind-protect
      (list
       (get-variable-watchers 'neomacs--oracle-vw-base)
       (get-variable-watchers 'neomacs--oracle-vw-alias)
       (set 'neomacs--oracle-vw-alias 'via-alias)
       neomacs--oracle-vw-base
       (set 'neomacs--oracle-vw-base 'via-base)
       neomacs--oracle-vw-alias
       (nreverse log))
    (remove-variable-watcher 'neomacs--oracle-vw-base
                             'neomacs--oracle-vw-alias-watch)
    (fmakunbound 'neomacs--oracle-vw-alias-watch)
    (unintern 'neomacs--oracle-vw-alias nil)
    (unintern 'neomacs--oracle-vw-base nil)))
"#;

    let expect = expect_test::expect![[
        r#""OK ((neomacs--oracle-vw-alias-watch) (neomacs--oracle-vw-alias-watch) via-alias via-alias via-base via-base ((neomacs--oracle-vw-base via-alias set) (neomacs--oracle-vw-base via-base set)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_defvaralias_watcher_runs_before_alias_install_and_doc_put() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((log nil))
  (defvar neomacs--oracle-vw-order-base 'base-value)
  (defvar neomacs--oracle-vw-order-alias 'alias-value)
  (fset 'neomacs--oracle-vw-order-watch
        (lambda (symbol newval operation where)
          (setq log
                (cons
                 (list symbol
                       newval
                       operation
                       (eq (indirect-variable symbol)
                           'neomacs--oracle-vw-order-base)
                       (symbol-value symbol)
                       (documentation-property
                        symbol 'variable-documentation t)
                       where)
                 log))))
  (add-variable-watcher 'neomacs--oracle-vw-order-alias
                        'neomacs--oracle-vw-order-watch)
  (unwind-protect
      (list
       (defvaralias 'neomacs--oracle-vw-order-alias
         'neomacs--oracle-vw-order-base
         "Alias doc installed after watcher.")
       (symbol-value 'neomacs--oracle-vw-order-alias)
       (documentation-property 'neomacs--oracle-vw-order-alias
                               'variable-documentation t)
       (nreverse log))
    (condition-case nil
        (remove-variable-watcher 'neomacs--oracle-vw-order-base
                                 'neomacs--oracle-vw-order-watch)
      (error nil))
    (condition-case nil
        (remove-variable-watcher 'neomacs--oracle-vw-order-alias
                                 'neomacs--oracle-vw-order-watch)
      (error nil))
    (fmakunbound 'neomacs--oracle-vw-order-watch)
    (condition-case nil
        (internal-delete-indirect-variable 'neomacs--oracle-vw-order-alias)
      (error nil))
    (dolist (sym '(neomacs--oracle-vw-order-base
                   neomacs--oracle-vw-order-alias))
      (when (boundp sym)
        (makunbound sym)))))
"#;

    let expect = expect_test::expect![[
        r#""OK (neomacs--oracle-vw-order-base base-value \"Alias doc installed after watcher.\" ((neomacs--oracle-vw-order-alias neomacs--oracle-vw-order-base defvaralias nil alias-value nil nil)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_variable_watchers_report_buffer_local_where() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((log nil)
      (buf (generate-new-buffer " *neomacs-oracle-vw-buffer*")))
  (defvar neomacs--oracle-vw-local 'default)
  (fset 'neomacs--oracle-vw-local-watch
        (lambda (symbol newval operation where)
          (setq log
                (cons
                 (list newval operation
                       (and (bufferp where) (buffer-name where))
                       (if (boundp symbol) (symbol-value symbol) :unbound))
                 log))))
  (add-variable-watcher 'neomacs--oracle-vw-local 'neomacs--oracle-vw-local-watch)
  (unwind-protect
      (list
       (set-default 'neomacs--oracle-vw-local 'default-set)
       (with-current-buffer buf
         (make-local-variable 'neomacs--oracle-vw-local)
         (setq neomacs--oracle-vw-local 'local-set)
         neomacs--oracle-vw-local)
       (default-value 'neomacs--oracle-vw-local)
       (nreverse log))
    (remove-variable-watcher 'neomacs--oracle-vw-local
                             'neomacs--oracle-vw-local-watch)
    (fmakunbound 'neomacs--oracle-vw-local-watch)
    (makunbound 'neomacs--oracle-vw-local)
    (when (buffer-live-p buf)
      (kill-buffer buf))))
"#;

    let expect = expect_test::expect![[
        r#""OK (default-set local-set default-set ((default-set set nil default) (local-set set \" *neomacs-oracle-vw-buffer*\" default-set)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_variable_watchers_set_default_reports_set_operation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((log nil))
  (defvar neomacs--oracle-vw-default-base 'base)
  (defvaralias 'neomacs--oracle-vw-default-alias
    'neomacs--oracle-vw-default-base)
  (fset 'neomacs--oracle-vw-default-watch
        (lambda (symbol newval operation where)
          (setq log
                (cons
                 (list symbol
                       (if (boundp symbol) (symbol-value symbol) :unbound)
                       newval operation where)
                 log))))
  (add-variable-watcher 'neomacs--oracle-vw-default-base
                        'neomacs--oracle-vw-default-watch)
  (unwind-protect
      (list
       (set-default 'neomacs--oracle-vw-default-base 'via-base)
       (set-default 'neomacs--oracle-vw-default-alias 'via-alias)
       (default-value 'neomacs--oracle-vw-default-base)
       (default-value 'neomacs--oracle-vw-default-alias)
       (nreverse log))
    (condition-case nil
        (remove-variable-watcher 'neomacs--oracle-vw-default-base
                                 'neomacs--oracle-vw-default-watch)
      (error nil))
    (fmakunbound 'neomacs--oracle-vw-default-watch)
    (condition-case nil
        (internal-delete-indirect-variable
         'neomacs--oracle-vw-default-alias)
      (error nil))
    (dolist (sym '(neomacs--oracle-vw-default-base
                   neomacs--oracle-vw-default-alias))
      (when (boundp sym)
        (makunbound sym)))))
"#;

    let expect = expect_test::expect![[
        r#""OK (via-base via-alias via-alias via-alias ((neomacs--oracle-vw-default-base base via-base set nil) (neomacs--oracle-vw-default-base via-base via-alias set nil) (neomacs--oracle-vw-default-base via-base via-alias set nil)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_variable_watchers_kill_local_variable_notifies_before_local_removal() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((log nil)
      (buf (generate-new-buffer " *neomacs-oracle-vw-kill-local*")))
  (defvar neomacs--oracle-vw-kill-local 'default)
  (fset 'neomacs--oracle-vw-kill-local-watch
        (lambda (symbol newval operation where)
          (setq log
                (cons
                 (list symbol newval operation
                       (and (bufferp where) (buffer-name where))
                       (if (boundp symbol) (symbol-value symbol) :unbound))
                 log))))
  (add-variable-watcher 'neomacs--oracle-vw-kill-local
                        'neomacs--oracle-vw-kill-local-watch)
  (unwind-protect
      (list
       (with-current-buffer buf
         (list
          (kill-local-variable 'neomacs--oracle-vw-kill-local)
          (make-variable-buffer-local 'neomacs--oracle-vw-kill-local)
          (kill-local-variable 'neomacs--oracle-vw-kill-local)
          (make-local-variable 'neomacs--oracle-vw-kill-local)
          (setq neomacs--oracle-vw-kill-local 'local)
          (kill-local-variable 'neomacs--oracle-vw-kill-local)
          neomacs--oracle-vw-kill-local))
       (nreverse log))
    (remove-variable-watcher 'neomacs--oracle-vw-kill-local
                             'neomacs--oracle-vw-kill-local-watch)
    (fmakunbound 'neomacs--oracle-vw-kill-local-watch)
    (when (boundp 'neomacs--oracle-vw-kill-local)
      (makunbound 'neomacs--oracle-vw-kill-local))
    (when (buffer-live-p buf)
      (kill-buffer buf))))
"#;

    let expect = expect_test::expect![[
        r#""OK ((neomacs--oracle-vw-kill-local neomacs--oracle-vw-kill-local neomacs--oracle-vw-kill-local neomacs--oracle-vw-kill-local local neomacs--oracle-vw-kill-local default) ((neomacs--oracle-vw-kill-local nil makunbound \" *neomacs-oracle-vw-kill-local*\" default) (neomacs--oracle-vw-kill-local local set \" *neomacs-oracle-vw-kill-local*\" default) (neomacs--oracle-vw-kill-local nil makunbound \" *neomacs-oracle-vw-kill-local*\" local)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
