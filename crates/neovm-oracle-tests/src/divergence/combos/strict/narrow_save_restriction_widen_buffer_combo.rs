//! Strict combo oracle probes, batch 296: narrowing + save-restriction deep.
//! narrow-to-region, point-min/max under narrowing, buffer-size, save-
//! restriction restore, and buffer-narrowed-p.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_narrow_to_region_point_min_max_buffer_size() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "AAAABBBBCCCCDDDD")
  (narrow-to-region 5 12)
  (list (point-min)
        (point-max)
        (buffer-size)
        (buffer-substring (point-min) (point-max))
        (buffer-narrowed-p)))
"##;
    let expect = expect_test::expect![[r#""OK (5 12 16 \"BBBBCCC\" t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_save_restriction_widen_restore() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "AAAABBBBCCCCDDDD")
  (narrow-to-region 5 12)
  (let ((inner (save-restriction
                 (widen)
                 (list (point-min) (point-max) (buffer-size)))))
    (list inner
          (point-min)
          (point-max)
          (buffer-size))))
"##;
    let expect = expect_test::expect![[r#""OK ((1 17 16) 5 12 16)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_save_excursion_narrow_combined_motion() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "0123456789ABCDEF")
  (goto-char 8)
  (let ((result (save-excursion
                  (save-restriction
                    (narrow-to-region 5 12)
                    (goto-char (point-min))
                    (forward-char 3)
                    (list (point) (point-min) (point-max))))))
    (list result (point) (buffer-size))))
"##;
    let expect = expect_test::expect![[r#""OK ((8 5 12) 8 16)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
