//! Strict combo oracle probes, batch 163: window configuration. current-window-
//! configuration + set-window-configuration round-trip (count restored),
//! save-window-excursion restoration after split, window-state-get/put with
//! buffer reconstruction, and per-window parameters survival through
//! configuration save/restore.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_window_configuration_save_set_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((b1 (get-buffer-create " *probe-wc-a*"))
      (b2 (get-buffer-create " *probe-wc-b*")))
  (unwind-protect
      (progn
        (delete-other-windows)
        (switch-to-buffer b1)
        (let ((w2 (split-window nil nil 'right)))
          (set-window-buffer w2 b2)
          (let ((cfg (current-window-configuration))
                (count-with-split (count-windows)))
            (delete-other-windows)
            (let ((count-after-delete (count-windows)))
              (set-window-configuration cfg)
              (let ((count-restored (count-windows)))
                (list count-with-split count-after-delete count-restored
                      (windowp (selected-window))))))))
    (kill-buffer b1)
    (kill-buffer b2)
    (delete-other-windows)))
"##;
    let expect = expect_test::expect![[r#""OK (2 1 2 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_save_window_excursion_restore() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((b (get-buffer-create " *probe-swe*")))
  (unwind-protect
      (progn
        (delete-other-windows)
        (switch-to-buffer b)
        (let ((inner-count (save-window-excursion
                             (split-window nil nil 'below)
                             (count-windows))))
          (list inner-count
                (count-windows)
                (eq (window-buffer) b))))
    (kill-buffer b)
    (delete-other-windows)))
"##;
    let expect = expect_test::expect![[r#""OK (2 1 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_window_state_get_put_parameter_survival() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((b (get-buffer-create " *probe-wsp*")))
  (unwind-protect
      (progn
        (delete-other-windows)
        (switch-to-buffer b)
        (let ((w (selected-window)))
          (set-window-parameter w 'probe-param 'survived)
          (set-window-parameter w 'probe-num 42)
          (let ((state (window-state-get w nil)))
            (delete-other-windows)
            (let ((w2 (selected-window)))
              (window-state-put state w2 nil)
              (list (window-parameter w2 'probe-param)
                    (window-parameter w2 'probe-num)
                    (windowp w2))))))
    (kill-buffer b)
    (delete-other-windows)))
"##;
    let expect = expect_test::expect![[r#""OK (nil nil t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
