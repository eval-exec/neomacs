//! Strict combo oracle probes, batch 101: regex validation edge cases —
//! repeat intervals (\\{0,0\\}, \\{,3\\}, \\{999999\\}, \\{1,0\\}),
//! backreferences to non-existent groups (\\1, \\3), and zero-repetition.
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_r5_regex_repeat_interval_edge_validation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (0 0 0 \"Invalid content of \\\\{\\\\}\" \"Invalid content of \\\\{\\\\}\" \"Invalid content of \\\\{\\\\}\")""#
    ]];
    // Divergence surfaced 2026-06-27 (repeat-interval validation):
    // GNU Emacs: OK (0 0 0 "Invalid content of \\{\\}" "Invalid content of \\{\\}" "Invalid content of \\{\\}")
    // Neomacs:   OK (0 0 0 0 nil "Regular expression too big")
    // Repeat intervals a\\{1,0\\} and a\\{5,3\\} (min > max) are silently
    // accepted by Neomacs (0 = compiled, no match) but rejected by GNU Emacs.
    // a\\{999999\\} is rejected by both but with different messages: Neomacs
    // says "Regular expression too big", GNU says "Invalid content of \\{\\}".
    // a\\{0,0\\}, a\\{,3\\}, a\\{0\\} agree (0 = valid, no match). Backreference
    // to non-existent groups also agrees (batch 101 test 2).
    crate::common::assert_oracle_parity_expect(
        r##"
(list (condition-case err (string-match "a\\{0,0\\}" "text") (invalid-regexp (cadr err)) (error 'other))
      (condition-case err (string-match "a\\{,3\\}" "text") (invalid-regexp (cadr err)) (error 'other))
      (condition-case err (string-match "a\\{0\\}" "text") (invalid-regexp (cadr err)) (error 'other))
      (condition-case err (string-match "a\\{1,0\\}" "text") (invalid-regexp (cadr err)) (error 'other))
      (condition-case err (string-match "a\\{5,3\\}" "text") (invalid-regexp (cadr err)) (error 'other))
      (condition-case err (string-match "a\\{999999\\}" "text") (invalid-regexp (cadr err)) (error 'other)))
"##,
        expect,
    );
}

#[test]
fn div_r5_regex_backref_to_nonexistent_group() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"Invalid back reference\" \"Invalid back reference\" \"Invalid back reference\" nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (condition-case err (string-match "\\1" "text") (invalid-regexp (cadr err)) (error 'other))
      (condition-case err (string-match "\\3" "text") (invalid-regexp (cadr err)) (error 'other))
      (condition-case err (string-match "\\(a\\)\\2" "text") (invalid-regexp (cadr err)) (error 'other))
      (condition-case err (string-match "\\(a\\)\\1" "a") (invalid-regexp (cadr err)) (error 'other)))
"##,
        expect,
    );
}
