//! Strict combo oracle probes, batch 98: syntax error handling — scan-lists,
//! up-list, forward-sexp, backward-sexp with excessive counts / unbalanced
//! parens, checking whether scan-error vs wrong-type-argument is signaled.
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_r2_syntax_error_handling_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil scan-error scan-error nil nil)""#]];
    // Divergence surfaced 2026-06-27 (up-list error handling):
    // GNU Emacs: OK (nil nil scan-error scan-error nil nil)
    // Neomacs:   OK (nil nil other-error other-error nil nil)
    // up-list with excessive count (±99) signals scan-error in GNU but a
    // non-scan-error (caught by the generic error fallback → other-error) in
    // Neomacs. scan-lists and forward/backward-sexp error handling agree (nil).
    // Same root cause as batch 97's up-list -1 divergence.
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "(a b (c d))")
  (goto-char 1)
  (list (condition-case err (scan-lists 1 99 0) (scan-error (car err)) (error 'other-error))
        (condition-case err (scan-lists 1 -99 0) (scan-error (car err)) (error 'other-error))
        (condition-case err (up-list 99) (scan-error (car err)) (error 'other-error))
        (condition-case err (up-list -99) (scan-error (car err)) (error 'other-error))
        (condition-case err (forward-sexp 99) (scan-error (car err)) (error 'other-error))
        (condition-case err (backward-sexp 99) (scan-error (car err)) (error 'other-error))))
"##,
        expect,
    );
}

#[test]
fn div_r2_scan_sexps_error_handling() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil 4 8)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "(a) (b) (c)")
  (goto-char 1)
  (list (condition-case err (scan-sexps 1 99) (scan-error (car err)) (error 'other-error))
        (condition-case err (scan-sexps 1 -99) (scan-error (car err)) (error 'other-error))
        (scan-sexps 1 1)
        (scan-sexps 1 2)))
"##,
        expect,
    );
}
