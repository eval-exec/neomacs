//! Divergence tests: edebug, trace, elp, and debugging facilities.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_trace_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'trace-function)
  (fboundp 'trace-function-foreground)
  (fboundp 'untrace-function)
  (fboundp 'untrace-all))"#,
        expect,
    );
}

#[test]
fn divergence_backtrace_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'backtrace)
  (fboundp 'backtrace-frame)
  (fboundp 'mapbacktrace))"#,
        expect,
    );
}

#[test]
fn divergence_test_cover() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'testcover-start)
  (fboundp 'testcover-end)
  (featurep 'testcover))"#,
        expect,
    );
}

#[test]
fn divergence_ert_framework() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'ert-deftest)
  (fboundp 'ert-run-tests-interactively)
  (featurep 'ert))"#,
        expect,
    );
}

#[test]
fn divergence_checkdoc() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'checkdoc)
  (fboundp 'checkdoc-current-buffer)
  (featurep 'checkdoc))"#,
        expect,
    );
}

#[test]
fn divergence_lisp_interaction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'eval-last-sexp)
  (fboundp 'eval-print-last-sexp)
  (fboundp 'eval-expression)
  (fboundp 'ielm))"#,
        expect,
    );
}

#[test]
fn divergence_elisp_bytecomp_warnings() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable byte-compile-warnings)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (boundp 'byte-compile-warnings)
  (listp byte-compile-warnings)
  (boundp 'byte-compile-verbose)
  (booleanp byte-compile-verbose))"#,
        expect,
    );
}

#[test]
fn divergence_subr_depth() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function max-specpdl-size)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (>= (max-specpdl-size) 100)
  (>= (max-lisp-eval-depth) 100)
  (integerp max-specpdl-size)
  (integerp max-lisp-eval-depth))"#,
        expect,
    );
}

#[test]
fn divergence_lread_variables() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable load-dangerously-install-links)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (integerp read-circle)
  (integerp load-read-function)
  (booleanp load-dangerously-install-links))"#,
        expect,
    );
}
