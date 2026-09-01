//! Oracle parity tests for GNU `copy-sequence` edge semantics.
//!
//! GNU implements this in `src/fns.c`: list/vector/record elements are shallow
//! copied, string bytes and intervals are copied, empty objects may be shared,
//! and non-sequences signal `wrong-type-argument` for `sequencep`.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_copy_sequence_shallow_list_spine_and_dotted_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((shared (list 'inner))
       (original (list shared 'tail))
       (copy (copy-sequence original)))
  (setcar copy 'changed)
  (setcar shared 'mutated)
  (list original
        copy
        (eq (car original) shared)
        (condition-case err
            (copy-sequence '(a b . c))
          (error (list (car err) (cdr err))))))
"#;

    let expect = expect_test::expect![[
        r#""OK (((mutated) tail) (changed tail) t (wrong-type-argument (listp c)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_copy_sequence_circular_list_error_payload() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((x (list 'a 'b 'c)))
  (setcdr (last x) x)
  (condition-case err
      (copy-sequence x)
    (error (list (car err) (cdr err)))))
"#;

    let expect = expect_test::expect![[r#""OK (circular-list ((a b c a b . #2)))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_copy_sequence_string_intervals_are_copied() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((s (copy-sequence "abcdef"))
       (_ (put-text-property 1 4 'face 'bold s))
       (_ (put-text-property 3 6 'mouse-face 'highlight s))
       (copy (copy-sequence s)))
  (put-text-property 2 3 'face 'italic copy)
  (list (substring-no-properties s)
        (substring-no-properties copy)
        (text-property-any 0 (length s) 'face 'bold s)
        (text-property-any 0 (length copy) 'face 'italic copy)
        (text-properties-at 2 s)
        (text-properties-at 2 copy)
        (text-properties-at 4 copy)))
"#;

    let expect = expect_test::expect![[
        r#""OK (\"abcdef\" \"abcdef\" 1 2 (face bold) (face italic) (mouse-face highlight))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_copy_sequence_vector_record_bool_vector_identity() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((shared (list 'cell))
       (v (vector shared 1))
       (vc (copy-sequence v))
       (bv (make-bool-vector 5 nil))
       (_ (aset bv 1 t))
       (_ (aset bv 4 t))
       (bvc (copy-sequence bv)))
  (aset vc 1 99)
  (aset bvc 1 nil)
  (list (eq v vc)
        (eq (aref v 0) (aref vc 0))
        (aref v 1)
        (aref vc 1)
        (list (aref bv 1) (aref bv 4))
        (list (aref bvc 1) (aref bvc 4))
        (condition-case err
            (copy-sequence 42)
          (error (list (car err) (cdr err))))))
"#;

    let expect = expect_test::expect![[
        r#""OK (nil t 1 99 (t t) (nil t) (wrong-type-argument (sequencep 42)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_copy_sequence_empty_object_identity_observable() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (eq nil (copy-sequence nil))
 (let ((s "")) (eq s (copy-sequence s)))
 (let ((v [])) (eq v (copy-sequence v)))
 (let ((b (make-bool-vector 0 nil))) (eq b (copy-sequence b))))
"#;

    let expect = expect_test::expect![[r#""OK (t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
