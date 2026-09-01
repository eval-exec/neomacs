//! Complex combo batch 87 — eieio / cl-defstruct interop with print-object,
//! method dispatch on parent class, generic function redefinition, and
//! `cl-defmethod` with `&context` (specializers).

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx87_eieio_generic_method_dispatch_parent_then_child() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:base :derived neo-cx87-base neo-cx87-derived)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx87-base () ())
      (defclass neo-cx87-derived (neo-cx87-base) ())
      (cl-defgeneric neo-cx87-fn (obj))
      (cl-defmethod neo-cx87-fn ((obj neo-cx87-base)) :base)
      (cl-defmethod neo-cx87-fn ((obj neo-cx87-derived)) :derived)
      (let ((b (make-instance 'neo-cx87-base))
            (d (make-instance 'neo-cx87-derived)))
        (list (neo-cx87-fn b)
              (neo-cx87-fn d)
              (class-of b) (class-of d))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx87_eieio_generic_method_redefinition() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:first :second)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx87-red () ())
      (cl-defgeneric neo-cx87-call (obj))
      (cl-defmethod neo-cx87-call ((obj neo-cx87-red)) :first)
      (let ((first (neo-cx87-call (make-instance 'neo-cx87-red))))
        (cl-defmethod neo-cx87-call ((obj neo-cx87-red)) :second)
        (let ((second (neo-cx87-call (make-instance 'neo-cx87-red))))
          (list first second))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx87_eieio_no_applicable_method_handler() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:other-error cl-no-applicable-method)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx87-na () ())
      (cl-defgeneric neo-cx87-g (obj))
      (let ((inst (make-instance 'neo-cx87-na)))
        (condition-case err
            (neo-cx87-g inst)
          (no-method (list :no-method err))
          (error (list :other-error (car err))))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx87_eieio_slot_unbound_method_handler() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t unbound)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx87-su ()
        ((x :initarg :x :initform unbound)))
      (let ((inst (make-instance 'neo-cx87-su)))
        (list (slot-boundp inst 'x)
              (condition-case err (slot-value inst 'x) (error (cons :err (car err)))))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx87_cl_defstruct_with_read_only_and_named() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(cl-defstruct (neo-cx87-vec (:type vector) :named)
  (val 0 :read-only t)
  name)
(let ((r (make-neo-cx87-vec :val 99 :name "alpha")))
  (list (neo-cx87-vec-p r)
        (neo-cx87-vec-val r)
        (neo-cx87-vec-name r)
        (condition-case e (setf (neo-cx87-vec-val r) 100) (error (cons :err (car e))))))
"##,
        expect,
    );
}

#[test]
fn div_cx87_cl_defstruct_conc_name_prefix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(cl-defstruct (neo-cx87-rec (:conc-name neo-cx87-r-))
  a b c)
(let ((r (make-neo-cx87-rec :a 1 :b 2 :c 3)))
  (list (neo-cx87-r-a r)
        (neo-cx87-r-b r)
        (neo-cx87-r-c r)
        (setf (neo-cx87-r-a r) 100)
        (neo-cx87-r-a r)))
"##,
        expect,
    );
}

#[test]
fn div_cx87_cl_defstruct_constructor_and_copier() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(cl-defstruct (neo-cx87-cust
               (:constructor neo-cx87-make (x y))
               (:constructor neo-cx87-new-with-z (x y z))
               (:copier neo-cx87-copy))
  x y z)
(let* ((r1 (neo-cx87-make 1 2))
       (r2 (neo-cx87-new-with-z 1 2 3))
       (c (neo-cx87-copy r2)))
  (list (neo-cx87-cust-x r1) (neo-cx87-cust-y r1) (neo-cx87-cust-z r1)
        (neo-cx87-cust-x r2) (neo-cx87-cust-y r2) (neo-cx87-cust-z r2)
        (neo-cx87-cust-z c)
        (eq r2 c)))
"##,
        expect,
    );
}

#[test]
fn div_cx87_eieio_make_instance_with_keyword_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 1 1 2 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx87-mk ()
        ((a :initarg :a :initform 0)
         (b :initarg :b :initform nil)
         (c :initarg :c :initform :default)))
      (list (slot-value (make-instance 'neo-cx87-mk) 'a)
            (slot-value (make-instance 'neo-cx87-mk :a 1) 'a)
            (slot-value (make-instance 'neo-cx87-mk :a 1 :b 2 :c 3) 'a)
            (slot-value (make-instance 'neo-cx87-mk :a 1 :b 2 :c 3) 'b)
            (slot-value (make-instance 'neo-cx87-mk :a 1 :b 2 :c 3) 'c)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx87_eieio_class_parents_and_children() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (nil (#s(eieio--class neo-cx87-root nil (#s(eieio--class eieio-default-superclass \"Default parent class for classes with no specified parent class.\\nIts slots are automatically adopted by classes with no specified parents.\" (#s(built-in-class record \"Abstract type of objects with slots.\" (#s(built-in-class atom \"Abstract supertype of anything but cons cells.\" (#s(built-in-class t \"Abstract supertype of everything.\" nil nil nil nil)) nil nil nil)) nil nil nil)) [] #s(hash-table test eq) (neo-cx87-root) nil [] [] #s(#4) (:custom-groups nil :documentation \"Default parent class for classes with no specified parent class.\\nIts slots are automatically adopted by classes with no specified parents.\" :abstract t))) [] #s(hash-table test eq) (neo-cx87-mid) nil [] [] #s(#2) (:custom-groups nil))) (#s(eieio--class neo-cx87-mid nil (#s(eieio--class neo-cx87-root nil (#s(eieio--class eieio-default-superclass \"Default parent class for classes with no specified parent class.\\nIts slots are automatically adopted by classes with no specified parents.\" (#s(built-in-class record \"Abstract type of objects with slots.\" (#s(built-in-class atom \"Abstract supertype of anything but cons cells.\" (#s(built-in-class t \"Abstract supertype of everything.\" nil nil nil nil)) nil nil nil)) nil nil nil)) [] #s(hash-table test eq) (neo-cx87-root) nil [] [] #s(#6) (:custom-groups nil :documentation \"Default parent class for classes with no specified parent class.\\nIts slots are automatically adopted by classes with no specified parents.\" :abstract t))) [] #s(hash-table test eq) (neo-cx87-mid) nil [] [] #s(#4) (:custom-groups nil))) [] #s(hash-table test eq) (neo-cx87-leaf) nil [] [] #s(#2) (:custom-groups nil))) (neo-cx87-mid) nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx87-root () ())
      (defclass neo-cx87-mid (neo-cx87-root) ())
      (defclass neo-cx87-leaf (neo-cx87-mid) ())
      (list (eieio-class-parents 'neo-cx87-root)
            (eieio-class-parents 'neo-cx87-mid)
            (eieio-class-parents 'neo-cx87-leaf)
            (eieio-class-children 'neo-cx87-root)
            (eieio-class-children 'neo-cx87-leaf)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx87_eieio_class_p_object_p_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx87-pred () ((x :initarg :x)))
      (let ((inst (make-instance 'neo-cx87-pred :x 1)))
        (list (object-p inst)
              (eieio-object-p inst)
              (cl-typep inst 'neo-cx87-pred)
              (cl-typep inst 'standard-object)
              (cl-typep 42 'neo-cx87-pred)
              (cl-typep inst 'integer))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx87_eieio_with_cl_print_object_and_circle_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r##""OK (\"#s(neo-cx87-pc (a b c))\" \"#s(neo-cx87-pc (a b c))\" \"#s(neo-cx87-pc (a b c))\" \"#s(neo-cx87-pc (a b c))\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx87-pc ()
        ((items :initarg :items :initform nil)))
      (cl-defmethod cl-print-object ((o neo-cx87-pc) stream)
        (princ "#<PC:" stream)
        (princ (mapcar #'identity (slot-value o 'items)) stream)
        (princ ">" stream)
        o)
      (let ((inst (make-instance 'neo-cx87-pc :items '(a b c))))
        (list (let ((print-circle t)) (prin1-to-string inst))
              (let ((print-circle nil)) (prin1-to-string inst))
              (format "%S" inst)
              (format "%s" inst))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx87_eieio_method_combination_plus_dispatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx87-mc () ())
      (cl-defgeneric neo-cx87-call (obj)
        (:method-combination +))
      (cl-defmethod neo-cx87-call + ((obj neo-cx87-mc)) 10)
      (cl-defmethod neo-cx87-call + ((obj neo-cx87-mc)) 20)
      (let ((inst (make-instance 'neo-cx87-mc)))
        (neo-cx87-call inst)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx87_eieio_dispatch_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored cl-no-primary-method)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx87-mega () ((v :initarg :v :initform 0)))
      (cl-defmethod neo-cx87-touch :before ((o neo-cx87-mega) x) (oset o v (+ (slot-value o 'v) x)))
      (cl-defmethod neo-cx87-touch :after  ((o neo-cx87-mega) x) (oset o v (* (slot-value o 'v) 2)))
      (let ((inst (make-instance 'neo-cx87-mega :v 1)))
        (with-temp-buffer
          (buffer-enable-undo)
          (insert "EIEIO mega test buffer content")
          (put-text-property 1 6 'face 'bold)
          (let ((m (set-marker (make-marker) 8))
                (ov (make-overlay 4 14)))
            (overlay-put ov 'face 'italic)
            (overlay-put ov 'evaporate t)
            (narrow-to-region 2 20)
            (neo-cx87-touch inst 5)
            (let ((state (list (slot-value inst 'v)
                               (buffer-string)
                               (marker-position m)
                               (overlay-start ov) (overlay-end ov)
                               (text-properties-at 1))))
              (undo)
              (widen)
              (list state (buffer-string) (marker-position m)
                    (overlay-start ov) (overlay-end ov)
                    (text-properties-at 1)))))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}
