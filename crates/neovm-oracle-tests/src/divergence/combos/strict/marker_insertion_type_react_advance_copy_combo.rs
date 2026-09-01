//! Strict combo oracle probes, batch 184: marker insertion-type and reactivity.
//! marker-insertion-type toggle (nil = stays, t = advances on insert before
//! it), marker-react-to-insertion, copy-marker isolation, and marker tracking
//! through delete-region / insert-before / insert-after sequences.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_marker_insertion_type_advance_vs_stay() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "ABCDEF")
  (let ((m-stay (set-marker (make-marker) 4))
        (m-adv (set-marker (make-marker) 4)))
    (set-marker-insertion-type m-adv t)
    (goto-char 4)
    (insert "X")
    (list (marker-position m-stay)
          (marker-position m-adv)
          (marker-insertion-type m-stay)
          (marker-insertion-type m-adv)
          (buffer-string))))
"##;
    let expect = expect_test::expect![[r#""OK (4 5 nil t \"ABCXDEF\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_marker_react_to_insertion_delete_combination() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "0123456789")
  (let ((m1 (set-marker (make-marker) 5))
        (m2 (set-marker (make-marker) 5))
        (m3 (set-marker (make-marker) 5)))
    (set-marker-insertion-type m2 t)
    (marker-react-to-insertion m3 4 2 1)
    (list (marker-position m1)
          (marker-position m2)
          (marker-position m3)
          (marker-buffer m1)
          (buffer-substring 1 5))))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function marker-react-to-insertion)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_marker_copy_isolation_delete_sequence() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "ABCDEFGHIJ")
  (let* ((m1 (set-marker (make-marker) 5))
         (m2 (copy-marker m1))
         (m3 (copy-marker m1 t)))
    (list (marker-position m1)
          (marker-position m2)
          (marker-position m3)
          (marker-insertion-type m3)
          (progn (delete-region 1 3) (marker-position m1))
          (marker-position m2)
          (marker-position m3)
          (eq (marker-buffer m1) (marker-buffer m2))
          (eq m1 m2))))
"##;
    let expect = expect_test::expect![[r#""OK (5 5 5 t 3 3 3 t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
