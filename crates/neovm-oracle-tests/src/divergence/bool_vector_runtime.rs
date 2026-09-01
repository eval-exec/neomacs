//! bool-vector parity: aref/length/count-population, set ops with an explicit
//! destination, subsetp; plus the nil-destination divergence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn bool_vector_aref() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil 8 7)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((bv (make-bool-vector 8 t)))
  (aset bv 3 nil)
  (list (aref bv 0) (aref bv 3) (length bv) (bool-vector-count-population bv)))"##,
        expect,
    );
}

#[test]
fn bool_vector_with_dest() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((t t t nil) 2 t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((a (bool-vector t nil t nil)) (b (bool-vector t t nil nil))
      (d (make-bool-vector 4 nil)))
  (list (append (bool-vector-union a b d) nil)
        (bool-vector-count-population a)
        (bool-vector-subsetp (bool-vector t nil nil) (bool-vector t t nil))
        (bool-vector-subsetp (bool-vector t t nil) (bool-vector t nil nil))))"##,
        expect,
    );
}

#[test]
fn divergence_bool_vector_set_ops_nil_dest() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((t t t nil) (t nil nil nil) (nil t nil t))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((a (bool-vector t nil t nil)) (b (bool-vector t t nil nil)))
  (list (append (bool-vector-union a b nil) nil)
        (append (bool-vector-intersection a b nil) nil)
        (append (bool-vector-not a nil) nil)))"##,
        expect,
    );
}
