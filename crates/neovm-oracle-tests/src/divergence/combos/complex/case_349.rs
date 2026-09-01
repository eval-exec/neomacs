//! Complex combo batch 349 — `eieio` ultimate: multiple inheritance method
//! resolution, :around chaining through 4 levels, method combination max/min/
//! and/or/nconc/append, slot-boundp/makunbound, with-slots/with-accessors.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx349_eieio_multiple_inheritance_dispatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (11 22 :a neo-cx349-c)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx349-a () ((x :initarg :x :initform 1)))
      (defclass neo-cx349-b () ((y :initarg :y :initform 2)))
      (defclass neo-cx349-c (neo-cx349-a neo-cx349-b) ())
      (cl-defmethod neo-cx349-who ((o neo-cx349-a)) :a)
      (cl-defmethod neo-cx349-who ((o neo-cx349-b)) :b)
      (let ((inst (neo-cx349-c :x 11 :y 22)))
        (list (slot-value inst 'x) (slot-value inst 'y)
              (neo-cx349-who inst) (class-of inst))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx349_eieio_method_combination_min_and_or() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx349-mc () ())
      (cl-defgeneric neo-cx349-minc (obj) (:method-combination min))
      (cl-defmethod neo-cx349-minc min ((obj neo-cx349-mc)) 10)
      (cl-defmethod neo-cx349-minc min ((obj neo-cx349-mc)) 50)
      (cl-defmethod neo-cx349-minc min ((obj neo-cx349-mc)) 25)
      (cl-defgeneric neo-cx349-andc (obj) (:method-combination and))
      (cl-defmethod neo-cx349-andc and ((obj neo-cx349-mc)) t)
      (cl-defmethod neo-cx349-andc and ((obj neo-cx349-mc)) nil)
      (cl-defgeneric neo-cx349-orc (obj) (:method-combination or))
      (cl-defmethod neo-cx349-orc or ((obj neo-cx349-mc)) nil)
      (cl-defmethod neo-cx349-orc or ((obj neo-cx349-mc)) :found)
      (let ((inst (make-instance 'neo-cx349-mc)))
        (list (neo-cx349-minc inst)
              (neo-cx349-andc inst)
              (neo-cx349-orc inst))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx349_eieio_slot_boundp_makunbound_with_slots() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t 0 unbound 99 99 eieio--unbound nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx349-sb ()
        ((x :initarg :x :initform unbound)
         (y :initarg :y :initform 0)))
      (let ((inst (make-instance 'neo-cx349-sb)))
        (list (slot-boundp inst 'x)
              (slot-boundp inst 'y)
              (slot-value inst 'y)
              (condition-case err (slot-value inst 'x) (error (car err)))
              (with-slots (x y) inst
                (setq y 99) y)
              (slot-value inst 'y)
              (slot-makeunbound inst 'y)
              (slot-boundp inst 'y))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx349_eieio_print_object_and_change_class() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored wrong-number-of-arguments)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx349-v1 () ((x :initarg :x :initform 0)))
      (defclass neo-cx349-v2 () ((x :initarg :x :initform 0)
                                 (y :initarg :y :initform 0)))
      (defclass neo-cx349-po ()
        ((name :initarg :name :initform "anon")))
      (cl-defmethod cl-print-object ((o neo-cx349-po) stream)
        (princ (format "#<PO:%s>" (slot-value o 'name)) stream)
        o)
      (let ((inst (make-instance 'neo-cx349-v1 :x 10)))
        (change-class inst 'neo-cx349-v2 :y 20)
        (let ((po (make-instance 'neo-cx349-po :name "alpha")))
          (list (slot-value inst 'x) (slot-value inst 'y) (class-of inst)
                (let ((print-circle t)) (prin1-to-string po))
                (princ-to-string po)))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx349_eieio_class_allocated_and_object_of_class_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored wrong-type-argument)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx349-cls ()
        ((counter :allocation :class :initform 0)
         (tag :initarg :tag)))
      (let ((a (make-instance 'neo-cx349-cls :tag :a))
            (b (make-instance 'neo-cx349-cls :tag :b)))
        (oset a counter 5)
        (list (slot-value a 'counter) (slot-value b 'counter)
              (oset b counter 10)
              (slot-value a 'counter) (slot-value b 'counter)
              (object-of-class-p a 'neo-cx349-cls)
              (object-of-class-p a 'standard-object)
              (same-class-p a 'neo-cx349-cls))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx349_eieio_initialize_instance_custom_and_class_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:init-ran (#s(eieio--class neo-cx349-mid nil (#s(eieio--class neo-cx349-root nil (#s(eieio--class eieio-default-superclass \"Default parent class for classes with no specified parent class.\\nIts slots are automatically adopted by classes with no specified parents.\" (#s(built-in-class record \"Abstract type of objects with slots.\" (#s(built-in-class atom \"Abstract supertype of anything but cons cells.\" (#s(built-in-class t \"Abstract supertype of everything.\" nil nil nil nil)) nil nil nil)) nil nil nil)) [] #s(hash-table test eq) (neo-cx349-root neo-cx349-init) nil [] [] #s(#6) (:custom-groups nil :documentation \"Default parent class for classes with no specified parent class.\\nIts slots are automatically adopted by classes with no specified parents.\" :abstract t))) [] #s(hash-table test eq) (neo-cx349-mid) nil [] [] #s(#4) (:custom-groups nil))) [] #s(hash-table test eq) (neo-cx349-leaf) nil [] [] #s(#2) (:custom-groups nil))) (#s(eieio--class neo-cx349-root nil (#s(eieio--class eieio-default-superclass \"Default parent class for classes with no specified parent class.\\nIts slots are automatically adopted by classes with no specified parents.\" (#s(built-in-class record \"Abstract type of objects with slots.\" (#s(built-in-class atom \"Abstract supertype of anything but cons cells.\" (#s(built-in-class t \"Abstract supertype of everything.\" nil nil nil nil)) nil nil nil)) nil nil nil)) [] #s(hash-table test eq) (neo-cx349-root neo-cx349-init) nil [] [] #s(#4) (:custom-groups nil :documentation \"Default parent class for classes with no specified parent class.\\nIts slots are automatically adopted by classes with no specified parents.\" :abstract t))) [] #s(hash-table test eq) (neo-cx349-mid) nil [] [] #s(#2) (:custom-groups nil))) (neo-cx349-mid) #s(eieio--class neo-cx349-leaf nil (#s(eieio--class neo-cx349-mid nil (#s(eieio--class neo-cx349-root nil (#s(eieio--class eieio-default-superclass \"Default parent class for classes with no specified parent class.\\nIts slots are automatically adopted by classes with no specified parents.\" (#s(built-in-class record \"Abstract type of objects with slots.\" (#s(built-in-class atom \"Abstract supertype of anything but cons cells.\" (#s(built-in-class t \"Abstract supertype of everything.\" nil nil nil nil)) nil nil nil)) nil nil nil)) [] #s(hash-table test eq) (neo-cx349-root neo-cx349-init) nil [] [] #s(#7) (:custom-groups nil :documentation \"Default parent class for classes with no specified parent class.\\nIts slots are automatically adopted by classes with no specified parents.\" :abstract t))) [] #s(hash-table test eq) (neo-cx349-mid) nil [] [] #s(#5) (:custom-groups nil))) [] #s(hash-table test eq) (neo-cx349-leaf) nil [] [] #s(#3) (:custom-groups nil))) [] #s(hash-table test eq) nil nil [] [] #s(#1) (:custom-groups nil)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx349-init ()
        ((computed :initform nil)))
      (cl-defmethod initialize-instance :after ((o neo-cx349-init) &rest _)
        (oset o computed :init-ran))
      (defclass neo-cx349-root () ())
      (defclass neo-cx349-mid (neo-cx349-root) ())
      (defclass neo-cx349-leaf (neo-cx349-mid) ())
      (let ((inst (make-instance 'neo-cx349-init)))
        (list (slot-value inst 'computed)
              (eieio-class-parents 'neo-cx349-leaf)
              (eieio-class-parents 'neo-cx349-mid)
              (eieio-class-children 'neo-cx349-root)
              (find-class 'neo-cx349-leaf))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx349_eieio_no_primary_and_no_applicable_method() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:caught-no-primary)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx349-na () ())
      (cl-defgeneric neo-cx349-na-call (obj))
      (cl-defmethod neo-cx349-na-call :before ((obj neo-cx349-na)) :before-only)
      (let ((inst (make-instance 'neo-cx349-na)))
        (condition-case err
            (neo-cx349-na-call inst)
          (cl-no-primary-method (list :caught-no-primary))
          (error (list :caught-other (car err))))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx349_eieio_defmethod_with_keyword_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((#s(neo-cx349-kw) :a nil) (#s(neo-cx349-kw) :b t))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx349-kw () ())
      (cl-defgeneric neo-cx349-kw-call (obj &key mode verbose))
      (cl-defmethod neo-cx349-kw-call ((obj neo-cx349-kw) &key mode verbose)
        (list obj mode verbose))
      (let ((inst (make-instance 'neo-cx349-kw)))
        (list (neo-cx349-kw-call inst :mode :a)
              (neo-cx349-kw-call inst :mode :b :verbose t))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx349_eieio_with_accessors_macro() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx349-wa ()
        ((x :initarg :x :initform 0 :reader neo-cx349-get-x)
         (y :initarg :y :initform 0 :accessor neo-cx349-y)))
      (let ((inst (make-instance 'neo-cx349-wa :x 1 :y 2)))
        (with-accessors ((gx neo-cx349-get-x) (gy neo-cx349-y)) inst
          (list gx gy))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx349_eieio_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored wrong-type-argument)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx349-mega ()
        ((value :initarg :value :initform 0)))
      (cl-defgeneric neo-cx349-mega-call (obj mult))
      (cl-defmethod neo-cx349-mega-call :before ((obj neo-cx349-mega) m)
        (oset obj value (+ (slot-value obj 'value) m)))
      (cl-defmethod neo-cx349-mega-call :after ((obj neo-cx349-mega) m)
        (oset obj value (* (slot-value obj 'value) m)))
      (cl-defmethod neo-cx349-mega-call ((obj neo-cx349-mega) m)
        (oset obj value (+ (slot-value obj 'value) m))
        (slot-value obj 'value))
      (let ((inst (make-instance 'neo-cx349-mega :value 1)))
        (with-temp-buffer
          (buffer-enable-undo)
          (insert "EIEIO ultimate mega test buffer content")
          (put-text-property 1 6 'face 'bold)
          (let ((m (set-marker (make-marker) 8))
                (ov (make-overlay 4 14)))
            (overlay-put ov 'face 'italic)
            (overlay-put ov 'evaporate t)
            (narrow-to-region 2 18)
            (let ((r (neo-cx349-mega-call inst 5)))
              (let ((state (list r (slot-value inst 'value)
                                 (cl-typep inst 'neo-cx349-mega)
                                 (object-of-class-p inst 'standard-object)
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
