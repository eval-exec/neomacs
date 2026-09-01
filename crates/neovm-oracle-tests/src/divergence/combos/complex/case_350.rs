//! Complex combo batch 350 — `cl-generic` ultimate: dispatch by type matrix
//! (string/integer/cons/vector/null/symbol/float/hash-table), head specializer,
//! satisfies predicate, context major-mode, argument-precedence-order, all
//! method combinations, call-next-method, no-applicable-method, keyword args.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx350_cl_generic_dispatch_by_type_full_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:str :int :cons :vec :nil :sym :float)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'cl-generic)
      (cl-defgeneric neo-cx350-dispatch (obj))
      (cl-defmethod neo-cx350-dispatch ((obj string)) :str)
      (cl-defmethod neo-cx350-dispatch ((obj integer)) :int)
      (cl-defmethod neo-cx350-dispatch ((obj cons)) :cons)
      (cl-defmethod neo-cx350-dispatch ((obj vector)) :vec)
      (cl-defmethod neo-cx350-dispatch ((obj null)) :nil)
      (cl-defmethod neo-cx350-dispatch ((obj symbol)) :sym)
      (cl-defmethod neo-cx350-dispatch ((obj float)) :float)
      (cl-defmethod neo-cx350-dispatch ((obj hash-table)) :hash)
      (cl-defmethod neo-cx350-dispatch (obj) :default)
      (mapcar #'neo-cx350-dispatch
              '("str" 42 (1 . 2) [1 2] nil sym 3.14)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx350_cl_generic_satisfies_and_head_specializer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'cl-generic)
      (cl-defgeneric neo-cx350-sat (obj))
      (cl-defmethod neo-cx350-sat ((obj (satisfies evenp))) :even)
      (cl-defmethod neo-cx350-sat ((obj (satisfies oddp))) :odd)
      (cl-defmethod neo-cx350-sat (obj) :other)
      (cl-defgeneric neo-cx350-head (obj))
      (cl-defmethod neo-cx350-head ((obj (head :kw))) (list :keyword (cdr obj)))
      (cl-defmethod neo-cx350-head ((obj (head :cmd))) (list :command (cdr obj)))
      (cl-defmethod neo-cx350-head (obj) :other)
      (list (mapcar #'neo-cx350-sat '(2 3 4 5 "str" nil))
            (mapcar #'neo-cx350-head '((:kw . v) (:cmd . x) (other . z) nil))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx350_cl_generic_argument_precedence_order() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:a-str :b-str :c-str :c-str)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'cl-generic)
      (cl-defgeneric neo-cx350-po (a b c)
        (:argument-precedence-order c b a))
      (cl-defmethod neo-cx350-po ((a string) b c) :a-str)
      (cl-defmethod neo-cx350-po (a (b string) c) :b-str)
      (cl-defmethod neo-cx350-po (a b (c string)) :c-str)
      (cl-defmethod neo-cx350-po (a b c) :none)
      (list (neo-cx350-po "x" 1 2)
            (neo-cx350-po 1 "y" 2)
            (neo-cx350-po 1 2 "z")
            (neo-cx350-po "x" "y" "z")))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx350_cl_generic_method_combination_all_variants() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'cl-generic)
      (cl-defgeneric neo-cx350-plus (obj) (:method-combination +))
      (cl-defmethod neo-cx350-plus + (obj) 10)
      (cl-defmethod neo-cx350-plus + (obj) 20)
      (cl-defgeneric neo-cx350-maxg (obj) (:method-combination max))
      (cl-defmethod neo-cx350-maxg max (obj) 10)
      (cl-defmethod neo-cx350-maxg max (obj) 50)
      (cl-defmethod neo-cx350-maxg max (obj) 25)
      (list (neo-cx350-plus "test") (neo-cx350-maxg "test")))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx350_cl_generic_call_next_method_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'cl-generic)
      (cl-defgeneric neo-cx350-next (obj))
      (cl-defmethod neo-cx350-next (obj)
        (if (next-method-p) (cons :default (cl-call-next-method)) :default))
      (cl-defmethod neo-cx350-next ((obj string))
        (if (next-method-p) (cons :str (cl-call-next-method)) :str))
      (list (neo-cx350-next "test") (neo-cx350-next 42) (neo-cx350-next :sym)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx350_cl_generic_no_applicable_method_handling() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK :caught-no-applicable""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'cl-generic)
      (cl-defgeneric neo-cx350-noapp (obj))
      (condition-case err
          (neo-cx350-noapp (list :opaque))
        (cl-no-applicable-method :caught-no-applicable)
        (error (list :caught-error (car err)))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx350_cl_generic_keyword_and_optional_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"test\" nil nil) (\"test\" :opt-val nil) (\"test\" :opt-val :m))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'cl-generic)
      (cl-defgeneric neo-cx350-kw (obj &optional opt &key mode))
      (cl-defmethod neo-cx350-kw (obj &optional opt &key mode)
        (list obj opt mode))
      (list (neo-cx350-kw "test")
            (neo-cx350-kw "test" :opt-val)
            (neo-cx350-kw "test" :opt-val :mode :m)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx350_cl_generic_methods_list_and_find_method() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'cl-generic)
      (cl-defgeneric neo-cx350-mlist (obj))
      (cl-defmethod neo-cx350-mlist ((obj string)) :str)
      (cl-defmethod neo-cx350-mlist ((obj integer)) :int)
      (cl-defmethod neo-cx350-mlist (obj) :default)
      (let ((methods (cl-generic-methods 'neo-cx350-mlist)))
        (list (consp methods) (= (length methods) 3))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx350_cl_generic_context_specializer_major_mode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:elisp-mode :text-mode :default)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'cl-generic)
      (cl-defgeneric neo-cx350-context (obj))
      (cl-defmethod neo-cx350-context (obj &context (major-mode emacs-lisp-mode))
        :elisp-mode)
      (cl-defmethod neo-cx350-context (obj &context (major-mode text-mode))
        :text-mode)
      (cl-defmethod neo-cx350-context (obj) :default)
      (list (with-temp-buffer (emacs-lisp-mode) (neo-cx350-context "test"))
            (with-temp-buffer (text-mode) (neo-cx350-context "test"))
            (with-temp-buffer (neo-cx350-context "test"))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx350_cl_generic_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'cl-generic)
      (cl-defgeneric neo-cx350-mega (obj))
      (cl-defmethod neo-cx350-mega ((obj string)) :str-result)
      (cl-defmethod neo-cx350-mega ((obj integer)) :int-result)
      (cl-defmethod neo-cx350-mega (obj) :default)
      (with-temp-buffer
        (buffer-enable-undo)
        (insert "cl-generic ultimate mega test buffer content")
        (put-text-property 1 6 'face 'bold)
        (let ((m (set-marker (make-marker) 8))
              (ov (make-overlay 4 14)))
          (overlay-put ov 'face 'italic)
          (overlay-put ov 'evaporate t)
          (narrow-to-region 2 18)
          (let ((state (list (neo-cx350-mega "hello")
                             (neo-cx350-mega 42)
                             (neo-cx350-mega :sym)
                             (length (cl-generic-methods 'neo-cx350-mega))
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
