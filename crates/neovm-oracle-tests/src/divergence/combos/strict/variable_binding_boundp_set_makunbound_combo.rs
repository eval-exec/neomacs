//! Strict combo oracle probes, batch 199: variable binding/visibility. boundp/
//! fboundp, symbol-value/set, default-boundp/default-value, makunbound +
//! void-variable signal, fmakunbound, and void-function on call.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_variable_boundp_fboundp_symbol_value_set() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(progn
  (defvar probe-vbind 'initial)
  (defun probe-fbind (x) (* x 2))
  (list (boundp 'probe-vbind)
        (fboundp 'probe-fbind)
        (boundp 'probe-missing-var)
        (fboundp 'probe-missing-fn)
        (symbol-value 'probe-vbind)
        (default-boundp 'probe-vbind)
        (progn (set 'probe-vbind 'newval) (symbol-value 'probe-vbind))
        (let ((probe-vbind 'local)) (symbol-value 'probe-vbind))
        (symbol-value 'probe-vbind)))
"##;
    let expect = expect_test::expect![[r#""OK (t t nil nil initial t newval local newval)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_makunbound_fmakunbound_void_signals() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(progn
  (defvar probe-vmu 'val)
  (defun probe-fmu () 'called)
  (list (boundp 'probe-vmu)
        (progn (makunbound 'probe-vmu) (boundp 'probe-vmu))
        (condition-case err (symbol-value 'probe-vmu)
          (void-variable (list (car err) (cadr err))))
        (fboundp 'probe-fmu)
        (progn (fmakunbound 'probe-fmu) (fboundp 'probe-fmu))
        (condition-case err (probe-fmu)
          (void-function (list (car err) (cadr err))))
        (condition-case err (default-value 'probe-vmu)
          (void-variable (list 'default-void (cadr err))))))
"##;
    let expect = expect_test::expect![[
        r#""OK (t nil (void-variable probe-vmu) t nil (void-function probe-fmu) (default-void probe-vmu))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_default_value_local_kill_buffer_local_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((b (generate-new-buffer " *probe-dvc*")))
  (unwind-protect
      (with-current-buffer b
        (defvar probe-dv 'global)
        (make-variable-buffer-local 'probe-dv)
        (list (default-value 'probe-dv)
              probe-dv
              (default-boundp 'probe-dv)
              (progn (setq-local probe-dv 'local) probe-dv)
              (default-value 'probe-dv)
              (local-variable-p 'probe-dv)
              (progn (setq-default probe-dv 'newdefault) (default-value 'probe-dv))
              probe-dv
              (progn (kill-local-variable 'probe-dv) probe-dv)))
    (kill-buffer b)))
"##;
    let expect = expect_test::expect![[
        r#""OK (global global t local global t newdefault local newdefault)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
