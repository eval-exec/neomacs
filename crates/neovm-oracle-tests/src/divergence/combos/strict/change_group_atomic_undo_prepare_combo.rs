//! Strict combo oracle probes, batch 322: change-group / atomic-change-group
//! undo atomicity. prepare-change-group, atomic-change-group, undo grouping,
//! and with-undo-amalgamate.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_prepare_change_group_undo_atomic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "start")
  (undo-boundary)
  (atomic-change-group
    (insert "A1")
    (insert "A2")
    (insert "A3"))
  (let ((s1 (buffer-string)))
    (undo)
    (let ((s2 (buffer-string)))
      (undo)
      (list s1 s2 (buffer-string)))))
"##;
    let expect = expect_test::expect![[r#""OK (\"startA1A2A3\" \"\" \"\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_change_group_amalgamate_boundary() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "base")
  (undo-boundary)
  (with-undo-amalgamate
    (insert "X")
    (undo-boundary)
    (insert "Y")
    (undo-boundary)
    (insert "Z"))
  (let ((s1 (buffer-string)))
    (undo)
    (list s1 (buffer-string))))
"##;
    let expect = expect_test::expect![[r#""OK (\"baseXYZ\" \"\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_undo_outer_limit_amalgamate_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((saved-outer-limit undo-outer-limit))
  (unwind-protect
      (with-temp-buffer
        (buffer-enable-undo)
        (setq undo-outer-limit 1000000)
        (dotimes (i 5)
          (insert (number-to-string i))
          (undo-boundary))
        (let ((full (buffer-string)))
          (undo)
          (undo)
          (list full (buffer-string) (> (length buffer-undo-list) 0))))
    (setq undo-outer-limit saved-outer-limit)))
"##;
    let expect = expect_test::expect![[r#""OK (\"01234\" \"0123\" t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
