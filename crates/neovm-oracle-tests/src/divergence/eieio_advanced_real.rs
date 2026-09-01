//! Divergence tests: real EIEIO advanced behavioral differences.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_eieio_class_allocated_slots() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 2 0 0)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (defclass test-counter-xxx ()
    ((count :initform 0 :accessor test-counter-count-xxx
            :allocation :class)
     (id :initarg :id :accessor test-counter-id-xxx))
    \"A class with class-allocated slot.\")
  (let ((c1 (test-counter-xxx \"c1\" :id 1))
        (c2 (test-counter-xxx \"c2\" :id 2)))
    (list (test-counter-id-xxx c1)
          (test-counter-id-xxx c2)
          (test-counter-count-xxx c1)
          (test-counter-count-xxx c2)))) ",
        expect,
    );
}

#[test]
fn divergence_eieio_slot_boundp() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil 42)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (defclass test-sb-xxx ()
    ((x :initarg :x)
     (y :initarg :y :initform 42)))
  (let ((o (test-sb-xxx \"o\")))
    (list (slot-boundp o 'y)
          (slot-boundp o 'x)
          (slot-value o 'y)))) ",
        expect,
    );
}

#[test]
fn divergence_eieio_make_instance() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (3 4)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (defclass test-point-xxx ()
    ((x :initarg :x :initform 0)
     (y :initarg :y :initform 0)))
  (let ((p1 (make-instance 'test-point-xxx :x 3 :y 4)))
    (list (slot-value p1 'x)
          (slot-value p1 'y)))) ",
        expect,
    );
}

#[test]
fn divergence_eieio_no_applicable_method() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (cl-no-applicable-method)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (defclass test-base-xxx () ())
  (cl-defgeneric test-generic-xxx (obj) \"Generic.\")
  (let ((o (test-base-xxx \"o\")))
    (condition-case err
        (test-generic-xxx o)
      (error (list (car err)))))) ",
        expect,
    );
}

#[test]
fn divergence_eieio_clone() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 2 1 2 nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (defclass test-pair-xxx ()
    ((a :initarg :a :initform 0)
     (b :initarg :b :initform 0)))
  (let* ((p1 (test-pair-xxx \"p\" :a 1 :b 2))
         (p2 (clone p1)))
    (list (slot-value p1 'a) (slot-value p1 'b)
          (slot-value p2 'a) (slot-value p2 'b)
          (eq p1 p2)
          (equal (eieio-object-class-name p1)
                 (eieio-object-class-name p2))))) ",
        expect,
    );
}

#[test]
fn divergence_eieio_object_print() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t 3)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (defclass test-printable-xxx ()
    ((val :initarg :val)))
  (let ((o (test-printable-xxx \"o\" :val 42)))
    (list (stringp (format \"%s\" o))
          (string-match \"test-printable\" (format \"%s\" o))))) ",
        expect,
    );
}

#[test]
fn divergence_cl_defmethod_before_after() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (11 (before primary after) 11)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (defvar test-method-log-xxx nil)
  (defclass test-ma-xxx () ((v :initarg :v :initform 0)))
  (cl-defmethod test-inc-xxx ((obj test-ma-xxx))
    (push 'primary test-method-log-xxx)
    (oset obj v (1+ (slot-value obj 'v)))
    (slot-value obj 'v))
  (cl-defmethod test-inc-xxx :before ((obj test-ma-xxx))
    (push 'before test-method-log-xxx))
  (cl-defmethod test-inc-xxx :after ((obj test-ma-xxx))
    (push 'after test-method-log-xxx))
  (let ((o (test-ma-xxx \"o\" :v 10)))
    (let ((result (test-inc-xxx o)))
      (list result
            (nreverse test-method-log-xxx)
            (slot-value o 'v))))) ",
        expect,
    );
}

#[test]
fn divergence_eieio_initform_evaluation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (2 3 3)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (defvar test-init-counter-xxx 0)
  (defclass test-init-xxx ()
    ((count :initform (progn (cl-incf test-init-counter-xxx)
                             test-init-counter-xxx))))
  (let ((o1 (test-init-xxx \"o1\"))
        (o2 (test-init-xxx \"o2\")))
    (list (slot-value o1 'count)
          (slot-value o2 'count)
          test-init-counter-xxx))) ",
        expect,
    );
}

#[test]
fn divergence_cl_defgeneric_docstring() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK (\"A documented generic function.\\n\\n(fn X)\" nil)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (cl-defgeneric test-doc-fn-xxx (x)
    \"A documented generic function.\")
  (list (documentation 'test-doc-fn-xxx)
        (string= (documentation 'test-doc-fn-xxx)
                 \"A documented generic function.\"))) ",
        expect,
    );
}

#[test]
fn divergence_eieio_multiple_inheritance() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 2 12 t t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (defclass test-mix-a-xxx () ((a :initarg :a)))
  (defclass test-mix-b-xxx () ((b :initarg :b)))
  (defclass test-mix-ab-xxx (test-mix-a-xxx test-mix-b-xxx)
    ((ab :initarg :ab)))
  (let ((o (test-mix-ab-xxx \"o\" :a 1 :b 2 :ab 12)))
    (list (slot-value o 'a)
          (slot-value o 'b)
          (slot-value o 'ab)
          (child-of-class-p (eieio-object-class o) 'test-mix-a-xxx)
          (child-of-class-p (eieio-object-class o) 'test-mix-b-xxx)))) ",
        expect,
    );
}
