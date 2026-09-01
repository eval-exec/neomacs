//! Strict combo oracle probes, batch 270: window point/start/end dynamics.
//! window-point, set-window-point, window-start/end, set-window-start, and
//! window-point-insertion-type across a buffer with motion.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_window_point_set_start_end_dynamics() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((b (get-buffer-create " *probe-wps*")))
  (unwind-protect
      (with-current-buffer b
        (erase-buffer)
        (insert "line one\nline two\nline three\nline four\n")
        (let ((w (selected-window)))
          (set-window-buffer w b)
          (set-window-start w 1 t)
          (goto-char 1)
          (forward-line 2)
          (list (window-start w)
                (window-point w)
                (progn (set-window-point w 5) (window-point w))
                (progn (set-window-start w 12 t) (window-start w))
                (window-end w)
                (window-point-insertion-type w))))
    (kill-buffer b)
    (delete-other-windows)))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function window-point-insertion-type)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_window_hscroll_vscroll_pixel_dynamics() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((b (get-buffer-create " *probe-wps2*")))
  (unwind-protect
      (with-current-buffer b
        (erase-buffer)
        (insert (make-string 200 ?x))
        (let ((w (selected-window)))
          (set-window-buffer w b)
          (list (window-hscroll w)
                (progn (set-window-hscroll w 5) (window-hscroll w))
                (window-vscroll w nil t)
                (progn (set-window-vscroll w 3 nil t) (window-vscroll w nil t)))))
    (kill-buffer b)
    (delete-other-windows)))
"##;
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments window-vscroll 3)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_window_dedicated_buffer_edges_update() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((b (get-buffer-create " *probe-wps3*")))
  (unwind-protect
      (let ((w (selected-window)))
        (set-window-buffer w b)
        (with-current-buffer b (insert "hello"))
        (set-window-dedicated-p w t)
        (list (window-dedicated-p w)
              (window-buffer w)
              (eq (window-buffer w) b)
              (progn (set-window-dedicated-p w nil) (window-dedicated-p w))))
    (kill-buffer b)
    (delete-other-windows)))
"##;
    let expect = expect_test::expect![[r#""OK (t #<killed buffer> t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
