//! Strict combo oracle probes, batch 94: window introspection — window-resizable,
//! window-safe, window-at with split, and window-combination-limit.
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_q8_window_resizable_and_safe() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument number-or-marker-p nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((b (get-buffer-create " *probe-resize*")))
  (unwind-protect
      (progn
        (delete-other-windows)
        (switch-to-buffer b)
        (let ((w2 (split-window nil nil 'right)))
          (list (window-resizable nil 5 nil nil nil)
                (window-resizable w2 nil 3 nil nil)
                (window-safe nil)
                (window-safe w2)
                (window-combination-limit nil)
                (window-combination-resize nil))))
    (when (buffer-live-p b) (kill-buffer b))
    (delete-other-windows)))
"##,
        expect,
    );
}

#[test]
fn div_q8_window_at_with_split() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((b (get-buffer-create " *probe-wat*")))
  (unwind-protect
      (progn
        (delete-other-windows)
        (switch-to-buffer b)
        (let ((w2 (split-window nil nil 'right)))
          (list (eq (window-at 0 0) (selected-window))
                (window-live-p (window-at 0 0))
                (eq (window-at (window-total-width) 0) w2))))
    (when (buffer-live-p b) (kill-buffer b))
    (delete-other-windows)))
"##,
        expect,
    );
}
