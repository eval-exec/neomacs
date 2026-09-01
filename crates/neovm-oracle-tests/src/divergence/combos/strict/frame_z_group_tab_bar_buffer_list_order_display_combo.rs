//! Strict combo oracle probes, batch 134: frame z-group, tab-bar-lines,
//! buffer-list ordering after bury/unbury combo, display-buffer-alist
//! custom action, and window-text-representation.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_u8_frame_z_group_and_tab_bar_lines() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (frame-parameter nil 'z-group)
      (default-value 'tab-bar-lines)
      (frame-parameter nil 'tab-bar-lines)
      (boundp 'tab-bar-show)
      (boundp 'tab-bar-mode)
      (boundp 'tab-bar-separator)
      (boundp 'tab-bar-format)
      (boundp 'tab-bar-new-tab-choice))
"##;
    let expect = expect_test::expect![[r#""ERR (void-variable tab-bar-lines)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_u8_buffer_list_order_after_bury_unbury() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((a (get-buffer-create " *probe-blo2-a*"))
      (b (get-buffer-create " *probe-blo2-b*"))
      (c (get-buffer-create " *probe-blo2-c*")))
  (unwind-protect
      (progn
        (switch-to-buffer a)
        (switch-to-buffer b)
        (switch-to-buffer c)
        (let ((before (mapcar #'buffer-name (buffer-list))))
          (bury-buffer b)
          (let ((after-bury (mapcar #'buffer-name (buffer-list))))
            (unbury-buffer b)
            (let ((after-unbury (mapcar #'buffer-name (buffer-list))))
              (list (car before)
                    (eq (car (buffer-list)) c)
                    (buffer-name (car (last (buffer-list))))))))))
    (kill-buffer a)
    (kill-buffer b)
    (kill-buffer c)))
"##;
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (0 . 0) 1)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_u8_display_buffer_alist_custom() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((b (get-buffer-create " *probe-dba*"))
      (display-buffer-alist
       '(("\\*probe-dba\\*" display-buffer-same-window))))
  (unwind-protect
      (let ((w (display-buffer b)))
        (prog1
            (list (windowp w)
                  (eq w (selected-window))
                  (buffer-name (window-buffer w))
                  (count-windows))
          (when (and w (not (eq w (selected-window))))
            (delete-window w))))
    (kill-buffer b)
    (delete-other-windows)))
"##;
    let expect = expect_test::expect![[r#""OK (t t \" *probe-dba*\" 1)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_u8_window_text_representation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((b (get-buffer-create " *probe-wtr*")))
  (unwind-protect
      (progn
        (delete-other-windows)
        (switch-to-buffer b)
        (list (window-text-representation)
              (frame-char-width)
              (frame-char-height)
              (default-font-width)
              (default-font-height)
              (window-text-pixel-size nil nil nil t)))
    (kill-buffer b)
    (delete-other-windows)))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function window-text-representation)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_u8_cl_loop_with_hash_and_accumulate_and_finally() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((h (make-hash-table :test 'equal)))
  (puthash 'x 10 h)
  (puthash 'y 20 h)
  (puthash 'z 30 h)
  (cl-loop for k being the hash-keys of h using (hash-values v)
           with total = 0
           do (setq total (+ total v))
           if (> v 15)
             collect (cons k v) into big
           else
             collect (cons k v) into small
           end
           finally (return (list total big small))))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
