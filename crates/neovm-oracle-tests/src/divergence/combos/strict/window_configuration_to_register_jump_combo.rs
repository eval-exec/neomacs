//! Strict combo oracle probes, batch 327: window-configuration-to-register.
//! window-configuration-to-register + jump-to-register round-trip, register
//! config type predicate, and point preservation across register jump.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_window_configuration_to_register_jump_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((b (get-buffer-create " *probe-wcr*")))
  (unwind-protect
      (progn
        (delete-other-windows)
        (switch-to-buffer b)
        (split-window nil nil 'right)
        (window-configuration-to-register ?w)
        (delete-other-windows)
        (let ((c1 (count-windows)))
          (jump-to-register ?w)
          (list c1 (count-windows) (windowp (selected-window)))))
    (kill-buffer b)
    (delete-other-windows)
    (set-register ?w nil)))
"##;
    let expect = expect_test::expect![[r#""OK (1 2 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_register_number_string_rect_window_config_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(set-register ?n 42)
(set-register ?s "a string")
(let ((b (get-buffer-create " *probe-reg-types*")))
  (window-configuration-to-register ?c)
  (list (get-register ?n)
        (get-register ?s)
        (window-configuration-p (get-register ?c))
        (markerp (get-register ?c))
        (set-register ?c nil)
        (get-register ?c)))
"##;
    let expect = expect_test::expect![[r#""OK (42 \"a string\" nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_point_to_register_jump_to_register_preserve() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((b (get-buffer-create " *probe-ppr*")))
  (unwind-protect
      (with-current-buffer b
        (insert "0123456789")
        (goto-char 5)
        (point-to-register ?p)
        (goto-char 1)
        (let ((before (point)))
          (jump-to-register ?p)
          (list before (point) (marker-position (get-register ?p)))))
    (kill-buffer b)))
"##;
    let expect = expect_test::expect![[r#""OK (1 5 5)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
