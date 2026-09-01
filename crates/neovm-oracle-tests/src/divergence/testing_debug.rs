//! Divergence tests: ERT testing framework, assertions deep.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_ert_core() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil nil nil t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'ert-deftest)
  (fboundp 'should)
  (fboundp 'should-not)
  (fboundp 'should-error)
  (fboundp 'ert-run-tests-interactively)
  (fboundp 'ert-run-tests-batch)
  (featurep 'ert)) "#,
        expect,
    );
}

#[test]
fn divergence_ert_selectors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'ert-select-tests)
  (fboundp 'ert-test-result-type-p)
  (fboundp 'ert-pass)
  (fboundp 'ert-fail)
  (fboundp 'ert--stats)) "#,
        expect,
    );
}

#[test]
fn divergence_buttercup() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'buttercup-define-matcher)
  (fboundp 'buttercup-run)
  (featurep 'buttercup)) "#,
        expect,
    );
}

#[test]
fn divergence_ert_mock() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'ert-with-test-buffer)
  (fboundp 'ert-with-global-buffer)
  (fboundp 'ert--explain)) "#,
        expect,
    );
}

#[test]
fn divergence_debugger() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'debug)
  (fboundp 'debug-on-entry)
  (fboundp 'cancel-debug-on-entry)
  (boundp 'debug-on-error)
  (boundp 'debug-on-quit)
  (boundp 'debugger)
  (fboundp debugger)) "#,
        expect,
    );
}

#[test]
fn divergence_backtrace() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'backtrace)
  (fboundp 'backtrace-frame)
  (fboundp 'backtrace-debug)
  (fboundp 'mapbacktrace)) "#,
        expect,
    );
}

#[test]
fn divergence_trace() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'trace-function)
  (fboundp 'trace-function-foreground)
  (fboundp 'untrace-function)
  (fboundp 'untrace-all)
  (featurep 'trace)) "#,
        expect,
    );
}

#[test]
fn divergence_elisp_demos() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'shortdoc-display-groups)
  (fboundp 'shortdoc-display-function)
  (featurep 'shortdoc)) "#,
        expect,
    );
}

#[test]
fn divergence_finder() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'finder-commentary)
  (fboundp 'finder-by-keyword)
  (featurep 'finder)) "#,
        expect,
    );
}

#[test]
fn divergence_package_tests() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'ert-test)
  (fboundp 'ert-make-test)
  (fboundp 'ert-get-test)
  (fboundp 'ert-test-boundp)) "#,
        expect,
    );
}
