//! Strict combo oracle probes, batch 379: window-walk-windows + balance-windows.
//! walk-windows iteration, balance-windows geometry, and next-window/
//! previous-window across the frame.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_walk_windows_next_previous_balance() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((b (get-buffer-create " *probe-walk2*")))
  (unwind-protect
      (progn
        (delete-other-windows)
        (switch-to-buffer b)
        (split-window nil nil 'right)
        (split-window nil nil 'below)
        (let ((count 0))
          (walk-windows (lambda (w) (setq count (1+ count))))
          (let ((balanced (progn (balance-windows) (walk-windows (lambda (w) (setq count count))))))
            (list count
                  (>= count 2)
                  (windowp (next-window))
                  (windowp (previous-window))
                  (eq (next-window (next-window)) (selected-window))))))
    (kill-buffer b)
    (delete-other-windows)))
"##;
    let expect = expect_test::expect![[r#""OK (3 t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_window_combination_resize_after_split() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((b (get-buffer-create " *probe-resize*")))
  (unwind-protect
      (progn
        (delete-other-windows)
        (switch-to-buffer b)
        (let ((w (selected-window))
              (initial-height (window-total-height)))
          (let ((w2 (split-window nil nil 'below)))
            (list (>= (window-total-height w) 1)
                  (>= (window-total-height w2) 1)
                  (>= initial-height (+ (window-total-height w) (window-total-height w2) -1))))))
    (kill-buffer b)
    (delete-other-windows)))
"##;
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_get_lru_window_get_largest_window() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((b (get-buffer-create " *probe-lru*")))
  (unwind-protect
      (progn
        (delete-other-windows)
        (switch-to-buffer b)
        (split-window nil nil 'right)
        (let ((lru (get-lru-window))
              (largest (get-largest-window)))
          (list (windowp lru)
                (windowp largest)
                (eq largest (get-largest-window))
                (>= (window-total-width largest) (window-total-width lru)))))
    (kill-buffer b)
    (delete-other-windows)))
"##;
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
