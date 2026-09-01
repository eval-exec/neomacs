//! Complex combo batch 384 — `cl-generic` ultimate: dispatch by type, head
//! specializer, satisfies, context major-mode, argument-precedence-order,
//! method combinations (+/list/max/min/and/or/nconc/append), call-next-method.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx384_cl_generic_dispatch_by_type_full_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:str :int :cons :vec :nil :sym :float)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'cl-generic)
      (cl-defgeneric neo-cx384-dispatch (obj))
      (cl-defmethod neo-cx384-dispatch ((obj string)) :str)
      (cl-defmethod neo-cx384-dispatch ((obj integer)) :int)
      (cl-defmethod neo-cx384-dispatch ((obj cons)) :cons)
      (cl-defmethod neo-cx384-dispatch ((obj vector)) :vec)
      (cl-defmethod neo-cx384-dispatch ((obj null)) :nil)
      (cl-defmethod neo-cx384-dispatch ((obj symbol)) :sym)
      (cl-defmethod neo-cx384-dispatch ((obj float)) :float)
      (cl-defmethod neo-cx384-dispatch ((obj hash-table)) :hash)
      (cl-defmethod neo-cx384-dispatch (obj) :default)
      (mapcar #'neo-cx384-dispatch
              '("str" 42 (1 . 2) [1 2] nil sym 3.14)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx384_cl_generic_satisfies_head_eql() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'cl-generic)
      (cl-defgeneric neo-cx384-special (obj))
      (cl-defmethod neo-cx384-special ((obj (satisfies evenp))) :even)
      (cl-defmethod neo-cx384-special ((obj (satisfies oddp))) :odd)
      (cl-defmethod neo-cx384-special ((obj (eql :kw))) :keyword)
      (cl-defmethod neo-cx384-special ((obj (head :cmd))) (list :command (cdr obj)))
      (cl-defmethod neo-cx384-special (obj) :other)
      (list (mapcar #'neo-cx384-special '(2 3 4 5 "str"))
            (neo-cx384-special :kw)
            (neo-cx384-special '(:cmd . run))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx384_cl_generic_argument_precedence_order() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:a-str :b-str :c-str :c-str)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'cl-generic)
      (cl-defgeneric neo-cx384-po (a b c)
        (:argument-precedence-order c b a))
      (cl-defmethod neo-cx384-po ((a string) b c) :a-str)
      (cl-defmethod neo-cx384-po (a (b string) c) :b-str)
      (cl-defmethod neo-cx384-po (a b (c string)) :c-str)
      (cl-defmethod neo-cx384-po (a b c) :none)
      (list (neo-cx384-po "x" 1 2)
            (neo-cx384-po 1 "y" 2)
            (neo-cx384-po 1 2 "z")
            (neo-cx384-po "x" "y" "z")))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx384_cl_generic_method_combinations_all() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'cl-generic)
      (cl-defgeneric neo-cx384-plus (obj) (:method-combination +))
      (cl-defmethod neo-cx384-plus + (obj) 10)
      (cl-defmethod neo-cx384-plus + (obj) 20)
      (cl-defgeneric neo-cx384-maxg (obj) (:method-combination max))
      (cl-defmethod neo-cx384-maxg max (obj) 10)
      (cl-defmethod neo-cx384-maxg max (obj) 50)
      (cl-defgeneric neo-cx384-listc (obj) (:method-combination list))
      (cl-defmethod neo-cx384-listc list (obj) :a)
      (cl-defmethod neo-cx384-listc list (obj) :b)
      (list (neo-cx384-plus "t") (neo-cx384-maxg "t") (neo-cx384-listc "t")))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx384_cl_generic_call_next_method_deep_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'cl-generic)
      (cl-defgeneric neo-cx384-next (obj))
      (cl-defmethod neo-cx384-next (obj)
        (if (next-method-p) (cons :default (cl-call-next-method)) :default))
      (cl-defmethod neo-cx384-next ((obj string))
        (if (next-method-p) (cons :str (cl-call-next-method)) :str))
      (list (neo-cx384-next "test")
            (neo-cx384-next 42)
            (neo-cx384-next :sym)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx384_cl_generic_context_major_mode_specializer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:elisp :text :default)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'cl-generic)
      (cl-defgeneric neo-cx384-context (obj))
      (cl-defmethod neo-cx384-context (obj &context (major-mode emacs-lisp-mode))
        :elisp)
      (cl-defmethod neo-cx384-context (obj &context (major-mode text-mode))
        :text)
      (cl-defmethod neo-cx384-context (obj) :default)
      (list (with-temp-buffer (emacs-lisp-mode) (neo-cx384-context "t"))
            (with-temp-buffer (text-mode) (neo-cx384-context "t"))
            (with-temp-buffer (neo-cx384-context "t"))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx384_cl_generic_no_applicable_and_no_primary() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:caught-no-applicable :caught-no-applicable)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'cl-generic)
      (cl-defgeneric neo-cx384-na (obj))
      (list (condition-case err
                (neo-cx384-na "test")
              (cl-no-applicable-method :caught-no-applicable)
              (error (list :err (car err))))
            (condition-case err
                (neo-cx384-na 42)
              (cl-no-applicable-method :caught-no-applicable)
              (error (list :err (car err))))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx384_cl_generic_keyword_and_optional_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"test\" nil nil) (\"test\" :opt-val nil) (\"test\" :opt-val :m))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'cl-generic)
      (cl-defgeneric neo-cx384-kw (obj &optional opt &key mode))
      (cl-defmethod neo-cx384-kw (obj &optional opt &key mode)
        (list obj opt mode))
      (list (neo-cx384-kw "test")
            (neo-cx384-kw "test" :opt-val)
            (neo-cx384-kw "test" :opt-val :mode :m)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx384_cl_generic_two_arg_dispatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:both-str :str-int :int-str :both-int)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'cl-generic)
      (cl-defgeneric neo-cx384-bi (a b))
      (cl-defmethod neo-cx384-bi ((a string) (b string)) :both-str)
      (cl-defmethod neo-cx384-bi ((a string) (b integer)) :str-int)
      (cl-defmethod neo-cx384-bi ((a integer) (b string)) :int-str)
      (cl-defmethod neo-cx384-bi ((a integer) (b integer)) :both-int)
      (cl-defmethod neo-cx384-bi (a b) :default)
      (list (neo-cx384-bi "x" "y")
            (neo-cx384-bi "x" 1)
            (neo-cx384-bi 1 "y")
            (neo-cx384-bi 1 2)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx384_cl_generic_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'cl-generic)
      (cl-defgeneric neo-cx384-mega (obj))
      (cl-defmethod neo-cx384-mega ((obj string)) :str-result)
      (cl-defmethod neo-cx384-mega ((obj integer)) :int-result)
      (cl-defmethod neo-cx384-mega (obj) :default)
      (with-temp-buffer
        (buffer-enable-undo)
        (insert "cl-generic ultimate mega test buffer content")
        (put-text-property 1 6 'face 'bold)
        (let ((m (set-marker (make-marker) 8))
              (ov (make-overlay 4 14)))
          (overlay-put ov 'face 'italic)
          (overlay-put ov 'evaporate t)
          (narrow-to-region 2 18)
          (let ((state (list (neo-cx384-mega "hello")
                             (neo-cx384-mega 42)
                             (neo-cx384-mega :sym)
                             (length (cl-generic-methods 'neo-cx384-mega))
                             (buffer-string)
                             (marker-position m)
                             (overlay-start ov) (overlay-end ov)
                             (text-properties-at 1))))
            (undo)
            (widen()
            (list state (buffer-string) (marker-position m)
                  (overlay-start ov) (overlay-end ov)
                  (text-properties-at 1))))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}
