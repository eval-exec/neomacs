//! Oracle parity tests for GNU buffer-local state save/restore helpers.
//!
//! GNU implements `buffer-local-set-state`, `buffer-local-set-state--get`,
//! and `buffer-local-restore-state` in `lisp/subr.el`.  These helpers are
//! small, but they combine macro expansion, buffer-local bindings, void
//! variables, and restoration via `kill-local-variable`.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

#[test]
fn oracle_prop_gnu_buffer_local_boundp_uses_buffer_local_value_contract() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU subr.el:buffer-local-boundp is a thin condition-case around
    // buffer-local-value.  It is true for a default binding even without a
    // local binding, false for void globals, and follows the queried buffer.
    let form = r#"
(progn
  (defvar neomacs--oracle-blbp-global 'global-value)
  (defvar neomacs--oracle-blbp-local 'default-local)
  (makunbound 'neomacs--oracle-blbp-void)
  (let ((buf-a (get-buffer-create " *oracle-blbp-a*"))
        (buf-b (get-buffer-create " *oracle-blbp-b*")))
    (unwind-protect
        (progn
          (with-current-buffer buf-a
            (set (make-local-variable 'neomacs--oracle-blbp-local) 'a-local)
            (set (make-local-variable 'neomacs--oracle-blbp-void) 'a-void-local))
          (list
           (buffer-local-boundp 'neomacs--oracle-blbp-global buf-a)
           (buffer-local-boundp 'neomacs--oracle-blbp-global buf-b)
           (buffer-local-boundp 'neomacs--oracle-blbp-local buf-a)
           (buffer-local-boundp 'neomacs--oracle-blbp-local buf-b)
           (buffer-local-boundp 'neomacs--oracle-blbp-void buf-a)
           (buffer-local-boundp 'neomacs--oracle-blbp-void buf-b)
           (condition-case err
               (buffer-local-boundp 'neomacs--oracle-blbp-global (kill-buffer buf-b))
             (error (list (car err) (cadr err))))))
      (when (buffer-live-p buf-a) (kill-buffer buf-a))
      (when (buffer-live-p buf-b) (kill-buffer buf-b)))))
"#;

    let expect = expect_test::expect![r#""OK (t t t t t nil (wrong-type-argument bufferp))""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_gnu_buffer_local_set_state_restores_local_global_and_void_vars() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (defvar neomacs--oracle-blss-a 'global-a)
  (defvar neomacs--oracle-blss-b 'global-b)
  (makunbound 'neomacs--oracle-blss-c)
  (with-temp-buffer
    (set (make-local-variable 'neomacs--oracle-blss-a) 'local-a)
    (let ((state (buffer-local-set-state
                  neomacs--oracle-blss-a 'new-a
                  neomacs--oracle-blss-b 'new-b
                  neomacs--oracle-blss-c 'new-c)))
      (let ((during (list state
                          neomacs--oracle-blss-a
                          neomacs--oracle-blss-b
                          neomacs--oracle-blss-c
                          (local-variable-p 'neomacs--oracle-blss-a)
                          (local-variable-p 'neomacs--oracle-blss-b)
                          (local-variable-p 'neomacs--oracle-blss-c))))
        (buffer-local-restore-state state)
        (list
         during
         (list neomacs--oracle-blss-a
               neomacs--oracle-blss-b
               (boundp 'neomacs--oracle-blss-c)
               (local-variable-p 'neomacs--oracle-blss-a)
               (local-variable-p 'neomacs--oracle-blss-b)
               (local-variable-p 'neomacs--oracle-blss-c)))))))
"#;

    let expect = expect_test::expect![
        r#""OK ((((neomacs--oracle-blss-a t local-a) (neomacs--oracle-blss-b nil global-b) (neomacs--oracle-blss-c nil nil)) new-a new-b new-c t t t) (local-a global-b nil t nil nil))""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_gnu_buffer_local_set_state_get_records_current_buffer_only() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (defvar neomacs--oracle-blss-current 'global-current)
  (defvar neomacs--oracle-blss-other 'global-other)
  (let ((other (get-buffer-create " *oracle-blss-other*")))
    (unwind-protect
        (progn
          (with-current-buffer other
            (set (make-local-variable 'neomacs--oracle-blss-current) 'other-local)
            (set (make-local-variable 'neomacs--oracle-blss-other) 'other-local))
          (with-temp-buffer
            (set (make-local-variable 'neomacs--oracle-blss-current) 'current-local)
            (list
             (buffer-local-set-state--get
              '(neomacs--oracle-blss-current neomacs--oracle-blss-other))
             (buffer-local-value 'neomacs--oracle-blss-current other)
             (buffer-local-value 'neomacs--oracle-blss-other other))))
      (kill-buffer other))))
"#;

    let expect = expect_test::expect![
        r#""OK (((neomacs--oracle-blss-current t current-local) (neomacs--oracle-blss-other nil global-other)) other-local other-local)""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_gnu_buffer_local_set_state_rejects_odd_pairs_at_macroexpand() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(condition-case err
    (macroexpand '(buffer-local-set-state neomacs--oracle-blss-x 1 neomacs--oracle-blss-y))
  (error err))
"#;

    let expect =
        expect_test::expect![r#""OK (wrong-number-of-arguments buffer-local-set-state 3)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}
