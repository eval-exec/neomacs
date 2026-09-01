//! Strict combo oracle probes, batch 331: window scroll / recenter / window-
//! start/end deep. set-window-start, recenter, window-start/end, pos-visible-in-
//! window-p, and window-end update with NO-UPDATE flag.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_window_start_set_recenter_dynamics() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((b (get-buffer-create " *probe-scroll*")))
  (unwind-protect
      (with-current-buffer b
        (erase-buffer)
        (dotimes (i 100) (insert (format "line %d\n" i)))
        (let ((w (selected-window)))
          (set-window-buffer w b)
          (set-window-start w 1 t)
          (let ((start1 (window-start w)))
            (set-window-start w 500 t)
            (let ((start2 (window-start w)))
              (recenter 0)
              (list start1
                    start2
                    (window-start w)
                    (>= (window-end w) (window-start w)))))))
    (kill-buffer b)
    (delete-other-windows)))
"##;
    let expect = expect_test::expect![[r#""OK (1 500 791 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_pos_visible_in_window_p_window_end_update() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((b (get-buffer-create " *probe-vis*")))
  (unwind-protect
      (with-current-buffer b
        (erase-buffer)
        (dotimes (i 50) (insert (format "row %d\n" i)))
        (let ((w (selected-window)))
          (set-window-buffer w b)
          (set-window-start w 1 t)
          (list (pos-visible-in-window-p 1 w)
                (integerp (window-end w t))
                (>= (window-end w t) (window-start w))
                (goto-char (window-start w))
                (integerp (window-end w)))))
    (kill-buffer b)
    (delete-other-windows)))
"##;
    let expect = expect_test::expect![[r#""OK (nil t t 1 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
