//! Complex combo batch 278 — `eieio` static/method combination with
//! `:around` overriding `:before`/`:after` chain; `cl-call-next-method`
//! through 4-level inheritance; `cl-no-primary-method` handling.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx278_eieio_4_level_inheritance_call_next_method() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx278-l1 () ())
      (defclass neo-cx278-l2 (neo-cx278-l1) ())
      (defclass neo-cx278-l3 (neo-cx278-l2) ())
      (defclass neo-cx278-l4 (neo-cx278-l3) ())
      (cl-defgeneric neo-cx278-chain (obj))
      (cl-defmethod neo-cx278-chain ((obj neo-cx278-l1))
        (if (next-method-p) (cons :l1 (cl-call-next-method)) :l1))
      (cl-defmethod neo-cx278-chain ((obj neo-cx278-l2))
        (if (next-method-p) (cons :l2 (cl-call-next-method)) :l2))
      (cl-defmethod neo-cx278-chain ((obj neo-cx278-l3))
        (if (next-method-p) (cons :l3 (cl-call-next-method)) :l3))
      (cl-defmethod neo-cx278-chain ((obj neo-cx278-l4))
        (if (next-method-p) (cons :l4 (cl-call-next-method)) :l4))
      (list (neo-cx278-chain (make-instance 'neo-cx278-l4))
            (neo-cx278-chain (make-instance 'neo-cx278-l3))
            (neo-cx278-chain (make-instance 'neo-cx278-l1))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx278_eieio_around_override_before_after() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:result (:around-enter :before :primary :after :around-exit))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx278-aro () ())
      (let (calls)
        (cl-defgeneric neo-cx278-aro-call (obj))
        (cl-defmethod neo-cx278-aro-call :around ((obj neo-cx278-aro))
          (push :around-enter calls)
          (let ((r (cl-call-next-method)))
            (push :around-exit calls)
            r))
        (cl-defmethod neo-cx278-aro-call :before ((obj neo-cx278-aro))
          (push :before calls))
        (cl-defmethod neo-cx278-aro-call ((obj neo-cx278-aro))
          (push :primary calls)
          :result)
        (cl-defmethod neo-cx278-aro-call :after ((obj neo-cx278-aro))
          (push :after calls))
        (let ((r (neo-cx278-aro-call (make-instance 'neo-cx278-aro))))
          (list r (nreverse calls)))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx278_eieio_method_combination_max_and_min() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx278-mm () ())
      (cl-defgeneric neo-cx278-maxc (obj) (:method-combination max))
      (cl-defmethod neo-cx278-maxc max ((obj neo-cx278-mm)) 10)
      (cl-defmethod neo-cx278-maxc max ((obj neo-cx278-mm)) 50)
      (cl-defmethod neo-cx278-maxc max ((obj neo-cx278-mm)) 25)
      (cl-defgeneric neo-cx278-minc (obj) (:method-combination min))
      (cl-defmethod neo-cx278-minc min ((obj neo-cx278-mm)) 10)
      (cl-defmethod neo-cx278-minc min ((obj neo-cx278-mm)) 50)
      (cl-defmethod neo-cx278-minc min ((obj neo-cx278-mm)) 25)
      (list (neo-cx278-maxc (make-instance 'neo-cx278-mm))
            (neo-cx278-minc (make-instance 'neo-cx278-mm))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx278_eieio_cl_no_primary_method_handling() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:caught-no-primary)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx278-np () ())
      (cl-defgeneric neo-cx278-np-call (obj))
      (cl-defmethod neo-cx278-np-call :before ((obj neo-cx278-np)) :before-only)
      (let ((inst (make-instance 'neo-cx278-np)))
        (condition-case err
            (neo-cx278-np-call inst)
          (cl-no-primary-method (list :caught-no-primary))
          (no-method (list :caught-no-method))
          (error (list :caught-other (car err))))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx278_eieio_eql_specializer_with_integer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:the-answer :the-keyword :default :default)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (cl-defgeneric neo-cx278-eqli (obj))
      (cl-defmethod neo-cx278-eqli ((obj (eql 42))) :the-answer)
      (cl-defmethod neo-cx278-eqli ((obj (eql :keyword))) :the-keyword)
      (cl-defmethod neo-cx278-eqli (obj) :default)
      (list (neo-cx278-eqli 42)
            (neo-cx278-eqli :keyword)
            (neo-cx278-eqli "other")
            (neo-cx278-eqli 99)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx278_eieio_with_slots_access_and_mutation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable e)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx278-ws ()
        ((x :initarg :x :initform 0 :reader neo-cx278-get-x)
         (y :initarg :y :initform 0 :accessor neo-cx278-y))))
      (let ((inst (make-instance 'neo-cx278-ws :x 1 :y 2)))
        (list (neo-cx278-get-x inst)
              (neo-cx278-y inst)
              (setf (neo-cx278-y inst) 99)
              (neo-cx278-y inst)
              (slot-value inst 'x))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx278_eieio_class_allocated_slot_shared() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (5 5 1 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx278-cls ()
        ((counter :allocation :class :initform 0)
         (id :allocation :instance :initarg :id)))
      (let ((a (make-instance 'neo-cx278-cls :id 1))
            (b (make-instance 'neo-cx278-cls :id 2)))
        (oset a counter 5)
        (list (slot-value a 'counter)
              (slot-value b 'counter)
              (slot-value a 'id)
              (slot-value b 'id))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx278_eieio_initialize_instance_after_method() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK :init-ran""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx278-init ()
        ((computed :initform nil)))
      (cl-defmethod initialize-instance :after ((o neo-cx278-init) &rest _)
        (oset o computed :init-ran))
      (let ((inst (make-instance 'neo-cx278-init)))
        (slot-value inst 'computed)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx278_eieio_print_object_override() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx278-po ()
        ((name :initarg :name :initform "anon")))
      (cl-defmethod cl-print-object ((o neo-cx278-po) stream)
        (princ (format "#<PO:%s>" (slot-value o 'name)) stream)
        o)
      (let ((inst (make-instance 'neo-cx278-po :name "alpha")))
        (list (let ((print-circle t)) (prin1-to-string inst))
              (princ-to-string inst)
              (format "%S" inst)
              (format "%s" inst))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx278_eieio_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored args-out-of-range)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx278-mega ()
        ((value :initarg :value :initform 0)))
      (cl-defgeneric neo-cx278-mega-call (obj mult))
      (cl-defmethod neo-cx278-mega-call :before ((obj neo-cx278-mega) m)
        (oset obj value (+ (slot-value obj 'value) m)))
      (cl-defmethod neo-cx278-mega-call :after ((obj neo-cx278-mega) m)
        (oset obj value (* (slot-value obj 'value) m)))
      (cl-defmethod neo-cx278-mega-call ((obj neo-cx278-mega) m)
        (oset obj value (+ (slot-value obj 'value) m))
        (slot-value obj 'value))
      (let ((inst (make-instance 'neo-cx278-mega :value 1)))
        (with-temp-buffer
          (buffer-enable-undo)
          (insert "EIEIO mega dispatch test buffer content")
          (put-text-property 1 6 'face 'bold)
          (let ((m (set-marker (make-marker) 8))
                (ov (make-overlay 4 14)))
            (overlay-put ov 'face 'italic)
            (overlay-put ov 'evaporate t)
            (narrow-to-region 2 18)
            (let ((r (neo-cx278-mega-call inst 5)))
              (let ((state (list r (slot-value inst 'value)
                                 (buffer-string)
                                 (marker-position m)
                                 (overlay-start ov) (overlay-end ov)
                                 (text-properties-at 1))))
                (undo)
                (widen)
                (list state (buffer-string) (marker-position m)
                      (overlay-start ov) (overlay-end ov)
                      (text-properties-at 1))))))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}
