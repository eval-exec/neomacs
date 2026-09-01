//! Strict combo oracle probes, batch 175: bool-vector operations.
//! make-bool-vector, bool-vector literal, bool-vector-p, aref/aset, length,
//! bool-vector-count (popcount), and bitwise set ops (intersection/union/
//! difference/xor/subsetp) over pairs.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_bool_vector_make_aset_aref_length_count() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((bv (make-bool-vector 8 nil)))
  (aset bv 0 t)
  (aset bv 3 t)
  (aset bv 7 t)
  (list (bool-vector-p bv)
        (bool-vector-p [t nil])
        (bool-vector-p "x")
        (length bv)
        (aref bv 0)
        (aref bv 1)
        (aref bv 3)
        (aref bv 7)
        (bool-vector-count bv)
        (bool-vector-count (bool-vector t t nil t))
        (bool-vector-count (bool-vector))))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function bool-vector-count)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_bool_vector_bitwise_set_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((a (bool-vector t nil t nil t nil t nil))
      (b (bool-vector nil nil t t nil nil t t)))
  (list (bool-vector-count (bool-vector-intersection a b))
        (bool-vector-count (bool-vector-union a b))
        (bool-vector-count (bool-vector-xor a b))
        (bool-vector-count (bool-vector-difference a b))
        (bool-vector-subsetp (bool-vector t t t t) (bool-vector t t t t))
        (bool-vector-subsetp a b)
        (bool-vector-subsetp (make-bool-vector 4 nil) a)
        (aref (bool-vector-intersection a b) 2)
        (aref (bool-vector-union a b) 0)
        (aref (bool-vector-xor a b) 1)
        (aref (bool-vector-difference a b) 0)))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function bool-vector-count)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_bool_vector_resize_fill_map_over() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((bv (make-bool-vector 5 t)))
  (list (bool-vector-count bv)
        (aref bv 0)
        (aref bv 4)
        (let ((collected nil))
          (mapc (lambda (x) (push x collected)) bv)
          (length collected))
        (bool-vector-p (make-bool-vector 0 nil))
        (length (make-bool-vector 0 nil))
        (let ((b2 (bool-vector nil nil nil nil)))
          (aset b2 2 t)
          (list (aref b2 2) (bool-vector-count b2)))))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function bool-vector-count)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
