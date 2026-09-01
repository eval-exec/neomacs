//! Strict combo oracle probes, batch 369: buffer-undo-list structure.
//! Undo list element types: insertion, deletion, marker adjustment,
//! text-property change, and boundary markers.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_undo_list_insertion_deletion_structure() {
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
      (list (> after-insert 0)
            (> after-delete after-insert)
            (consp buffer-undo-list)
            (memq nil buffer-undo-list)))))
"##;
    let expect = expect_test::expect![[r#""OK (t t t (nil (1 . 6) (t . 0)))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_undo_list_text_property_marker_elements() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (buffer-enable-undo)
  (setq buffer-undo-list nil)
  (insert "AAAAABBBBB")
  (undo-boundary)
  (add-text-properties 1 6 '(face bold))
  (let ((after-prop (length buffer-undo-list)))
    (undo-boundary)
    (let ((m (set-marker (make-marker) 5)))
      (set-marker m 7)
      (undo-boundary)
      (list (> after-prop 0)
            (consp buffer-undo-list)
            (> (length buffer-undo-list) after-prop)))))
"##;
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_undo_boundary_grouping_primitive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (buffer-enable-undo)
  (setq buffer-undo-list nil)
  (insert "a")
  (undo-boundary)
  (insert "b")
  (undo-boundary)
  (insert "c")
  (let ((boundaries (cl-count nil buffer-undo-list)))
    (list boundaries
          (>= boundaries 2)
          (consp buffer-undo-list))))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function cl-count)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
