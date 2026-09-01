//! Oracle parity tests for buffer-local symbol identity.
//!
//! GNU's buffer-local machinery uses symbol identity after variable-alias
//! resolution (`XSYMBOL`/`XSETSYMBOL` in `src/data.c` and `src/buffer.c`).
//! Uninterned symbols with the same print name as interned symbols must not
//! share buffer-local bindings.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_make_local_variable_keeps_uninterned_symbol_identity() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((uninterned (make-symbol "neomacs--oracle-local-id"))
       (interned (intern "neomacs--oracle-local-id"))
       (buf (current-buffer)))
  (unwind-protect
      (progn
        (set uninterned 1)
        (set interned 2)
        (make-local-variable uninterned)
        (set uninterned 3)
        (set interned 4)
        (list
         (eq uninterned interned)
         (boundp uninterned)
         (boundp interned)
         (local-variable-p uninterned)
         (local-variable-p interned)
         (buffer-local-value uninterned buf)
         (buffer-local-value interned buf)
         (symbol-value uninterned)
         (symbol-value interned)))
    (makunbound uninterned)
    (makunbound interned)))
"#;

    let expect = expect_test::expect![r#""OK (nil t t t nil 3 4 3 4)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_buffer_local_value_uses_uninterned_symbol_identity() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((uninterned (make-symbol "neomacs--oracle-blv-id"))
       (interned (intern "neomacs--oracle-blv-id"))
       (buf-a (current-buffer))
       (buf-b (get-buffer-create " *neomacs oracle blv id*")))
  (unwind-protect
      (progn
        (set uninterned 'uninterned-default)
        (set interned 'interned-default)
        (make-local-variable uninterned)
        (set uninterned 'uninterned-local-a)
        (with-current-buffer buf-b
          (make-local-variable interned)
          (set interned 'interned-local-b))
        (list
         (buffer-local-value uninterned buf-a)
         (buffer-local-value interned buf-a)
         (buffer-local-value uninterned buf-b)
         (buffer-local-value interned buf-b)
         (with-current-buffer buf-a
           (list (local-variable-p uninterned)
                 (local-variable-p interned)))
         (with-current-buffer buf-b
           (list (local-variable-p uninterned)
                 (local-variable-p interned)))))
    (when (buffer-live-p buf-b)
      (kill-buffer buf-b))
    (makunbound uninterned)
    (makunbound interned)))
"#;

    let expect = expect_test::expect![
        r#""OK (uninterned-local-a interned-default uninterned-default interned-local-b (t nil) (nil t))""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_make_variable_buffer_local_keeps_uninterned_symbol_identity() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((uninterned (make-symbol "neomacs--oracle-auto-local-id"))
       (interned (intern "neomacs--oracle-auto-local-id"))
       (buf-a (current-buffer))
       (buf-b (get-buffer-create " *neomacs oracle auto local id*")))
  (unwind-protect
      (progn
        (set uninterned 'uninterned-default)
        (set interned 'interned-default)
        (make-variable-buffer-local uninterned)
        (with-current-buffer buf-b
          (set uninterned 'uninterned-local-b)
          (set interned 'interned-global-from-b))
        (list
         (default-value uninterned)
         (default-value interned)
         (buffer-local-value uninterned buf-a)
         (buffer-local-value uninterned buf-b)
         (buffer-local-value interned buf-a)
         (buffer-local-value interned buf-b)
         (with-current-buffer buf-a
           (list (local-variable-p uninterned)
                 (local-variable-p interned)))
         (with-current-buffer buf-b
           (list (local-variable-p uninterned)
                 (local-variable-p interned)))))
    (when (buffer-live-p buf-b)
      (kill-buffer buf-b))
    (makunbound uninterned)
    (makunbound interned)))
"#;

    let expect = expect_test::expect![
        r#""OK (uninterned-default interned-global-from-b uninterned-default uninterned-local-b interned-global-from-b interned-global-from-b (nil nil) (t nil))""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}
