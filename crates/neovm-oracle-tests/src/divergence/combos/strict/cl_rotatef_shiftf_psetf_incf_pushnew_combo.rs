//! Strict combo oracle probes, batch 195: cl-lib generalized assignment.
//! cl-rotatef (cyclic multi-place swap), cl-shiftf (shift-with-default),
//! cl-psetf (parallel assignment), cl-incf/cl-decf on generalized places,
//! and cl-pushnew with :test.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_cl_rotatef_shiftf_psetf() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (let ((x 5) (y 10) (z 15))
        (cl-rotatef x y z)
        (list x y z))
      (let ((x 5) (y 10) (z 15))
        (cl-rotatef x y)
        (list x y z))
      (let ((x 5) (y 10))
        (cl-shiftf x y 99)
        (list x y))
      (let ((a 1) (b 2))
        (cl-psetf a b b a)
        (list a b))
      (let ((a 1) (b 2) (c 3))
        (cl-psetf a 100 b 200 c 300)
        (list a b c)))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function cl-rotatef)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_cl_incf_decf_generalized_places() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((lst (list 1 2 3))
      (vec (vector 10 20 30))
      (ht (make-hash-table)))
  (puthash 'k 100 ht)
  (cl-incf (car lst) 10)
  (cl-decf (nth 2 lst))
  (cl-incf (aref vec 1))
  (cl-decf (gethash 'k ht) 5)
  (list lst
        vec
        (gethash 'k ht)
        (let ((x 5)) (cl-incf x x) x)
        (let ((x 10)) (cl-decf x 3) x)))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function cl-decf)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_cl_push_pushnew_pop_remf_getf_plist() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((stack nil)
      (plist '(a 1 b 2)))
  (cl-pushnew 3 stack)
  (cl-pushnew 1 stack)
  (cl-pushnew 3 stack)   ;; already present, no-op
  (cl-pushnew 2 stack :test #'=)   ;; 2 not equal to 3/1 numerically? = compares
  (let ((popped (cl-pop stack)))
    (list stack popped
          (cl-getf plist 'a)
          (cl-getf plist 'b)
          (cl-getf plist 'c 'missing)
          (progn (cl-remf plist 'a) (cl-getf plist 'a 'gone)))))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function cl-pushnew)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
