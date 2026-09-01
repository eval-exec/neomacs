//! Strict combo oracle probes, batch 378: window geometry / valid / live / at /
//! edges deep. window-valid-p, window-live-p, window-at with coordinates,
//! window-edges/body-edges/inside-edges/pixel-edges round-trips.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_window_valid_live_at_edges_geometry() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((w (selected-window)))
  (delete-other-windows)
  (list (window-valid-p w)
        (window-live-p w)
        (windowp w)
        (window-at 0 0)
        (windowp (window-at 0 0))
        (eq (window-at 0 0) w)
        (consp (window-edges w))
        (consp (window-body-edges w))
        (consp (window-inside-edges w))
        (consp (window-pixel-edges w))))
"##;
    let expect = expect_test::expect![[r#""OK (t t t #<window 1 on *scratch*> t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_window_root_child_sibling_walk() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((b (get-buffer-create " *probe-walk*")))
  (unwind-protect
      (progn
        (delete-other-windows)
        (switch-to-buffer b)
        (split-window nil nil 'right)
        (let ((root (window-root)))
          (list (windowp root)
                (eq root (window-parent (selected-window)))
                (>= (window-child-count root) 1)
                (windowp (window-child root 0))
                (eq (window-next-sibling (selected-window))
                    (window-next-sibling (selected-window))))))
    (kill-buffer b)
    (delete-other-windows)))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function window-root)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_window_total_body_pixel_size_ratio() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((w (selected-window)))
  (delete-other-windows)
  (let ((total-w (window-total-width w))
        (total-h (window-total-height w))
        (body-w (window-body-width w))
        (body-h (window-body-height w)))
    (list (integerp total-w)
          (integerp total-h)
          (>= total-w body-w)
          (>= total-h body-h)
          (>= body-w 1)
          (>= body-h 1))))
"##;
    let expect = expect_test::expect![[r#""OK (t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
