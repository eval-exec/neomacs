//! Complex combo batch 284 — `cl-generic` method dispatch with mixed
//! type specializers: string, integer, cons, vector, hash-table, null,
//! and `satisfies` predicate.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx284_cl_generic_dispatch_by_type_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:str :int :cons :vec :nil :sym :float)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'cl-generic)
      (cl-defgeneric neo-cx284-dispatch (obj))
      (cl-defmethod neo-cx284-dispatch ((obj string)) :str)
      (cl-defmethod neo-cx284-dispatch ((obj integer)) :int)
      (cl-defmethod neo-cx284-dispatch ((obj cons)) :cons)
      (cl-defmethod neo-cx284-dispatch ((obj vector)) :vec)
      (cl-defmethod neo-cx284-dispatch ((obj null)) :nil)
      (cl-defmethod neo-cx284-dispatch ((obj symbol)) :sym)
      (cl-defmethod neo-cx284-dispatch ((obj float)) :float)
      (cl-defmethod neo-cx284-dispatch ((obj hash-table)) :hash)
      (cl-defmethod neo-cx284-dispatch (obj) :default)
      (mapcar #'neo-cx284-dispatch
              '("str" 42 (1 . 2) [1 2] nil sym 3.14)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx284_cl_generic_with_satisfies_predicate() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'cl-generic)
      (cl-defgeneric neo-cx284-sat (obj))
      (cl-defmethod neo-cx284-sat ((obj (satisfies evenp))) :even)
      (cl-defmethod neo-cx284-sat ((obj (satisfies oddp))) :odd)
      (cl-defmethod neo-cx284-sat (obj) :other)
      (mapcar #'neo-cx284-sat '(2 3 4 5 "str" nil)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx284_cl_generic_with_head_specializer_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((:keyword v) (:command x) (:option y) :other :other)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'cl-generic)
      (cl-defgeneric neo-cx284-head (obj))
      (cl-defmethod neo-cx284-head ((obj (head :kw))) (list :keyword (cdr obj)))
      (cl-defmethod neo-cx284-head ((obj (head :cmd))) (list :command (cdr obj)))
      (cl-defmethod neo-cx284-head ((obj (head :opt))) (list :option (cdr obj)))
      (cl-defmethod neo-cx284-head (obj) :other)
      (mapcar #'neo-cx284-head '((:kw . v) (:cmd . x) (:opt . y) (other . z) nil)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx284_cl_generic_two_arg_dispatch_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:both-str :str-int :int-str :both-int :default)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'cl-generic)
      (cl-defgeneric neo-cx284-bi (a b))
      (cl-defmethod neo-cx284-bi ((a string) (b string)) :both-str)
      (cl-defmethod neo-cx284-bi ((a string) (b integer)) :str-int)
      (cl-defmethod neo-cx284-bi ((a integer) (b string)) :int-str)
      (cl-defmethod neo-cx284-bi ((a integer) (b integer)) :both-int)
      (cl-defmethod neo-cx284-bi (a b) :default)
      (list (neo-cx284-bi "x" "y")
            (neo-cx284-bi "x" 1)
            (neo-cx284-bi 1 "y")
            (neo-cx284-bi 1 2)
            (neo-cx284-bi 'sym :kw)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx284_cl_generic_argument_precedence_order_complex() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:a-str :b-str :c-str :c-str)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'cl-generic)
      (cl-defgeneric neo-cx284-po (a b c)
        (:argument-precedence-order c b a))
      (cl-defmethod neo-cx284-po ((a string) b c) :a-str)
      (cl-defmethod neo-cx284-po (a (b string) c) :b-str)
      (cl-defmethod neo-cx284-po (a b (c string)) :c-str)
      (cl-defmethod neo-cx284-po (a b c) :none)
      (list (neo-cx284-po "x" 1 2)
            (neo-cx284-po 1 "y" 2)
            (neo-cx284-po 1 2 "z")
            (neo-cx284-po "x" "y" "z")))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx284_cl_generic_call_next_method_through_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'cl-generic)
      (cl-defgeneric neo-cx284-next (obj))
      (cl-defmethod neo-cx284-next (obj)
        (if (next-method-p) (cons :default (cl-call-next-method)) :default))
      (list (neo-cx284-next "str")
            (neo-cx284-next 42)
            (neo-cx284-next :sym)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx284_cl_generic_method_combination_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'cl-generic)
      (cl-defgeneric neo-cx284-listc (obj) (:method-combination list))
      (cl-defmethod neo-cx284-listc list ((obj string)) :s1)
      (cl-defmethod neo-cx284-listc list ((obj string)) :s2)
      (cl-defmethod neo-cx284-listc list ((obj string)) :s3)
      (neo-cx284-listc "test"))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx284_cl_generic_with_keyword_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"test\" :a nil) (\"test\" :b t))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'cl-generic)
      (cl-defgeneric neo-cx284-kw (obj &key mode verbose))
      (cl-defmethod neo-cx284-kw ((obj string) &key mode verbose)
        (list obj mode verbose))
      (list (neo-cx284-kw "test" :mode :a)
            (neo-cx284-kw "test" :mode :b :verbose t)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx284_cl_generic_no_applicable_method_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK :caught-no-applicable""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'cl-generic)
      (cl-defgeneric neo-cx284-noapp (obj))
      (let ((inst (list :opaque)))
        (condition-case err
            (neo-cx284-noapp inst)
          (cl-no-applicable-method :caught-no-applicable)
          (error (list :caught-error (car err))))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx284_cl_generic_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'cl-generic)
      (cl-defgeneric neo-cx284-mega (obj))
      (cl-defmethod neo-cx284-mega ((obj string)) :str-result)
      (cl-defmethod neo-cx284-mega ((obj integer)) :int-result)
      (cl-defmethod neo-cx284-mega (obj) :default)
      (with-temp-buffer
        (buffer-enable-undo)
        (insert "cl-generic dispatch mega test buffer content")
        (put-text-property 1 6 'face 'bold)
        (let ((m (set-marker (make-marker) 8))
              (ov (make-overlay 4 14)))
          (overlay-put ov 'face 'italic)
          (overlay-put ov 'evaporate t)
          (narrow-to-region 2 18)
          (let ((state (list (neo-cx284-mega "hello")
                             (neo-cx284-mega 42)
                             (neo-cx284-mega :sym)
                             (length (cl-generic-methods 'neo-cx284-mega))
                             (buffer-string)
                             (marker-position m)
                             (overlay-start ov) (overlay-end ov)
                             (text-properties-at 1))))
            (undo)
            (widen)
            (list state (buffer-string) (marker-position m)
                  (overlay-start ov) (overlay-end ov)
                  (text-properties-at 1))))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}
