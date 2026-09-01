//! Strict combo oracle probes, batch 298: undo / redo deep. undo-boundary,
//! undo, undo-only, undo-redo, primitive-undo, and buffer-undo-list shape.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_undo_boundary_undo_redo_sequence() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "abc")
  (undo-boundary)
  (insert "def")
  (undo-boundary)
  (let ((s1 (buffer-string)))
    (undo)
    (let ((s2 (buffer-string)))
      (undo-redo)
      (list s1 s2 (buffer-string)
            (buffer-modified-p)))))
"##;
    let expect = expect_test::expect![[r#""OK (\"abcdef\" \"abc\" \"abcdef\" t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_undo_only_primitive_undo_stepwise() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "1")
  (undo-boundary)
  (insert "2")
  (undo-boundary)
  (insert "3")
  (let ((full (buffer-string)))
    (undo)
    (let ((after-undo (buffer-string)))
      (primitive-undo 1 buffer-undo-list)
      (list full after-undo (buffer-string)))))
"##;
    let expect = expect_test::expect![[r#""OK (\"123\" \"1\" \"12\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_undo_list_structure_after_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (buffer-enable-undo)
  (setq buffer-undo-list nil)
  (insert "hello")
  (let ((after-insert (length buffer-undo-list)))
    (undo-boundary)
    (delete-region 1 3)
    (let ((after-delete (length buffer-undo-list)))
      (buffer-undo-list)
      (list (> after-insert 0)
            (> after-delete after-insert)
            (consp buffer-undo-list)))))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function buffer-undo-list)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
