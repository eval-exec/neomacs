//! Strict combo oracle probes, batch 104: unknown format specifiers (%z/%l/%j/
//! %_), empty/nil regex, and deeply-nested regex grouping.
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_r8_unknown_format_specifiers() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (error error error error error error)""#]];
    crate::common::assert_oracle_parity_expect(
        r####"
(list (condition-case err (format "%z" 1) (error (car err)))
      (condition-case err (format "%l" 1) (error (car err)))
      (condition-case err (format "%j" 1) (error (car err)))
      (condition-case err (format "%_" 1) (error (car err)))
      (condition-case err (format "%q" 1) (error (car err)))
      (condition-case err (format "%r" 1) (error (car err))))
"####,
        expect,
    );
}

#[test]
fn div_r8_empty_and_nil_regex() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 caught nil caught)""#]];
    crate::common::assert_oracle_parity_expect(
        r####"
(list (string-match "" "text")
      (condition-case err (string-match nil "text") (wrong-type-argument 'caught) (error 'other))
      (string-match "x" "")
      (condition-case err (re-search-forward nil) (wrong-type-argument 'caught) (error 'other)))
"####,
        expect,
    );
}

#[test]
fn div_r8_deeply_nested_regex() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range #<buffer *scratch*> 0 5)""#]];
    crate::common::assert_oracle_parity_expect(
        r####"
(list (string-match-p "\\((((((((a))))))))\\)" "((((((((a))))))))")
      (string-match-p "\\(a\\|b\\|c\\|d\\|e\\|f\\)" "c")
      (match-data)
      (and (string-match "\\([^x]+\\)\\([^y]+\\)" "abcdez") (list (match-string 1) (match-string 2))))
"####,
        expect,
    );
}
