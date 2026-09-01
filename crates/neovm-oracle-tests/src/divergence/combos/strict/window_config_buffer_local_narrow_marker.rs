//! Strict combo oracle probes, batch 320: multi-subsystem combo -- window
//! configuration + buffer-local + narrowing + marker + save-excursion.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_combo_window_config_buffer_local_narrow_marker() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((b (get-buffer-create " *probe-combo-wc*")))
  (unwind-protect
      (with-current-buffer b
        (insert "AAAABBBBCCCCDDDDEEEEFFFF")
        (make-local-variable 'probe-combo-var)
        (setq probe-combo-var 'local)
        (narrow-to-region 5 16)
        (let ((m (set-marker (make-marker) 10)))
          (delete-other-windows)
          (switch-to-buffer b)
          (let ((cfg (current-window-configuration))
                (count1 (count-windows)))
            (split-window nil nil 'right)
            (let ((count2 (count-windows)))
              (set-window-configuration cfg)
              (list (point-min) (point-max)
                    (marker-position m)
                    probe-combo-var
                    (default-value 'probe-combo-var)
                    count1 count2
                    (count-windows))))))
    (kill-buffer b)
    (delete-other-windows)))
"##;
    let expect = expect_test::expect![[r#""ERR (void-variable probe-combo-var)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_combo_indirect_buffer_narrow_independent_marker() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let* ((base (generate-new-buffer " *probe-combo-ind-base*"))
       (ind (make-indirect-buffer base " *probe-combo-ind*")))
  (unwind-protect
      (progn
        (with-current-buffer base
          (insert "AAAABBBBCCCCDDDD")
          (buffer-enable-undo))
        (with-current-buffer ind
          (narrow-to-region 5 12))
        (list (with-current-buffer base (cons (point-min) (point-max)))
              (with-current-buffer ind (cons (point-min) (point-max)))
              (with-current-buffer base (buffer-size))
              (with-current-buffer ind (buffer-size))
              (eq (buffer-base-buffer ind) base)))
    (kill-buffer ind)
    (kill-buffer base)))
"##;
    let expect = expect_test::expect![[r#""OK ((1 . 17) (5 . 12) 16 16 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_combo_save_excursion_restriction_match_data_restore() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "0123456789ABCDEFGHIJ")
  (string-match "DEFG" (buffer-string))
  (let ((md (match-data))
        (result (save-excursion
                  (save-restriction
                    (narrow-to-region 5 15)
                    (goto-char (point-min))
                    (forward-char 3)
                    (list (point) (point-min) (point-max))))))
    (list result
          (point)
          (widen)
          (point-min)
          (point-max)
          (equal (match-data) md))))
"##;
    let expect = expect_test::expect![[r#""OK ((8 5 15) 21 nil 1 21 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
