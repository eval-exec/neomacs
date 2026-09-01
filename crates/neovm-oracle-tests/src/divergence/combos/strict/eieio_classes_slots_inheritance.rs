//! Strict combo oracle probes, batch 27: EIEIO — class definition, slots,
//! initforms, oref/oset/slot-value/slot-boundp, single and multiple
//! inheritance, class/object predicates, and :class slot allocation.
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_g2_eieio_basic_slots() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (10 \"y\" 1 \"x\" 10 t t 99)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (defclass probe-eieio-1 ()
    ((a :initarg :a :initform 1 :type integer)
     (b :initarg :b :initform "x")))
  (let ((o (probe-eieio-1 :a 10 :b "y")))
    (list (oref o a)
          (oref o b)
          (oref-default o a)
          (oref-default o b)
          (slot-value o 'a)
          (object-of-class-p o 'probe-eieio-1)
          (slot-boundp o 'a)
          (progn (oset o a 99) (oref o a)))))
"##,
        expect,
    );
}

#[test]
fn div_g2_eieio_inheritance() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (5 6 t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (defclass probe-eieio-base () ((p :initarg :p :initform 0)))
  (defclass probe-eieio-child (probe-eieio-base) ((c :initarg :c :initform 1)))
  (let ((o (probe-eieio-child :p 5 :c 6)))
    (list (oref o p)
          (oref o c)
          (object-of-class-p o 'probe-eieio-base)
          (object-of-class-p o 'probe-eieio-child)
          (child-of-class-p 'probe-eieio-child 'probe-eieio-base)
          (eq (eieio-object-class o) 'probe-eieio-child))))
"##,
        expect,
    );
}

#[test]
fn div_g2_eieio_initforms() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 (1 2 3) 3 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (defclass probe-eieio-2 ()
    ((a :initform (+ 1 2))
     (b :initform (list 1 2 3))
     (c :initform nil)))
  (let ((o (probe-eieio-2)))
    (list (oref o a)
          (oref o b)
          (length (oref o b))
          (slot-boundp o 'c))))
"##,
        expect,
    );
}

#[test]
fn div_g2_eieio_predicates_and_class() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (defclass probe-eieio-3 () ((name :initarg :name :initform "default")))
  (let ((o (probe-eieio-3 :name "probe")))
    (list (eieio-object-p o)
          (eq (eieio-object-class o) 'probe-eieio-3)
          (slot-boundp o 'name)
          (class-p 'probe-eieio-3)
          (cl-typep o 'probe-eieio-3))))
"##,
        expect,
    );
}

#[test]
fn div_g2_eieio_class_slot_allocation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (5 5 5)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (defclass probe-eieio-4 ()
    ((shared :allocation :class :initarg :shared :initform 0)))
  (let ((o1 (probe-eieio-4))
        (o2 (probe-eieio-4)))
    (oset o1 shared 5)
    (list (oref o1 shared)
          (oref o2 shared)
          (oref-default o1 shared))))
"##,
        expect,
    );
}
