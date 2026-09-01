//! Complex combo batch 228 — `cl-generic` deep: `cl-defgeneric` with
//! `:method-combination` variants (`+`, `list`, `max`, `min`, `and`, `or`,
//! `progn`, `concat`), `cl-defmethod` with `&context` and `&aux`.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx228_cl_generic_method_combination_concat() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'cl-generic)
      (cl-defgeneric neo-cx228-concat-combo (obj) (:method-combination concat))
      (cl-defmethod neo-cx228-concat-combo concat ((obj string)) "str-")
      (cl-defmethod neo-cx228-concat-combo concat ((obj string)) "result")
      (neo-cx228-concat-combo "hello"))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx228_cl_generic_method_combination_progn() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'cl-generic)
      (let (calls)
        (cl-defgeneric neo-cx228-progn-combo (obj) (:method-combination progn))
        (cl-defmethod neo-cx228-progn-combo progn ((obj string)) (push :first calls))
        (cl-defmethod neo-cx228-progn-combo progn ((obj string)) (push :second calls))
        (neo-cx228-progn-combo "test")
        (nreverse calls)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx228_cl_generic_method_combination_and() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'cl-generic)
      (cl-defgeneric neo-cx228-and-combo (obj) (:method-combination and))
      (cl-defmethod neo-cx228-and-combo and ((obj string)) t)
      (cl-defmethod neo-cx228-and-combo and ((obj string)) nil)
      (cl-defmethod neo-cx228-and-combo and ((obj string)) t)
      (neo-cx228-and-combo "test"))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx228_cl_generic_method_combination_or() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'cl-generic)
      (cl-defgeneric neo-cx228-or-combo (obj) (:method-combination or))
      (cl-defmethod neo-cx228-or-combo or ((obj string)) nil)
      (cl-defmethod neo-cx228-or-combo or ((obj string)) :found)
      (cl-defmethod neo-cx228-or-combo or ((obj string)) :never)
      (neo-cx228-or-combo "test"))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx228_cl_generic_with_context_specializer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'cl-generic)
      (let ((mode :debug))
        (cl-defgeneric neo-cx228-context (obj))
        (cl-defmethod neo-cx228-context (obj &context (:major-mode (eq mode :debug)))
          :debug-mode)
        (cl-defmethod neo-cx228-context (obj)
          :default)
        (list (neo-cx228-context "test"))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx228_cl_generic_dispatch_with_head_specializer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:keyword-head value) :other)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'cl-generic)
      (cl-defgeneric neo-cx228-head (obj))
      (cl-defmethod neo-cx228-head ((obj (head :keyword)))
        (list :keyword-head (cdr obj)))
      (cl-defmethod neo-cx228-head (obj)
        :other)
      (list (neo-cx228-head '(:keyword . value))
            (neo-cx228-head '(other . value))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx228_cl_generic_methods_list_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'cl-generic)
      (cl-defgeneric neo-cx228-mlist (obj))
      (cl-defmethod neo-cx228-mlist ((obj string)) :str)
      (cl-defmethod neo-cx228-mlist ((obj integer)) :int)
      (cl-defmethod neo-cx228-mlist (obj) :default)
      (let ((methods (cl-generic-methods 'neo-cx228-mlist)))
        (list (consp methods)
              (= (length methods) 3))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx228_cl_generic_next_method_p_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'cl-generic)
      (cl-defgeneric neo-cx228-next (obj))
      (cl-defmethod neo-cx228-next ((obj string))
        (if (next-method-p) (cl-call-next-method) :no-next))
      (cl-defmethod neo-cx228-next (obj) :primary)
      (list (neo-cx228-next "test")
            (neo-cx228-next 42)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx228_cl_generic_argument_precedence_order() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:both-str :a-str :b-str :neither)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'cl-generic)
      (cl-defgeneric neo-cx228-argorder (obj-a obj-b)
        (:argument-precedence-order obj-b obj-a))
      (cl-defmethod neo-cx228-argorder ((a string) (b string)) :both-str)
      (cl-defmethod neo-cx228-argorder ((a string) b) :a-str)
      (cl-defmethod neo-cx228-argorder (a (b string)) :b-str)
      (cl-defmethod neo-cx228-argorder (a b) :neither)
      (list (neo-cx228-argorder "x" "y")
            (neo-cx228-argorder "x" 42)
            (neo-cx228-argorder 42 "y")
            (neo-cx228-argorder 42 99)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx228_cl_generic_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'cl-generic)
      (cl-defgeneric neo-cx228-mega (obj))
      (cl-defmethod neo-cx228-mega ((obj string)) :str)
      (cl-defmethod neo-cx228-mega ((obj integer)) :int)
      (cl-defmethod neo-cx228-mega (obj) :default)
      (with-temp-buffer
        (buffer-enable-undo)
        (insert "cl-generic mega test buffer content")
        (put-text-property 1 6 'face 'bold)
        (let ((m (set-marker (make-marker) 8))
              (ov (make-overlay 4 14)))
          (overlay-put ov 'face 'italic)
          (overlay-put ov 'evaporate t)
          (narrow-to-region 2 18)
          (let ((state (list (neo-cx228-mega "hello")
                             (neo-cx228-mega 42)
                             (neo-cx228-mega :sym)
                             (length (cl-generic-methods 'neo-cx228-mega))
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
    );
}
