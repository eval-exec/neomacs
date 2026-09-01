//! Complex combo batch 286 — `cl-typep` with complex type specifiers:
//! `(satisfies fn)`, `(member ...)`, `(integer low high)`, `(or ...)`,
//! `(and ...)`, `(not ...)`, `(cons type1 type2)`, `(vector ...)`.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx286_cl_typep_satisfies_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-typep)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (cl-typep 4 '(satisfies evenp))
      (cl-typep 5 '(satisfies evenp))
      (cl-typep 5 '(satisfies oddp))
      (cl-typep "abc" '(satisfies stringp))
      (cl-typep 42 '(satisfies stringp)))
"##,
        expect,
    )
}

#[test]
fn div_cx286_cl_typep_member_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-typep)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (cl-typep 'a '(member a b c))
      (cl-typep 'd '(member a b c))
      (cl-typep 42 '(member 1 2 42 99))
      (cl-typep 100 '(member 1 2 42 99))
      (cl-typep :kw '(member :a :b :kw)))
"##,
        expect,
    )
}

#[test]
fn div_cx286_cl_typep_integer_range() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-typep)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (cl-typep 5 '(integer 0 10))
      (cl-typep 11 '(integer 0 10))
      (cl-typep 0 '(integer 0 10))
      (cl-typep 10 '(integer 0 10))
      (cl-typep -1 '(integer 0))
      (cl-typep (expt 2 128) '(integer 0)))
"##,
        expect,
    )
}

#[test]
fn div_cx286_cl_typep_or_and_not() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-typep)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (cl-typep "x" '(or string integer))
      (cl-typep 42 '(or string integer))
      (cl-typep 'sym '(or string integer))
      (cl-typep 5 '(and integer (satisfies evenp)))
      (cl-typep 5 '(and integer (satisfies oddp)))
      (cl-typep "x" '(not integer))
      (cl-typep 42 '(not integer)))
"##,
        expect,
    )
}

#[test]
fn div_cx286_cl_typep_cons_and_vector() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (cl-typep '(1 . 2) '(cons integer integer))
          (cl-typep '(1 . "x") '(cons integer integer))
          (cl-typep '(1 2 3) '(cons integer list))
          (cl-typep [1 2 3] 'vector)
          (cl-typep (vector 1 2) '(vector integer integer)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx286_cl_typep_float_and_number() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-typep)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (cl-typep 3.14 'float)
      (cl-typep 42 'float)
      (cl-typep 42 'number)
      (cl-typep 3.14 'number)
      (cl-typep "x" 'number)
      (cl-typep 1/3 'number)
      (cl-typep (expt 2 128) 'integer)
      (cl-typep (expt 2 128) 'bignums))
"##,
        expect,
    )
}

#[test]
fn div_cx286_cl_typep_with_eieio_classes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx286-base () ())
      (defclass neo-cx286-derived (neo-cx286-base) ())
      (let ((b (make-instance 'neo-cx286-base))
            (d (make-instance 'neo-cx286-derived)))
        (list (cl-typep b 'neo-cx286-base)
              (cl-typep b 'neo-cx286-derived)
              (cl-typep d 'neo-cx286-base)
              (cl-typep d 'neo-cx286-derived)
              (cl-typep d 'standard-object))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx286_cl_typep_array_and_sequence() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-typep)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (cl-typep [1 2 3] 'array)
      (cl-typep "abc" 'array)
      (cl-typep '(1 2 3) 'array)
      (cl-typep [1 2 3] 'sequence)
      (cl-typep "abc" 'sequence)
      (cl-typep '(1 2 3) 'sequence)
      (cl-typep 42 'sequence))
"##,
        expect,
    )
}

#[test]
fn div_cx286_cl_check_type_with_message() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:no-error void-function :no-error void-function :no-error)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (condition-case e (cl-check-type 42 integer) (error :no-error))
      (condition-case e (cl-check-type "x" integer) (error (car e)))
      (condition-case e (cl-check-type 5 (integer 0 10)) (error :no-error))
      (condition-case e (cl-check-type 11 (integer 0 10)) (error (car e)))
      (condition-case e (cl-check-type "x" string) (error :no-error)))
"##,
        expect,
    )
}

#[test]
fn div_cx286_cl_typep_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-typep)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((data '("str" 42 (1 . 2) [1 2 3] nil sym 3.14 1/3)))
  (with-temp-buffer
    (buffer-enable-undo)
    (insert (format "Typep mega: %S" data))
    (put-text-property 1 6 'face 'bold)
    (let ((m (set-marker (make-marker) 10))
          (ov (make-overlay 4 18)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 25)
      (let ((state (list (mapcar (lambda (v) (type-of v)) data)
                         (mapcar (lambda (v) (cl-typep v 'number)) data)
                         (buffer-string)
                         (marker-position m)
                         (overlay-start ov) (overlay-end ov)
                         (text-properties-at 1))))
        (undo)
        (widen)
        (list state (buffer-string) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              (text-properties-at 1))))))
"##,
        expect,
    )
}
