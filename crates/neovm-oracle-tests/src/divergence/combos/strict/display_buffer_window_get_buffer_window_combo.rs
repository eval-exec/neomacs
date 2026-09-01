//! Strict combo oracle probes, batch 325: display-buffer / window management.
//! display-buffer return + buffer association, get-buffer-window,
//! display-buffer-base-action, and window-edges consistency.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_display_buffer_get_buffer_window() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((buf (get-buffer-create " *probe-dba*")))
  (unwind-protect
      (let ((w (display-buffer buf)))
        (list (windowp w)
              (eq (window-buffer w) buf)
              (eq (get-buffer-window buf) w)
              (consp (window-edges w))
              (window-live-p w)))
    (when (get-buffer-window buf) (delete-window (get-buffer-window buf)))
    (kill-buffer buf)))
"##;
    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_display_buffer_pop_up_frame_action() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((buf (get-buffer-create " *probe-dbf*")))
  (unwind-protect
      (let* ((saved display-buffer-alist)
             (w (let ((display-buffer-alist nil))
                  (display-buffer buf '(display-buffer-same-window)))))
        (prog1
            (list (windowp w)
                  (eq (window-buffer w) buf)
                  (eq w (selected-window)))
          (setq display-buffer-alist saved)))
    (kill-buffer buf)))
"##;
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_get_buffer_window_list_all_frames() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((buf (get-buffer-create " *probe-gbwl*")))
  (unwind-protect
      (let ((w (display-buffer buf)))
        (list (consp (get-buffer-window-list buf))
              (consp (member w (get-buffer-window-list buf)))
              (windowp (car (get-buffer-window-list buf)))
              (eq (length (get-buffer-window-list buf)) 1)
              (windowp w)))
    (when (get-buffer-window buf) (delete-window (get-buffer-window buf)))
    (kill-buffer buf)))
"##;
    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
