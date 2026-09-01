//! Strict combo oracle probes, batch 97: list navigation — forward-list,
//! backward-list, up-list, down-list across nested parentheses.
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_r1_list_navigation_forward_backward_up_down() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (scan-error \"Containing expression ends prematurely\" 15 16)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "(a (b (c d)) e)")
  (goto-char 1)
  (down-list)
  (let ((d1 (point)))
    (forward-list)
    (let ((f1 (point)))
      (down-list)
      (let ((d2 (point)))
        (up-list 2)
        (list d1 f1 d2 (point))))))
"##,
        expect,
    );
}

#[test]
fn div_r1_backward_list_and_up_list_errors() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (15 15 scan-error)""#]];
    // Divergence surfaced 2026-06-27:
    // GNU Emacs: OK (15 15 scan-error)
    // Neomacs:   ERR (wrong-type-argument symbolp (scan-error "Unbalanced parentheses" 15 1))
    // up-list -1 on unbalanced parens signals wrong-type-argument in Neomacs
    // instead of scan-error. The condition-case (scan-error ...) doesn't catch
    // it, so the error propagates. In GNU, scan-error is caught cleanly.
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "xxx (a b) yyy (c d) zzz")
  (goto-char 25)
  (backward-list)
  (let ((b1 (point)))
    (condition-case err (up-list -1) (scan-error (list 'err)))
    (list b1 (point)
          (condition-case err (up-list 99) (scan-error (car err))))))
"##,
        expect,
    );
}
