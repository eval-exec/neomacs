/// Batch 525: bool-vector operations - all ops on all vector types.
use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx525_bool_vector_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (#&3\"\u{5}\" #&3\"\u{2}\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (bool-vector t nil t) (bool-vector nil t nil))
"##,
        expect,
    );
}

#[test]
fn div_cx525_bool_vector_count_pop() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((bv (bool-vector t nil t nil t)))
  (list (bool-vector-count-population bv)
        (bool-vector-count-consecutive bv t 0)
        (bool-vector-count-consecutive bv nil 1)))
"##,
        expect,
    );
}

#[test]
fn div_cx525_bool_vector_union() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK #&3\"\u{7}\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((a (bool-vector t nil t))
      (b (bool-vector nil t nil))
      (c (bool-vector nil nil nil)))
  (bool-vector-union a b c)
  c)
"##,
        expect,
    );
}

#[test]
fn div_cx525_bool_vector_intersection() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK #&3\"\\0\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((a (bool-vector t nil t))
      (b (bool-vector nil t nil))
      (c (bool-vector nil nil nil)))
  (bool-vector-intersection a b c)
  c)
"##,
        expect,
    );
}

#[test]
fn div_cx525_bool_vector_difference() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function bool-vector-difference)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((a (bool-vector t nil t))
      (b (bool-vector nil t nil))
      (c (bool-vector nil nil nil)))
  (bool-vector-difference a b c)
  c)
"##,
        expect,
    );
}

#[test]
fn div_cx525_bool_vector_exclusive_or() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK #&3\"\u{7}\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((a (bool-vector t nil t))
      (b (bool-vector nil t nil))
      (c (bool-vector nil nil nil)))
  (bool-vector-exclusive-or a b c)
  c)
"##,
        expect,
    );
}

#[test]
fn div_cx525_bool_vector_subsetp() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((a (bool-vector t nil t))
      (b (bool-vector t nil t))
      (c (bool-vector t t t)))
  (list (bool-vector-subsetp a b) (bool-vector-subsetp a c)))
"##,
        expect,
    );
}

#[test]
fn div_cx525_bool_vector_not() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK #&3\"\u{2}\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((a (bool-vector t nil t))
      (c (bool-vector nil nil nil)))
  (bool-vector-not a c)
  c)
"##,
        expect,
    );
}

#[test]
fn div_cx525_bool_vector_make() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (#&5\"\u{1f}\" #&3\"\\0\" #&3\"\u{3}\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (make-bool-vector 5 t) (make-bool-vector 3 nil) (bool-vector t t nil))
"##,
        expect,
    );
}

#[test]
fn div_cx525_bool_vector_aref_aset() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((bv (make-bool-vector 5 nil)))
  (aset bv 2 t)
  (list (aref bv 0) (aref bv 2) (aref bv 4)))
"##,
        expect,
    );
}

#[test]
fn div_cx525_bool_vector_different_lengths() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK wrong-length-argument""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (let ((a (bool-vector t)) (b (bool-vector nil nil)) (c (bool-vector nil)))
      (bool-vector-union a b c))
  (error (car e)))
"##,
        expect,
    );
}

#[test]
fn div_cx525_bool_vector_empty() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (#&0\"\" 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (make-bool-vector 0 t) (bool-vector-count-population (make-bool-vector 0 nil)))
"##,
        expect,
    );
}

#[test]
fn div_cx525_bool_vector_all_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 0""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((bv (make-bool-vector 10 nil)))
  (bool-vector-count-population bv))
"##,
        expect,
    );
}

#[test]
fn div_cx525_bool_vector_all_t() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 10""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((bv (make-bool-vector 10 t)))
  (bool-vector-count-consecutive bv t 0))
"##,
        expect,
    );
}

#[test]
fn div_cx525_bool_vector_long() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 34""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((bv (make-bool-vector 100 nil)))
  (dotimes (i 100) (when (zerop (mod i 3)) (aset bv i t)))
  (bool-vector-count-population bv))
"##,
        expect,
    );
}
