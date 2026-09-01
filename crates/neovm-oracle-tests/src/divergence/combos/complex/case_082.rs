//! Complex combo batch 82 — eieio dispatch with multiple inheritance,
//! method combination, defclass metaclass options, slot allocation, and
//! interaction with cl-defstruct.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx82_eieio_class_hierarchy_with_initforms() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (4 \"labrador\" t t neo-cx82-dog #s(eieio--class neo-cx82-dog nil (#s(eieio--class neo-cx82-animal nil (#s(eieio--class eieio-default-superclass \"Default parent class for classes with no specified parent class.\\nIts slots are automatically adopted by classes with no specified parents.\" (#s(built-in-class record \"Abstract type of objects with slots.\" (#s(built-in-class atom \"Abstract supertype of anything but cons cells.\" (#s(built-in-class t \"Abstract supertype of everything.\" nil nil nil nil)) nil nil nil)) nil nil nil)) [] #s(hash-table test eq) (neo-cx82-animal) nil [] [] #s(#5) (:custom-groups nil :documentation \"Default parent class for classes with no specified parent class.\\nIts slots are automatically adopted by classes with no specified parents.\" :abstract t))) [#s(cl-slot-descriptor legs 4 t nil)] #s(hash-table test eq data (legs 1)) (neo-cx82-dog) ((:legs . legs)) [] [] #s(#3 4) (:custom-groups nil))) [#s(cl-slot-descriptor legs 4 t nil) #s(cl-slot-descriptor breed \"unknown\" t nil)] #s(hash-table test eq data (legs 1 breed 2)) nil ((:legs . legs) (:breed . breed)) [] [] #s(#1 4 \"unknown\") (:custom-groups nil)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx82-animal () ((legs :initarg :legs :initform 4)))
      (defclass neo-cx82-dog (neo-cx82-animal)
        ((breed :initarg :breed :initform "unknown")))
      (let ((d (neo-cx82-dog :legs 4 :breed "labrador")))
        (list (slot-value d 'legs)
              (slot-value d 'breed)
              (object-of-class-p d 'neo-cx82-animal)
              (object-of-class-p d 'neo-cx82-dog)
              (class-of d)
              (find-class 'neo-cx82-dog))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx82_eieio_multiple_inheritance_precedence() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (neo-cx82-c (#s(eieio--class neo-cx82-a nil (#s(eieio--class eieio-default-superclass \"Default parent class for classes with no specified parent class.\\nIts slots are automatically adopted by classes with no specified parents.\" (#s(built-in-class record \"Abstract type of objects with slots.\" (#s(built-in-class atom \"Abstract supertype of anything but cons cells.\" (#s(built-in-class t \"Abstract supertype of everything.\" nil nil nil nil)) nil nil nil)) nil nil nil)) [] #s(hash-table test eq) (neo-cx82-b neo-cx82-a) nil [] [] #s(#4) (:custom-groups nil :documentation \"Default parent class for classes with no specified parent class.\\nIts slots are automatically adopted by classes with no specified parents.\" :abstract t))) [] #s(hash-table test eq) (neo-cx82-c) nil [] [] #s(#2) (:custom-groups nil)) #s(eieio--class neo-cx82-b nil (#s(eieio--class eieio-default-superclass \"Default parent class for classes with no specified parent class.\\nIts slots are automatically adopted by classes with no specified parents.\" (#s(built-in-class record \"Abstract type of objects with slots.\" (#s(built-in-class atom \"Abstract supertype of anything but cons cells.\" (#s(built-in-class t \"Abstract supertype of everything.\" nil nil nil nil)) nil nil nil)) nil nil nil)) [] #s(hash-table test eq) (neo-cx82-b neo-cx82-a) nil [] [] #s(#4) (:custom-groups nil :documentation \"Default parent class for classes with no specified parent class.\\nIts slots are automatically adopted by classes with no specified parents.\" :abstract t))) [] #s(hash-table test eq) (neo-cx82-c) nil [] [] #s(#2) (:custom-groups nil))) nil t nil t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx82-a () ())
      (defclass neo-cx82-b () ())
      (defclass neo-cx82-c (neo-cx82-a neo-cx82-b) ())
      (let ((inst (make-instance 'neo-cx82-c)))
        (list (class-of inst)
              (eieio-class-parents 'neo-cx82-c)
              (eieio-class-parents 'neo-cx82-a)
              (same-class-p inst 'neo-cx82-c)
              (same-class-p inst 'neo-cx82-a)
              (object-of-class-p inst 'neo-cx82-b))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx82_eieio_method_qualifier_dispatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:before :primary :after)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx82-q () () )
      (let (calls)
        (cl-defmethod neo-cx82-fn :before ((o neo-cx82-q)) (push :before calls))
        (cl-defmethod neo-cx82-fn ((o neo-cx82-q)) (push :primary calls))
        (cl-defmethod neo-cx82-fn :after ((o neo-cx82-q)) (push :after calls))
        (let ((r (neo-cx82-fn (make-instance 'neo-cx82-q))))
          (nreverse calls))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx82_eieio_around_method_with_call_next_method() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:done (:around-begin :primary :around-end))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx82-ar () () )
      (let (calls)
        (cl-defmethod neo-cx82-call :around ((o neo-cx82-ar))
          (push :around-begin calls)
          (let ((result (cl-call-next-method)))
            (push :around-end calls)
            result))
        (cl-defmethod neo-cx82-call ((o neo-cx82-ar))
          (push :primary calls)
          :done)
        (let ((r (neo-cx82-call (make-instance 'neo-cx82-ar))))
          (list r (nreverse calls)))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx82_eieio_class_slots_shared_across_instances() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (5 5 10 10)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx82-static ()
        ((counter :allocation :class :initform 0)
         (instance-tag :initarg :tag)))
      (let ((a (make-instance 'neo-cx82-static :tag :a))
            (b (make-instance 'neo-cx82-static :tag :b)))
        (oset a counter 5)
        (list (slot-value a 'counter)
              (slot-value b 'counter)
              (oset b counter 10)
              (slot-value a 'counter))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx82_eieio_custom_initialize_instance() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK :init-ran""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx82-init ()
        ((computed :initform nil)))
      (cl-defmethod initialize-instance :after ((o neo-cx82-init) &rest _)
        (oset o computed :init-ran))
      (let ((inst (make-instance 'neo-cx82-init)))
        (slot-value inst 'computed)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx82_eieio_slot_options_read_only_writer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:val :val)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx82-ro ()
        ((read-only :initarg :read-only :initform :default :reader neo-cx82-get)))
      (let ((inst (make-instance 'neo-cx82-ro :read-only :val)))
        (list (slot-value inst 'read-only)
              (neo-cx82-get inst))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx82_eieio_with_slots_macro_access() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (11 22)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx82-ws ()
        ((x :initarg :x :initform 0)
         (y :initarg :y :initform 0)))
      (let ((inst (make-instance 'neo-cx82-ws :x 1 :y 2)))
        (with-slots (x y) inst
          (list x y
                (cl-incf x 10)
                (cl-incf y 20)
                x y))
        (list (slot-value inst 'x) (slot-value inst 'y))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx82_eieio_print_object_custom() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx82-po ()
        ((name :initarg :name :initform "anon")))
      (cl-defmethod cl-print-object ((o neo-cx82-po) stream)
        (princ (format "#<PO:%s>" (slot-value o 'name)) stream)
        o)
      (let ((inst (make-instance 'neo-cx82-po :name "alpha")))
        (list (prin1-to-string inst)
              (princ-to-string inst)
              (format "%s" inst))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx82_cl_defstruct_interop_with_eieio_classes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 99 100 100 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (cl-defstruct neo-cx82-data value)
      (defclass neo-cx82-holder () ((data :initarg :data)))
      (let* ((rec (make-neo-cx82-data :value 99))
             (holder (make-instance 'neo-cx82-holder :data rec)))
        (list (neo-cx82-data-value rec)
              (neo-cx82-data-value (slot-value holder 'data))
              (setf (neo-cx82-data-value rec) 100)
              (neo-cx82-data-value (slot-value holder 'data))
              (eq (slot-value holder 'data) rec))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx82_eieio_change_class_after_init() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored wrong-number-of-arguments)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx82-v1 () ((x :initarg :x :initform 0)))
      (defclass neo-cx82-v2 () ((x :initarg :x :initform 0)
                                (y :initarg :y :initform 0)))
      (let ((inst (make-instance 'neo-cx82-v1 :x 10)))
        (change-class inst 'neo-cx82-v2 :y 20)
        (list (slot-value inst 'x)
              (slot-value inst 'y)
              (class-of inst))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx82_eieio_dispatch_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored args-out-of-range)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx82-mega ()
        ((value :initarg :value :initform 0)))
      (let ((calls nil))
        (cl-defmethod neo-cx82-touch :before ((o neo-cx82-mega) v)
          (push (list :before v) calls))
        (cl-defmethod neo-cx82-touch ((o neo-cx82-mega) v)
          (oset o value v)
          (push (list :primary v) calls))
        (let ((inst (make-instance 'neo-cx82-mega :value 0)))
          (with-temp-buffer
            (buffer-enable-undo)
            (insert "EIEIO test buffer content")
            (put-text-property 1 5 'face 'bold)
            (let ((m (set-marker (make-marker) 8))
                  (ov (make-overlay 4 14)))
              (overlay-put ov 'face 'italic)
              (overlay-put ov 'evaporate t)
              (narrow-to-region 2 18)
              (neo-cx82-touch inst 42)
              (let ((state (list (nreverse calls)
                                 (slot-value inst 'value)
                                 (buffer-string)
                                 (marker-position m)
                                 (overlay-start ov) (overlay-end ov)
                                 (text-properties-at 1))))
                (undo)
                (widen)
                (list state
                      (buffer-string) (marker-position m)
                      (overlay-start ov) (overlay-end ov)
                      (text-properties-at 1))))))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}
