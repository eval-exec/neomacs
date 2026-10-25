//! GNU-exact local restoration during a backtrace-eval exchange, in Sim and Green.
//!
//! Green is admitted before each probe so bindings use owner records. Each
//! probe runs without a yield inside the exchange: its saved local restoration
//! must agree with GNU in both modes. A swap that encounters no local permanently
//! retires the entry; recreating the local before that swap keeps it restorable.
//! Generate expectations with NEOVM_ORACLE_MODE=refresh UPDATE_EXPECT=1,
//! using GNU 31.1 and the mandatory PID sandbox.

use crate::common;

use common::return_if_neovm_enable_oracle_proptest_not_set;

const SIM: &[(&str, &str)] = &[("NEOVM_THREADS", "sim"), ("NEOVM_JIT", "0")];
const GREEN: &[(&str, &str)] = &[("NEOVM_THREADS", "green")];

/// An exchanged binding alone does not overwrite the replacement local on pop.
#[test]
fn backtrace_local_simple_remake_in_both_modes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_under_envs_expect(
        r##"(progn
  (thread-join (make-thread (lambda () nil)))
  (defvar g3e-local 'default)
  (fset 'g3e-local-probe
        (lambda ()
          (let ((g3e-local 'bound))
            (list g3e-local
                  (backtrace-eval
                   '(list g3e-local (progn (kill-local-variable 'g3e-local) g3e-local) (progn (make-local-variable 'g3e-local) (setq g3e-local 'remade) g3e-local))
                   0 'g3e-local-probe)
                  g3e-local))))
  (with-temp-buffer
    (setq-local g3e-local 'local)
    (list (g3e-local-probe) g3e-local (local-variable-p 'g3e-local))))"##,
        &[SIM, GREEN],
        expect_test::expect![[r#""OK ((bound (local default remade) bound) remade t)""#]],
    );
}

/// The new binding saves the original local even while the older binding is suspended.
#[test]
fn backtrace_local_inner_remake_in_both_modes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_under_envs_expect(
        r##"(progn
  (thread-join (make-thread (lambda () nil)))
  (defvar g3e-local 'default)
  (fset 'g3e-local-probe
        (lambda ()
          (let ((g3e-local 'bound))
            (list g3e-local
                  (backtrace-eval
                   '(let ((g3e-local 'exchange-bound)) (list g3e-local (progn (kill-local-variable 'g3e-local) g3e-local) (progn (make-local-variable 'g3e-local) (setq g3e-local 'remade) g3e-local)))
                   0 'g3e-local-probe)
                  g3e-local))))
  (with-temp-buffer
    (setq-local g3e-local 'local)
    (list (g3e-local-probe) g3e-local (local-variable-p 'g3e-local))))"##,
        &[SIM, GREEN],
        expect_test::expect![[r#""OK ((bound (exchange-bound default remade) bound) local t)""#]],
    );
}

/// Nested new bindings restore their predecessor and then the original local.
#[test]
fn backtrace_local_double_inner_remake_in_both_modes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_under_envs_expect(
        r##"(progn
  (thread-join (make-thread (lambda () nil)))
  (defvar g3e-local 'default)
  (fset 'g3e-local-probe
        (lambda ()
          (let ((g3e-local 'bound))
            (list g3e-local
                  (backtrace-eval
                   '(let ((g3e-local 'exchange-outer)) (list (let ((g3e-local 'exchange-inner)) (list g3e-local (progn (kill-local-variable 'g3e-local) g3e-local) (progn (make-local-variable 'g3e-local) (setq g3e-local 'remade) g3e-local))) g3e-local))
                   0 'g3e-local-probe)
                  g3e-local))))
  (with-temp-buffer
    (setq-local g3e-local 'local)
    (list (g3e-local-probe) g3e-local (local-variable-p 'g3e-local))))"##,
        &[SIM, GREEN],
        expect_test::expect![[
            r#""OK ((bound ((exchange-inner default remade) exchange-outer) bound) local t)""#
        ]],
    );
}

/// A missing local prevents both the new binding restore and the older rewind.
#[test]
fn backtrace_local_inner_kill_no_remake_in_both_modes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_under_envs_expect(
        r##"(progn
  (thread-join (make-thread (lambda () nil)))
  (defvar g3e-local 'default)
  (fset 'g3e-local-probe
        (lambda ()
          (let ((g3e-local 'bound))
            (list g3e-local
                  (backtrace-eval
                   '(let ((g3e-local 'exchange-bound)) (list g3e-local (progn (kill-local-variable 'g3e-local) g3e-local)))
                   0 'g3e-local-probe)
                  g3e-local))))
  (with-temp-buffer
    (setq-local g3e-local 'local)
    (list (g3e-local-probe) g3e-local (local-variable-p 'g3e-local))))"##,
        &[SIM, GREEN],
        expect_test::expect![[r#""OK ((bound (exchange-bound default) default) default nil)""#]],
    );
}

/// An exchange while the local is absent permanently retires the entries, even after remake.
#[test]
fn backtrace_local_swap_absent_then_remake_in_both_modes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_under_envs_expect(
        r##"(progn
  (thread-join (make-thread (lambda () nil)))
  (defvar g3e-local 'default)
  (fset 'g3e-local-probe
        (lambda ()
          (let ((g3e-local 'bound))
            (list g3e-local
                  (backtrace-eval
                   '(let ((g3e-local 'exchange-bound)) (list g3e-local (progn (kill-local-variable 'g3e-local) (backtrace-eval 'g3e-local 0 'g3e-local-probe)) (progn (make-local-variable 'g3e-local) (setq g3e-local 'remade) g3e-local)))
                   0 'g3e-local-probe)
                  g3e-local))))
  (with-temp-buffer
    (setq-local g3e-local 'local)
    (list (g3e-local-probe) g3e-local (local-variable-p 'g3e-local))))"##,
        &[SIM, GREEN],
        expect_test::expect![[r#""OK ((bound (exchange-bound default remade) remade) remade t)""#]],
    );
}

/// Local-if-set recreation through setq reattaches the same saved local as explicit make-local-variable.
#[test]
fn backtrace_local_auto_remake_in_both_modes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_under_envs_expect(
        r##"(progn
  (thread-join (make-thread (lambda () nil)))
  (defvar g3e-local 'default) (make-variable-buffer-local 'g3e-local)
  (fset 'g3e-local-probe
        (lambda ()
          (let ((g3e-local 'bound))
            (list g3e-local
                  (backtrace-eval
                   '(let ((g3e-local 'exchange-bound)) (list g3e-local (progn (kill-local-variable 'g3e-local) g3e-local) (progn (setq g3e-local 'remade) g3e-local)))
                   0 'g3e-local-probe)
                  g3e-local))))
  (with-temp-buffer
    (setq-local g3e-local 'local)
    (list (g3e-local-probe) g3e-local (local-variable-p 'g3e-local))))"##,
        &[SIM, GREEN],
        expect_test::expect![[r#""OK ((bound (exchange-bound default remade) bound) local t)""#]],
    );
}

/// First admission occurs while the local is absent. The join then performs
/// a real missing-local thread swap, permanently retiring the saved entries.
#[test]
fn backtrace_local_late_admission_join_simple_remake() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_under_envs_expect(
        r##"(progn
  (defvar g3e-local 'default)
  (fset 'g3e-local-probe
        (lambda ()
          (let ((g3e-local 'bound))
            (list g3e-local
                  (backtrace-eval
                   '(list g3e-local (progn (kill-local-variable 'g3e-local) (thread-join (make-thread (lambda () nil))) g3e-local) (progn (make-local-variable 'g3e-local) (setq g3e-local 'remade) g3e-local))
                   0 'g3e-local-probe)
                  g3e-local))))
  (with-temp-buffer
    (setq-local g3e-local 'local)
    (list (g3e-local-probe) g3e-local (local-variable-p 'g3e-local))))"##,
        &[GREEN],
        expect_test::expect![[r#""OK ((bound (local default remade) remade) remade t)""#]],
    );
}

/// First admission occurs while the local is absent. The join then performs
/// a real missing-local thread swap, permanently retiring the saved entries.
#[test]
fn backtrace_local_late_admission_join_inner_remake() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_under_envs_expect(
        r##"(progn
  (defvar g3e-local 'default)
  (fset 'g3e-local-probe
        (lambda ()
          (let ((g3e-local 'bound))
            (list g3e-local
                  (backtrace-eval
                   '(let ((g3e-local 'exchange-bound)) (list g3e-local (progn (kill-local-variable 'g3e-local) (thread-join (make-thread (lambda () nil))) g3e-local) (progn (make-local-variable 'g3e-local) (setq g3e-local 'remade) g3e-local)))
                   0 'g3e-local-probe)
                  g3e-local))))
  (with-temp-buffer
    (setq-local g3e-local 'local)
    (list (g3e-local-probe) g3e-local (local-variable-p 'g3e-local))))"##,
        &[GREEN],
        expect_test::expect![[r#""OK ((bound (exchange-bound default remade) remade) remade t)""#]],
    );
}

/// Creation admits Green while the local is absent; joining after remake
/// leaves no actual missing-local swap that could retire the older entries.
#[test]
fn backtrace_local_late_admission_create_simple_remake_in_both_modes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_under_envs_expect(
        r##"(progn
  (defvar g3e-local 'default)
  (fset 'g3e-local-probe
        (lambda ()
          (let ((g3e-local 'bound))
            (list g3e-local
                  (backtrace-eval
                   '(list g3e-local (progn (kill-local-variable 'g3e-local) (setq g3e-late-worker (make-thread (lambda () nil))) g3e-local) (progn (make-local-variable 'g3e-local) (setq g3e-local 'remade) g3e-local))
                   0 'g3e-local-probe)
                  g3e-local))))
  (with-temp-buffer
    (setq-local g3e-local 'local)
    (prog1 (list (g3e-local-probe) g3e-local (local-variable-p 'g3e-local)) (thread-join g3e-late-worker))))"##,
        &[SIM, GREEN],
        expect_test::expect![[r#""OK ((bound (local default remade) bound) remade t)""#]],
    );
}

/// Creation admits Green while the local is absent; joining after remake
/// leaves no actual missing-local swap that could retire the older entries.
#[test]
fn backtrace_local_late_admission_create_inner_remake_in_both_modes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_under_envs_expect(
        r##"(progn
  (defvar g3e-local 'default)
  (fset 'g3e-local-probe
        (lambda ()
          (let ((g3e-local 'bound))
            (list g3e-local
                  (backtrace-eval
                   '(let ((g3e-local 'exchange-bound)) (list g3e-local (progn (kill-local-variable 'g3e-local) (setq g3e-late-worker (make-thread (lambda () nil))) g3e-local) (progn (make-local-variable 'g3e-local) (setq g3e-local 'remade) g3e-local)))
                   0 'g3e-local-probe)
                  g3e-local))))
  (with-temp-buffer
    (setq-local g3e-local 'local)
    (prog1 (list (g3e-local-probe) g3e-local (local-variable-p 'g3e-local)) (thread-join g3e-late-worker))))"##,
        &[SIM, GREEN],
        expect_test::expect![[r#""OK ((bound (exchange-bound default remade) bound) local t)""#]],
    );
}
