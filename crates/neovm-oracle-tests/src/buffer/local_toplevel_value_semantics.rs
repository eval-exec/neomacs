//! Oracle parity tests for GNU buffer-local toplevel value semantics.
//!
//! GNU implements these in `src/eval.c`: `buffer-local-toplevel-value` reads
//! the buffer-local binding outside any `let` binding and signals
//! `void-variable` if the target buffer has no local value.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_buffer_local_toplevel_value_read_set_and_missing_local_errors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((b1 (get-buffer-create " *bltv-oracle-1*"))
      (b2 (get-buffer-create " *bltv-oracle-2*")))
  (makunbound 'neomacs--oracle-bltv)
  (list
   (condition-case err
       (buffer-local-toplevel-value 'neomacs--oracle-bltv b1)
     (error (cons (car err) (cdr err))))
   (set-buffer-local-toplevel-value 'neomacs--oracle-bltv 11 b1)
   (buffer-local-toplevel-value 'neomacs--oracle-bltv b1)
   (local-variable-p 'neomacs--oracle-bltv b1)
   (local-variable-p 'neomacs--oracle-bltv b2)
   (condition-case err
       (buffer-local-toplevel-value 'neomacs--oracle-bltv b2)
     (error (cons (car err) (cdr err))))))
"#;

    let expect = expect_test::expect![
        r#""OK ((void-variable neomacs--oracle-bltv) nil 11 t nil (void-variable neomacs--oracle-bltv))""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_buffer_local_toplevel_value_ignores_active_let_local_binding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU src/eval.c:local_toplevel_binding finds the saved SPECPDL_LET_LOCAL
    // entry.  set-buffer-local-toplevel-value updates that saved toplevel
    // value while the active let-bound value remains visible until unwind.
    let form = r#"
(let ((b1 (get-buffer-create " *bltv-oracle-let-1*"))
      (b2 (get-buffer-create " *bltv-oracle-let-2*")))
  (unwind-protect
      (progn
        (makunbound 'neomacs--oracle-bltv-let)
        (with-current-buffer b1
          (setq-local neomacs--oracle-bltv-let 'b1-local))
        (with-current-buffer b2
          (setq-local neomacs--oracle-bltv-let 'b2-local))
        (list
         (with-current-buffer b1
           (let ((neomacs--oracle-bltv-let 'let-local))
             (list
              neomacs--oracle-bltv-let
              (buffer-local-toplevel-value 'neomacs--oracle-bltv-let)
              (buffer-local-toplevel-value 'neomacs--oracle-bltv-let b1)
              (set-buffer-local-toplevel-value
               'neomacs--oracle-bltv-let 'b1-top-set)
              neomacs--oracle-bltv-let
              (buffer-local-toplevel-value 'neomacs--oracle-bltv-let)
              (buffer-local-value 'neomacs--oracle-bltv-let b1))))
         (buffer-local-value 'neomacs--oracle-bltv-let b1)
         (with-current-buffer b2
           (let ((neomacs--oracle-bltv-let 'b2-let-local))
             (list
              neomacs--oracle-bltv-let
              (buffer-local-toplevel-value 'neomacs--oracle-bltv-let)
              (set-buffer-local-toplevel-value
               'neomacs--oracle-bltv-let 'b2-top-set b2)
              neomacs--oracle-bltv-let
              (buffer-local-toplevel-value 'neomacs--oracle-bltv-let b2))))
         (buffer-local-value 'neomacs--oracle-bltv-let b2)
         (condition-case err
             (buffer-local-toplevel-value 'neomacs--oracle-bltv-let 42)
           (error (list (car err) (cdr err))))
         (condition-case err
             (set-buffer-local-toplevel-value
              'neomacs--oracle-bltv-let 'bad-buffer 42)
           (error (list (car err) (cdr err)))))))
    (dolist (buf (list b1 b2))
      (when (buffer-live-p buf)
        (kill-buffer buf)))
    (when (boundp 'neomacs--oracle-bltv-let)
      (makunbound 'neomacs--oracle-bltv-let))))
"#;

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 45 47)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
