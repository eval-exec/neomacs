//! Strict combo oracle probes, batch 374: display-buffer action variations.
//! display-buffer with display-buffer-use-some-window, pop-to-buffer,
//! and display-buffer-alist custom action.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_display_buffer_pop_to_buffer_action() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((b (get-buffer-create " *probe-pop*")))
  (unwind-protect
      (let ((w (pop-to-buffer b)))
        (list (windowp w)
              (eq (window-buffer w) b)
              (eq (window-buffer) b)))
    (delete-other-windows)
    (kill-buffer b)))
"##;
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument windowp #<killed buffer>)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_display_buffer_use_some_window_action() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((b1 (get-buffer-create " *probe-dba1*"))
      (b2 (get-buffer-create " *probe-dba2*")))
  (unwind-protect
      (let ((w1 (display-buffer b1 '(display-buffer-use-some-window)))
            (w2 (display-buffer b2 '(display-buffer-use-some-window))))
        (list (windowp w1)
              (windowp w2)
              (eq (window-buffer w1) b1)
              (eq (window-buffer w2) b2)
              (or (eq w1 w2) (not (eq w1 w2)))))
    (delete-other-windows)
    (kill-buffer b1)
    (kill-buffer b2)))
"##;
    let expect = expect_test::expect![[r#""OK (t t nil t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_display_buffer_alist_custom_override() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((b (get-buffer-create " *probe-override*"))
      (saved display-buffer-overriding-action))
  (unwind-protect
      (let ((display-buffer-overriding-action
             '((display-buffer-same-window))))
        (let ((w (display-buffer b)))
          (list (windowp w)
                (eq w (selected-window))
                (eq (window-buffer) b))))
    (setq display-buffer-overriding-action saved)
    (delete-other-windows)
    (kill-buffer b)))
"##;
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
