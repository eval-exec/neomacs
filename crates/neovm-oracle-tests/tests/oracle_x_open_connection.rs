//! `x-open-connection` argument checks, before any arm.
//!
//! GNU `Fx_open_connection` does `CHECK_STRING` on DISPLAY, then
//! `CHECK_STRING` on XRM-STRING when it is non-nil, before `x_term_init`
//! (`src/xfns.c:7645-7647`, Emacs 31.1 `a360712c9d27`). Both failures are
//! `(wrong-type-argument stringp VALUE)`.
//!
//! The batch harness cannot cover a successful open: GNU `--batch` dies with
//! SIGIO on that path. Binding `initial-window-system` to `x` is only how the
//! argument rows enter Neomacs's GUI arm. A string DISPLAY there returns nil
//! on Neomacs and signals on GNU, so that row is not in this oracle.

#![cfg(unix)]

use crate::common;
use common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_x_open_connection_nil_display_is_wrong_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_expect(
        r#"(condition-case err
             (x-open-connection nil)
           (error (list (car err) (cdr err))))"#,
        expect_test::expect![[r#""OK (wrong-type-argument (stringp nil))""#]],
    );
}

#[test]
fn oracle_x_open_connection_checks_xrm_string_before_opening() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_expect(
        r#"(condition-case err
             (x-open-connection ":987" 42)
           (error (list (car err) (cdr err))))"#,
        expect_test::expect![[r#""OK (wrong-type-argument (stringp 42))""#]],
    );
}

#[test]
fn oracle_x_open_connection_active_arm_checks_display_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_expect(
        r#"(progn
            (setq initial-window-system 'x)
            (list
             (condition-case err
                 (x-open-connection 42)
               (error (list (car err) (cdr err))))
             (condition-case err
                 (x-open-connection 'not-a-string)
               (error (list (car err) (cdr err))))
             (condition-case err
                 (x-open-connection nil)
               (error (list (car err) (cdr err))))
             (condition-case err
                 (x-open-connection ":987" 42)
               (error (list (car err) (cdr err))))))"#,
        expect_test::expect![[
            r#""OK ((wrong-type-argument (stringp 42)) (wrong-type-argument (stringp not-a-string)) (wrong-type-argument (stringp nil)) (wrong-type-argument (stringp 42)))""#
        ]],
    );
}
