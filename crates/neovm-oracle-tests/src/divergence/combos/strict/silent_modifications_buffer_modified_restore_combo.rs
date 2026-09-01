//! Strict combo oracle probes, batch 299: with-silent-modifications +
//! buffer-modified-p + restore-buffer-modified-p deep.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_with_silent_modifications_preserves_flag() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "x")
  (not-modified)
  (let ((mod-before (buffer-modified-p)))
    (with-silent-modifications
      (insert "y"))
    (list mod-before
          (buffer-modified-p)
          (buffer-string))))
"##;
    let expect = expect_test::expect![[r#""OK (nil nil \"xy\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_restore_buffer_modified_p_set_modified() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "x")
  (let ((m1 (buffer-modified-p)))
    (set-buffer-modified-p t)
    (let ((m2 (buffer-modified-p)))
      (set-buffer-modified-p nil)
      (let ((m3 (buffer-modified-p)))
        (restore-buffer-modified-p t)
        (list m1 m2 m3 (buffer-modified-p))))))
"##;
    let expect = expect_test::expect![[r#""OK (t t nil t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_buffer_modified_tick_consistent() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "hello")
  (let ((tick1 (buffer-modified-tick))
        (tick2 (buffer-chars-modified-tick)))
    (insert " world")
    (let ((tick3 (buffer-modified-tick))
          (tick4 (buffer-chars-modified-tick)))
      (list (integerp tick1)
            (integerp tick2)
            (/= tick1 tick3)
            (/= tick2 tick4)
            (> tick3 tick1)))))
"##;
    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
