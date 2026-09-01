//! Divergence tests: complex macro + EIEIO + generic function combinations.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_macro_generates_defclass() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable \\`)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (defmacro test-def-entity-xxx (name slots)
    \\`(defclass ,(intern (concat \"test-entity-\" (symbol-name name) \"-xxx\")) ()
       ,(mapcar (lambda (s) (list s :initarg (intern (concat \":\" (symbol-name s)))
                                  :accessor (intern (concat \"test-entity-\" (symbol-name name) \"-\" (symbol-name s) \"-xxx\"))))
                slots)))
  (test-def-entity-xxx person (name age))
  (let ((p (test-entity-person-xxx \"p\" :name \"Bob\" :age 25)))
    (list (test-entity-person-name-xxx p)
          (test-entity-person-age-xxx p)
          (eieio-object-class-name p)))) ", expect);
}

#[test]
fn divergence_defclass_with_generic_method_dispatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"car speed=100 doors=2\" \"bike speed=30 gears=21\" 0 0)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (defclass test-vehicle-xxx () ((speed :initarg :speed :initform 0)))
  (defclass test-car-xxx (test-vehicle-xxx) ((doors :initarg :doors :initform 4)))
  (defclass test-bike-xxx (test-vehicle-xxx) ((gears :initarg :gears :initform 10)))
  (cl-defgeneric test-describe-xxx (v) \"Describe a vehicle.\")
  (cl-defmethod test-describe-xxx ((v test-car-xxx))
    (format \"car speed=%d doors=%d\" (slot-value v 'speed) (slot-value v 'doors)))
  (cl-defmethod test-describe-xxx ((v test-bike-xxx))
    (format \"bike speed=%d gears=%d\" (slot-value v 'speed) (slot-value v 'gears)))
  (let ((c (test-car-xxx \"c\" :speed 100 :doors 2))
        (b (test-bike-xxx \"b\" :speed 30 :gears 21)))
    (list (test-describe-xxx c)
          (test-describe-xxx b)
          (string-match \"car\" (test-describe-xxx c))
          (string-match \"bike\" (test-describe-xxx b))))) ",
        expect,
    );
}

#[test]
fn divergence_defclass_initform_uses_macro() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable \\`)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (defmacro test-double-xxx (x) \\`(* 2 ,x))
  (defclass test-init-xxx ()
    ((base :initarg :base :initform 5)
     (doubled :initform (test-double-xxx 10))))
  (let ((o (test-init-xxx \"o\")))
    (list (slot-value o 'base)
          (slot-value o 'doubled)))) ",
        expect,
    );
}

#[test]
fn divergence_eieio_accessor_via_setf() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (30 25 750)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (defclass test-rect-xxx ()
    ((width :initarg :width :accessor test-rect-width-xxx :initform 0)
     (height :initarg :height :accessor test-rect-height-xxx :initform 0)))
  (let ((r (test-rect-xxx \"r\" :width 10 :height 20)))
    (setf (test-rect-width-xxx r) 30)
    (cl-incf (test-rect-height-xxx r) 5)
    (list (test-rect-width-xxx r)
          (test-rect-height-xxx r)
          (* (slot-value r 'width) (slot-value r 'height))))) ",
        expect,
    );
}

#[test]
fn divergence_multiple_inheritance_dispatch_order() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-class-parents)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (defclass test-base-a-xxx () ((a-slot :initarg :a-slot)))
  (defclass test-base-b-xxx () ((b-slot :initarg :b-slot)))
  (defclass test-combo-xxx (test-base-a-xxx test-base-b-xxx) ((c-slot :initarg :c-slot)))
  (let ((o (test-combo-xxx \"o\" :a-slot 1 :b-slot 2 :c-slot 3)))
    (list (slot-value o 'a-slot)
          (slot-value o 'b-slot)
          (slot-value o 'c-slot)
          (cl-class-parents (eieio-object-class o))
          (child-of-class-p (eieio-object-class o) 'test-base-a-xxx)
          (child-of-class-p (eieio-object-class o) 'test-base-b-xxx)))) ",
        expect,
    );
}

#[test]
fn divergence_cl_defmethod_before_after_around() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (11 (before primary after))""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (defvar test-baa-log-xxx nil)
  (defclass test-baa-xxx () ((val :initarg :val :initform 0)))
  (cl-defmethod test-incr-xxx ((obj test-baa-xxx))
    (push 'primary test-baa-log-xxx)
    (oset obj val (1+ (slot-value obj 'val))))
  (cl-defmethod test-incr-xxx :before ((obj test-baa-xxx))
    (push 'before test-baa-log-xxx))
  (cl-defmethod test-incr-xxx :after ((obj test-baa-xxx))
    (push 'after test-baa-log-xxx))
  (let ((o (test-baa-xxx \"o\" :val 10)))
    (test-incr-xxx o)
    (list (slot-value o 'val)
          (nreverse test-baa-log-xxx)))) ",
        expect,
    );
}

#[test]
fn divergence_macro_generates_defun_with_closure() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable \\`)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (defmacro test-make-adder-xxx (n)
    \\`(lambda (x) (+ x ,n)))
  (let ((add5 (test-make-adder-xxx 5))
        (add10 (test-make-adder-xxx 10)))
    (list (funcall add5 3)
          (funcall add10 3)
          (funcall add5 100)
          (funcall add10 100)))) ",
        expect,
    );
}

#[test]
fn divergence_eieio_slot_default_and_initarg_interaction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((1 2 3) (10 2 3) (100 200 300))""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (defclass test-defaults-xxx ()
    ((a :initarg :a :initform 1)
     (b :initarg :b :initform 2)
     (c :initarg :c :initform (progn \"computed\" 3))))
  (let ((o1 (test-defaults-xxx \"o1\"))
        (o2 (test-defaults-xxx \"o2\" :a 10))
        (o3 (test-defaults-xxx \"o3\" :a 100 :b 200 :c 300)))
    (list (list (slot-value o1 'a) (slot-value o1 'b) (slot-value o1 'c))
          (list (slot-value o2 'a) (slot-value o2 'b) (slot-value o2 'c))
          (list (slot-value o3 'a) (slot-value o3 'b) (slot-value o3 'c))))) ",
        expect,
    );
}

#[test]
fn divergence_defclass_print_function() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (3 22)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (defclass test-printable-xxx ()
    ((label :initarg :label :initform \"\"))
    \"A printable class.\")
  (cl-defmethod cl-print-object ((obj test-printable-xxx) stream)
    (princ (format \"#<test-printable label=%s>\" (slot-value obj 'label)) stream)
    obj)
  (let ((o (test-printable-xxx \"o\" :label \"hello\")))
    (list (string-match \"test-printable\" (format \"%s\" o))
          (string-match \"hello\" (format \"%s\" o))))) ",
        expect,
    );
}

#[test]
fn divergence_defclass_with_type_checking() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function typep)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (defclass test-typed-xxx ()
    ((name :initarg :name :type string :initform \"default\")
     (count :initarg :count :type integer :initform 0)))
  (let ((o (test-typed-xxx \"o\" :name \"test\" :count 5)))
    (list (slot-value o 'name)
          (slot-value o 'count)
          (typep (slot-value o 'name) 'string)
          (typep (slot-value o 'count) 'integer)))) ",
        expect,
    );
}
