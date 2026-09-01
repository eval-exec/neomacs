//! Complex combo batch 386 — `eieio` ultimate: multiple inheritance, 4-level
//! call-next-method, method combinations +/max/min/and/or, EQL specializer,
//! class-allocated slots, print-object, initialize-instance, slot-boundp,
//! with-slots/with-accessors, change-class, no-primary/no-applicable.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx386_eieio_multiple_inheritance_and_dispatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (11 22 :a neo-cx386-c)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx386-a () ((x :initarg :x :initform 1)))
      (defclass neo-cx386-b () ((y :initarg :y :initform 2)))
      (defclass neo-cx386-c (neo-cx386-a neo-cx386-b) ())
      (cl-defmethod neo-cx386-who ((o neo-cx386-a)) :a)
      (cl-defmethod neo-cx386-who ((o neo-cx386-b)) :b)
      (let ((inst (neo-cx386-c :x 11 :y 22)))
        (list (slot-value inst 'x) (slot-value inst 'y)
              (neo-cx386-who inst) (class-of inst))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx386_eieio_4_level_call_next_method() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx386-l1 () ())
      (defclass neo-cx386-l2 (neo-cx386-l1) ())
      (defclass neo-cx386-l3 (neo-cx386-l2) ())
      (defclass neo-cx386-l4 (neo-cx386-l3) ())
      (cl-defgeneric neo-cx386-chain (obj))
      (cl-defmethod neo-cx386-chain ((o neo-cx386-l1))
        (if (next-method-p) (cons :l1 (cl-call-next-method)) :l1))
      (cl-defmethod neo-cx386-chain ((o neo-cx386-l2))
        (if (next-method-p) (cons :l2 (cl-call-next-method)) :l2))
      (cl-defmethod neo-cx386-chain ((o neo-cx386-l3))
        (if (next-method-p) (cons :l3 (cl-call-next-method)) :l3))
      (cl-defmethod neo-cx386-chain ((o neo-cx386-l4))
        (if (next-method-p) (cons :l4 (cl-call-next-method)) :l4))
      (list (neo-cx386-chain (make-instance 'neo-cx386-l4))
            (neo-cx386-chain (make-instance 'neo-cx386-l3))
            (neo-cx386-chain (make-instance 'neo-cx386-l1))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx386_eieio_method_combination_plus_max_min() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx386-mc () ())
      (cl-defgeneric neo-cx386-plus (obj) (:method-combination +))
      (cl-defmethod neo-cx386-plus + ((o neo-cx386-mc)) 10)
      (cl-defmethod neo-cx386-plus + ((o neo-cx386-mc)) 20)
      (cl-defmethod neo-cx386-plus + ((o neo-cx386-mc)) 30)
      (cl-defgeneric neo-cx386-maxg (obj) (:method-combination max))
      (cl-defmethod neo-cx386-maxg max ((o neo-cx386-mc)) 10)
      (cl-defmethod neo-cx386-maxg max ((o neo-cx386-mc)) 50)
      (cl-defmethod neo-cx386-maxg max ((o neo-cx386-mc)) 25)
      (cl-defgeneric neo-cx386-ming (obj) (:method-combination min))
      (cl-defmethod neo-cx386-ming min ((o neo-cx386-mc)) 10)
      (cl-defmethod neo-cx386-ming min ((o neo-cx386-mc)) 50)
      (cl-defmethod neo-cx386-ming min ((o neo-cx386-mc)) 25)
      (let ((inst (make-instance 'neo-cx386-mc)))
        (list (neo-cx386-plus inst) (neo-cx386-maxg inst) (neo-cx386-ming inst))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx386_eieio_eql_specializer_and_class_slots() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (5 5 10 10 1 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx386-static ()
        ((counter :allocation :class :initform 0)
         (id :initarg :id)))
      (let ((a (make-instance 'neo-cx386-static :id 1))
            (b (make-instance 'neo-cx386-static :id 2)))
        (oset a counter 5)
        (list (slot-value a 'counter) (slot-value b 'counter)
              (oset b counter 10)
              (slot-value a 'counter)
              (slot-value a 'id) (slot-value b 'id))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx386_eieio_print_object_and_change_class() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored wrong-number-of-arguments)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx386-v1 () ((x :initarg :x :initform 0)))
      (defclass neo-cx386-v2 () ((x :initarg :x :initform 0)
                                 (y :initarg :y :initform 0)))
      (defclass neo-cx386-po ()
        ((name :initarg :name :initform "anon")))
      (cl-defmethod cl-print-object ((o neo-cx386-po) stream)
        (princ (format "#<PO:%s>" (slot-value o 'name)) stream) o)
      (let ((inst (make-instance 'neo-cx386-v1 :x 10)))
        (change-class inst 'neo-cx386-v2 :y 20)
        (let ((po (make-instance 'neo-cx386-po :name "alpha")))
          (list (slot-value inst 'x) (slot-value inst 'y) (class-of inst)
                (let ((print-circle t)) (prin1-to-string po))
                (princ-to-string po)))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx386_eieio_slot_boundp_makunbound_with_slots() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t 0 unbound 99 99 eieio--unbound nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx386-sb ()
        ((x :initarg :x :initform unbound)
         (y :initarg :y :initform 0)))
      (let ((inst (make-instance 'neo-cx386-sb)))
        (list (slot-boundp inst 'x)
              (slot-boundp inst 'y)
              (slot-value inst 'y)
              (condition-case err (slot-value inst 'x) (error (car err)))
              (with-slots (x y) inst (setq y 99) y)
              (slot-value inst 'y)
              (slot-makeunbound inst 'y)
              (slot-boundp inst 'y))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx386_eieio_initialize_instance_and_parents_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:init-ran (#s(eieio--class neo-cx386-mid nil (#s(eieio--class neo-cx386-root nil (#s(eieio--class eieio-default-superclass \"Default parent class for classes with no specified parent class.\\nIts slots are automatically adopted by classes with no specified parents.\" (#s(built-in-class record \"Abstract type of objects with slots.\" (#s(built-in-class atom \"Abstract supertype of anything but cons cells.\" (#s(built-in-class t \"Abstract supertype of everything.\" nil nil nil nil)) nil nil nil)) nil nil nil)) [] #s(hash-table test eq) (neo-cx386-root neo-cx386-init) nil [] [] #s(#6) (:custom-groups nil :documentation \"Default parent class for classes with no specified parent class.\\nIts slots are automatically adopted by classes with no specified parents.\" :abstract t))) [] #s(hash-table test eq) (neo-cx386-mid) nil [] [] #s(#4) (:custom-groups nil))) [] #s(hash-table test eq) (neo-cx386-leaf) nil [] [] #s(#2) (:custom-groups nil))) (#s(eieio--class neo-cx386-root nil (#s(eieio--class eieio-default-superclass \"Default parent class for classes with no specified parent class.\\nIts slots are automatically adopted by classes with no specified parents.\" (#s(built-in-class record \"Abstract type of objects with slots.\" (#s(built-in-class atom \"Abstract supertype of anything but cons cells.\" (#s(built-in-class t \"Abstract supertype of everything.\" nil nil nil nil)) nil nil nil)) nil nil nil)) [] #s(hash-table test eq) (neo-cx386-root neo-cx386-init) nil [] [] #s(#4) (:custom-groups nil :documentation \"Default parent class for classes with no specified parent class.\\nIts slots are automatically adopted by classes with no specified parents.\" :abstract t))) [] #s(hash-table test eq) (neo-cx386-mid) nil [] [] #s(#2) (:custom-groups nil))) (neo-cx386-mid))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx386-init ()
        ((computed :initform nil)))
      (cl-defmethod initialize-instance :after ((o neo-cx386-init) &rest _)
        (oset o computed :init-ran))
      (defclass neo-cx386-root () ())
      (defclass neo-cx386-mid (neo-cx386-root) ())
      (defclass neo-cx386-leaf (neo-cx386-mid) ())
      (let ((inst (make-instance 'neo-cx386-init)))
        (list (slot-value inst 'computed)
              (eieio-class-parents 'neo-cx386-leaf)
              (eieio-class-parents 'neo-cx386-mid)
              (eieio-class-children 'neo-cx386-root))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx386_eieio_no_primary_and_no_applicable() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:caught-no-primary)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx386-na () ())
      (cl-defgeneric neo-cx386-na-call (obj))
      (cl-defmethod neo-cx386-na-call :before ((o neo-cx386-na)) :before-only)
      (let ((inst (make-instance 'neo-cx386-na)))
        (condition-case err
            (neo-cx386-na-call inst)
          (cl-no-primary-method (list :caught-no-primary))
          (error (list :caught-other (car err))))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx386_eieio_keyword_args_and_with_accessors() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx386-kw () ()
        )
      (cl-defgeneric neo-cx386-kw-call (obj &key mode verbose))
      (cl-defmethod neo-cx386-kw-call ((obj neo-cx386-kw) &key mode verbose)
        (list obj mode verbose))
      (defclass neo-cx386-wa ()
        ((x :initarg :x :initform 0 :reader neo-cx386-get-x)
         (y :initarg :y :initform 0 :accessor neo-cx386-y)))
      (let ((kinst (make-instance 'neo-cx386-kw))
            (winst (make-instance 'neo-cx386-wa :x 1 :y 2)))
        (list (neo-cx386-kw-call kinst :mode :a)
              (neo-cx386-kw-call kinst :mode :b :verbose t)
              (with-accessors ((gx neo-cx386-get-x) (gy neo-cx386-y)) winst
                (list gx gy)))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx386_eieio_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored wrong-type-argument)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx386-mega ()
        ((value :initarg :value :initform 0)))
      (cl-defgeneric neo-cx386-mega-call (obj mult))
      (cl-defmethod neo-cx386-mega-call :before ((obj neo-cx386-mega) m)
        (oset obj value (+ (slot-value obj 'value) m)))
      (cl-defmethod neo-cx386-mega-call :after ((obj neo-cx386-mega) m)
        (oset obj value (* (slot-value obj 'value) m)))
      (cl-defmethod neo-cx386-mega-call ((obj neo-cx386-mega) m)
        (oset obj value (+ (slot-value obj 'value) m))
        (slot-value obj 'value))
      (let ((inst (make-instance 'neo-cx386-mega :value 1)))
        (with-temp-buffer
          (buffer-enable-undo)
          (insert "EIEIO ultimate mega test buffer content")
          (put-text-property 1 6 'face 'bold)
          (let ((m (set-marker (make-marker) 8))
                (ov (make-overlay 4 14)))
            (overlay-put ov 'face 'italic)
            (overlay-put ov 'evaporate t)
            (narrow-to-region 2 18)
            (let ((r (neo-cx386-mega-call inst 5)))
              (let ((state (list r (slot-value inst 'value)
                                 (cl-typep inst 'neo-cx386-mega)
                                 (object-of-class-p inst 'standard-object)
                                 (buffer-string)
                                 (marker-position m)
                                 (overlay-start ov) (overlay-end ov)
                                 (text-properties-at 1))))
                (undo)
                (widen()
                (list state (buffer-string) (marker-position m)
                      (overlay-start ov) (overlay-end ov)
                      (text-properties-at 1)))))))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}
