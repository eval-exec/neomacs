//! Divergence tests: cl-defgeneric/cl-defmethod (EIEIO) and generic functions.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_cl_defgeneric_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (10 \"HELLO\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(require 'eieio)
(cl-defgeneric my-generic-test (x)
  "A test generic function.")
(cl-defmethod my-generic-test ((x number))
  (* x 2))
(cl-defmethod my-generic-test ((x string))
  (upcase x))
(list (my-generic-test 5)
      (my-generic-test "hello"))"#,
        expect,
    );
}

#[test]
fn divergence_cl_defmethod_specializers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((integer 42) (float 3.14) (other \"hi\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(require 'eieio)
(cl-defgeneric my-spec-test (x)
  "Test")
(cl-defmethod my-spec-test ((x integer))
  (list 'integer x))
(cl-defmethod my-spec-test ((x float))
  (list 'float x))
(cl-defmethod my-spec-test (x)
  (list 'other x))
(list (my-spec-test 42)
      (my-spec-test 3.14)
      (my-spec-test "hi"))"#,
        expect,
    );
}

#[test]
fn divergence_cl_defclass_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-defclass)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(require 'eieio)
(cl-defclass my-test-class ()
  ((name :initarg :name :accessor my-test-name)
   (value :initarg :value :initform 0)))
(let ((obj (my-test-class :name "test" :value 42)))
  (list (my-test-name obj)
        (slot-value obj 'value)
        (object-of-class-p obj 'my-test-class)
        (eieio-object-p obj)))"#,
        expect,
    );
}

#[test]
fn divergence_cl_defclass_inheritance() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-defclass)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(require 'eieio)
(cl-defclass my-base-class ()
  ((base-slot :initarg :base-slot :initform 'base)))
(cl-defclass my-derived-class (my-base-class)
  ((derived-slot :initarg :derived-slot :initform 'derived)))
(let ((obj (my-derived-class :base-slot 'a :derived-slot 'b)))
  (list (slot-value obj 'base-slot)
        (slot-value obj 'derived-slot)
        (child-of-class-p (eieio-object-class obj) 'my-base-class)
        (object-of-class-p obj 'my-base-class)))"#,
        expect,
    );
}

#[test]
fn divergence_eieio_generic_call_next() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (zero nonzero)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(require 'eieio)
(cl-defgeneric my-next-test (x))
(cl-defmethod my-next-test ((x number))
  (if (= x 0) 'zero 'nonzero))
(list (my-next-test 0)
      (my-next-test 5))"#,
        expect,
    );
}

#[test]
fn divergence_eieio_slot_accessors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-defclass)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(require 'eieio)
(cl-defclass my-accessor-class ()
  ((data :initarg :data :accessor my-data
         :type list :initform nil)))
(let ((obj (my-accessor-class :data '(1 2 3))))
  (setf (my-data obj) '(a b c))
  (list (my-data obj)
        (slot-value obj 'data)))"#,
        expect,
    );
}

#[test]
fn divergence_eieio_class_allocated() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-defclass)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(require 'eieio)
(cl-defclass my-class-alloc ()
  ((shared :allocation :class :initform 0)))
(let ((a (my-class-alloc))
      (b (my-class-alloc)))
  (setf (slot-value a 'shared) 42)
  (list (slot-value a 'shared)
        (slot-value b 'shared)))"#,
        expect,
    );
}

#[test]
fn divergence_cl_print_object() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-defclass)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(require 'eieio)
(cl-defclass my-print-class ()
  ((val :initarg :val :initform 0)))
(let ((obj (my-print-class :val 42)))
  (list (stringp (format "%s" obj))
        (stringp (prin1-to-string obj))))"#,
        expect,
    );
}
