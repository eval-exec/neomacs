//! Oracle parity tests for buffer-local ↔ default value interactions.
//!
//! GNU src/data.c and src/buffer.c implement intricate interactions between
//! `set`, `setq-default`, `make-local-variable`, `kill-local-variable`,
//! `default-value`, `default-boundp`, and `buffer-local-value`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_err_kind, assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

#[test]
fn oracle_make_local_variable_then_set() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (200 100)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (defvar neovm--test-mlv-defvar 100)
  (make-variable-buffer-local 'neovm--test-mlv-defvar)
  (set 'neovm--test-mlv-defvar 200)
  (list neovm--test-mlv-defvar
        (default-value 'neovm--test-mlv-defvar)))"#,
        expect,
    );
    assert_ok_eq("(200 100)", &oracle, &neovm);
}

#[test]
fn oracle_set_in_buffer_after_make_local() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 50)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (defvar neovm--test-set-local 50)
  (make-variable-buffer-local 'neovm--test-set-local)
  (setq neovm--test-set-local 99)
  (list neovm--test-set-local
        (default-value 'neovm--test-set-local)))"#,
        expect,
    );
    assert_ok_eq("(99 50)", &oracle, &neovm);
}

#[test]
fn oracle_kill_local_restores_default() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (neovm--test-kill-local 10)""#]];
    // GNU: kill-local-variable returns the variable symbol.
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (defvar neovm--test-kill-local 10)
  (make-variable-buffer-local 'neovm--test-kill-local)
  (setq neovm--test-kill-local 20)
  (prog1
      (list (kill-local-variable 'neovm--test-kill-local)
            neovm--test-kill-local)
    (kill-local-variable 'neovm--test-kill-local)))"#,
        expect,
    );
    assert_ok_eq("(neovm--test-kill-local 10)", &oracle, &neovm);
}

#[test]
fn oracle_default_boundp_after_make_local_without_set() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (defvar neovm--test-dbp-local 77)
  (make-variable-buffer-local 'neovm--test-dbp-local)
  (list (default-boundp 'neovm--test-dbp-local)
        (boundp 'neovm--test-dbp-local)))"#,
        expect,
    );
    assert_ok_eq("(t t)", &oracle, &neovm);
}

#[test]
fn oracle_buffer_local_value_returns_default_when_no_local() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 42""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (defvar neovm--test-blv-default 42)
  (make-variable-buffer-local 'neovm--test-blv-default)
  (let ((buf (get-buffer-create "*neovm-test-blv*")))
    (buffer-local-value 'neovm--test-blv-default buf)))"#,
        expect,
    );
    assert_ok_eq("42", &oracle, &neovm);
}

#[test]
fn oracle_buffer_local_value_in_different_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 1)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (defvar neovm--test-blv-diff 1)
  (make-variable-buffer-local 'neovm--test-blv-diff)
  (let ((other-buf (get-buffer-create "neovm-test-blv-diff")))
    (unwind-protect
        (progn
          (set-buffer other-buf)
          (setq neovm--test-blv-diff 99)
          (list (buffer-local-value 'neovm--test-blv-diff other-buf)
                (progn (set-buffer (get-buffer-create "*scratch*"))
                       neovm--test-blv-diff)))
      (kill-buffer other-buf))))"#,
        expect,
    );
    assert_ok_eq("(99 1)", &oracle, &neovm);
}

#[test]
fn oracle_local_variable_if_set_auto_local_alias_and_forwarded_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU src/data.c:Flocal_variable_if_set_p resolves variable aliases and
    // returns t for automatically buffer-local symbols before a local value is
    // present.  It differs from local-variable-p, which only reports an actual
    // local binding already present in the queried buffer.
    let form = r#"
(let ((b1 (get-buffer-create " *neomacs-oracle-lvis-1*"))
      (b2 (get-buffer-create " *neomacs-oracle-lvis-2*")))
  (unwind-protect
      (progn
        (dolist (sym '(neomacs--oracle-lvis-plain
                       neomacs--oracle-lvis-auto
                       neomacs--oracle-lvis-alias))
          (condition-case nil
              (internal-delete-indirect-variable sym)
            (error nil))
          (when (boundp sym)
            (makunbound sym)))
        (setq-default neomacs--oracle-lvis-plain 'plain-default)
        (setq-default neomacs--oracle-lvis-auto 'auto-default)
        (make-variable-buffer-local 'neomacs--oracle-lvis-auto)
        (defvaralias 'neomacs--oracle-lvis-alias
          'neomacs--oracle-lvis-auto)
        (list
         (with-current-buffer b1
           (list
            (local-variable-p 'neomacs--oracle-lvis-plain)
            (local-variable-if-set-p 'neomacs--oracle-lvis-plain)
            (local-variable-p 'neomacs--oracle-lvis-auto)
            (local-variable-if-set-p 'neomacs--oracle-lvis-auto)
            (local-variable-p 'neomacs--oracle-lvis-alias)
            (local-variable-if-set-p 'neomacs--oracle-lvis-alias)))
         (with-current-buffer b1
           (setq neomacs--oracle-lvis-alias 'b1-local)
           (list
            (local-variable-p 'neomacs--oracle-lvis-auto)
            (local-variable-p 'neomacs--oracle-lvis-alias)
            neomacs--oracle-lvis-auto
            (default-value 'neomacs--oracle-lvis-auto)))
         (with-current-buffer b2
           (list
            (local-variable-p 'neomacs--oracle-lvis-auto)
            (local-variable-if-set-p 'neomacs--oracle-lvis-alias)
            neomacs--oracle-lvis-auto))
         (with-current-buffer b1
           (list
            (local-variable-if-set-p 'fill-column)
            (local-variable-p 'fill-column)
            (progn
              (setq fill-column 123)
              (local-variable-p 'fill-column))
            fill-column))
         (condition-case err
             (local-variable-if-set-p 42)
           (error (list (car err) (cdr err))))
         (condition-case err
             (local-variable-if-set-p 'neomacs--oracle-lvis-auto 42)
           (error (list (car err) (cdr err)))))))
    (dolist (buf (list b1 b2))
      (when (buffer-live-p buf)
        (kill-buffer buf)))
    (condition-case nil
        (internal-delete-indirect-variable 'neomacs--oracle-lvis-alias)
      (error nil))
    (dolist (sym '(neomacs--oracle-lvis-plain
                   neomacs--oracle-lvis-auto
                   neomacs--oracle-lvis-alias))
      (when (boundp sym)
        (makunbound sym)))))
"#;
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 64 28)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
