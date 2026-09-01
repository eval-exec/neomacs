//! Complex combo batch 331 — `eieio` ultimate: 4-level inheritance
//! call-next-method chain, method combination +/list/max/min, EQL
//! specializer, class-allocated slots, print-object, initialize-instance.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx331_eieio_4_level_inheritance_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx331-l1 () ())
      (defclass neo-cx331-l2 (neo-cx331-l1) ())
      (defclass neo-cx331-l3 (neo-cx331-l2) ())
      (defclass neo-cx331-l4 (neo-cx331-l3) ())
      (cl-defgeneric neo-cx331-chain (obj))
      (cl-defmethod neo-cx331-chain ((obj neo-cx331-l1))
        (if (next-method-p) (cons :l1 (cl-call-next-method)) :l1))
      (cl-defmethod neo-cx331-chain ((obj neo-cx331-l2))
        (if (next-method-p) (cons :l2 (cl-call-next-method)) :l2))
      (cl-defmethod neo-cx331-chain ((obj neo-cx331-l3))
        (if (next-method-p) (cons :l3 (cl-call-next-method)) :l3))
      (cl-defmethod neo-cx331-chain ((obj neo-cx331-l4))
        (if (next-method-p) (cons :l4 (cl-call-next-method)) :l4))
      (list (neo-cx331-chain (make-instance 'neo-cx331-l4))
            (neo-cx331-chain (make-instance 'neo-cx331-l3))
            (neo-cx331-chain (make-instance 'neo-cx331-l1))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx331_eieio_method_combination_plus() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx331-mc () ())
      (cl-defgeneric neo-cx331-mc-call (obj) (:method-combination +))
      (cl-defmethod neo-cx331-mc-call + ((obj neo-cx331-mc)) 10)
      (cl-defmethod neo-cx331-mc-call + ((obj neo-cx331-mc)) 20)
      (cl-defmethod neo-cx331-mc-call + ((obj neo-cx331-mc)) 30)
      (neo-cx331-mc-call (make-instance 'neo-cx331-mc)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx331_eieio_eql_specializer_dispatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:eql-special :class :default)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx331-eq () ())
      (cl-defgeneric neo-cx331-eq-call (obj))
      (cl-defmethod neo-cx331-eq-call ((obj (eql :special))) :eql-special)
      (cl-defmethod neo-cx331-eq-call ((obj neo-cx331-eq)) :class)
      (cl-defmethod neo-cx331-eq-call (obj) :default)
      (list (neo-cx331-eq-call :special)
            (neo-cx331-eq-call (make-instance 'neo-cx331-eq))
            (neo-cx331-eq-call "other")))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx331_eieio_class_allocated_slots_shared() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (5 5 10 10 1 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx331-static ()
        ((counter :allocation :class :initform 0)
         (id :allocation :instance :initarg :id)))
      (let ((a (make-instance 'neo-cx331-static :id 1))
            (b (make-instance 'neo-cx331-static :id 2)))
        (oset a counter 5)
        (list (slot-value a 'counter)
              (slot-value b 'counter)
              (oset b counter 10)
              (slot-value a 'counter)
              (slot-value a 'id)
              (slot-value b 'id))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx331_eieio_print_object_override() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx331-po ()
        ((name :initarg :name :initform "anon")))
      (cl-defmethod cl-print-object ((o neo-cx331-po) stream)
        (princ (format "#<PO:%s>" (slot-value o 'name)) stream)
        o)
      (let ((inst (make-instance 'neo-cx331-po :name "alpha")))
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
fn div_cx331_eieio_initialize_instance_after() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK :init-ran""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx331-init ()
        ((computed :initform nil)))
      (cl-defmethod initialize-instance :after ((o neo-cx331-init) &rest _)
        (oset o computed :init-ran))
      (let ((inst (make-instance 'neo-cx331-init)))
        (slot-value inst 'computed)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx331_eieio_slot_boundp_and_makunbound() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t 0 unbound eieio--unbound nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx331-sb ()
        ((x :initarg :x :initform unbound)
         (y :initarg :y :initform 0)))
      (let ((inst (make-instance 'neo-cx331-sb)))
        (list (slot-boundp inst 'x)
              (slot-boundp inst 'y)
              (slot-value inst 'y)
              (condition-case err (slot-value inst 'x) (error (car err)))
              (slot-makeunbound inst 'y)
              (slot-boundp inst 'y))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx331_eieio_with_slots_and_accessors() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable e)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx331-ws ()
        ((x :initarg :x :initform 0 :reader neo-cx331-get-x)
         (y :initarg :y :initform 0 :accessor neo-cx331-y)))
      (let ((inst (make-instance 'neo-cx331-ws :x 1 :y 2)))
        (with-slots (x y) inst
          (list x y (cl-incf x 10) (cl-incf y 20) x y))
          (list (neo-cx331-get-x inst)
                (neo-cx331-y inst)
                (setf (neo-cx331-y inst) 99)
                (neo-cx331-y inst)))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx331_eieio_class_parents_children_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((#s(eieio--class neo-cx331-mid nil (#s(eieio--class neo-cx331-root nil (#s(eieio--class eieio-default-superclass \"Default parent class for classes with no specified parent class.\\nIts slots are automatically adopted by classes with no specified parents.\" (#s(built-in-class record \"Abstract type of objects with slots.\" (#s(built-in-class atom \"Abstract supertype of anything but cons cells.\" (#s(built-in-class t \"Abstract supertype of everything.\" nil nil nil nil)) nil nil nil)) nil nil nil)) [] #s(hash-table test eq) (neo-cx331-root) nil [] [] #s(#6) (:custom-groups nil :documentation \"Default parent class for classes with no specified parent class.\\nIts slots are automatically adopted by classes with no specified parents.\" :abstract t))) [] #s(hash-table test eq) (neo-cx331-mid) nil [] [] #s(#4) (:custom-groups nil))) [] #s(hash-table test eq) (neo-cx331-leaf) nil [] [] #s(#2) (:custom-groups nil))) (#s(eieio--class neo-cx331-root nil (#s(eieio--class eieio-default-superclass \"Default parent class for classes with no specified parent class.\\nIts slots are automatically adopted by classes with no specified parents.\" (#s(built-in-class record \"Abstract type of objects with slots.\" (#s(built-in-class atom \"Abstract supertype of anything but cons cells.\" (#s(built-in-class t \"Abstract supertype of everything.\" nil nil nil nil)) nil nil nil)) nil nil nil)) [] #s(hash-table test eq) (neo-cx331-root) nil [] [] #s(#4) (:custom-groups nil :documentation \"Default parent class for classes with no specified parent class.\\nIts slots are automatically adopted by classes with no specified parents.\" :abstract t))) [] #s(hash-table test eq) (neo-cx331-mid) nil [] [] #s(#2) (:custom-groups nil))) nil (neo-cx331-mid))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx331-root () ())
      (defclass neo-cx331-mid (neo-cx331-root) ())
      (defclass neo-cx331-leaf (neo-cx331-mid) ())
      (list (eieio-class-parents 'neo-cx331-leaf)
            (eieio-class-parents 'neo-cx331-mid)
            (eieio-class-parents 'neo-cx331-root)
            (eieio-class-children 'neo-cx331-root)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx331_eieio_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored args-out-of-range)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx331-mega ()
        ((value :initarg :value :initform 0)))
      (cl-defgeneric neo-cx331-mega-call (obj mult))
      (cl-defmethod neo-cx331-mega-call :before ((obj neo-cx331-mega) m)
        (oset obj value (+ (slot-value obj 'value) m)))
      (cl-defmethod neo-cx331-mega-call :after ((obj neo-cx331-mega) m)
        (oset obj value (* (slot-value obj 'value) m)))
      (cl-defmethod neo-cx331-mega-call ((obj neo-cx331-mega) m)
        (oset obj value (+ (slot-value obj 'value) m))
        (slot-value obj 'value))
      (let ((inst (make-instance 'neo-cx331-mega :value 1)))
        (with-temp-buffer
          (buffer-enable-undo)
          (insert "EIEIO mega ultimate test buffer content")
          (put-text-property 1 6 'face 'bold)
          (let ((m (set-marker (make-marker) 8))
                (ov (make-overlay 4 14)))
            (overlay-put ov 'face 'italic)
            (overlay-put ov 'evaporate t)
            (narrow-to-region 2 18)
            (let ((r (neo-cx331-mega-call inst 5)))
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
