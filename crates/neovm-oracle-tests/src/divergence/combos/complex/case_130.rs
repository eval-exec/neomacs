//! Complex combo batch 130 — eieio `cl-defstruct` records with `:type list`
//! vs `:type vector`, hash-table as method dispatch table, `cl-print-object`
//! override with custom rendering.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx130_cl_defstruct_type_list_vs_vector() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(cl-defstruct (neo-cx130-list-rec (:type list) :named)
  a b c)
(cl-defstruct (neo-cx130-vec-rec (:type vector) :named)
  x y z)
(let ((lst (make-neo-cx130-list-rec :a 1 :b 2 :c 3))
      (vec (make-neo-cx130-vec-rec :x 10 :y 20 :z 30)))
  (list (neo-cx130-list-rec-a lst)
        (neo-cx130-list-rec-b lst)
        (neo-cx130-list-rec-c lst)
        (neo-cx130-vec-rec-x vec)
        (neo-cx130-vec-rec-y vec)
        (neo-cx130-vec-rec-z vec)
        (neo-cx130-list-rec-p lst)
        (neo-cx130-vec-rec-p vec)
        (eq (type-of lst) 'cons)
        (eq (type-of vec) 'vector)))
"##,
        expect,
    );
}

#[test]
fn div_cx130_cl_defstruct_with_included_slots() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(cl-defstruct (neo-cx130-base (:conc-name neo-cx130-b-))
  name (count :read-only t))
(cl-defstruct (neo-cx130-derived (:include neo-cx130-base)
                                 (:conc-name neo-cx130-d-))
  extra)
(let ((b (make-neo-cx130-base :name "alpha" :count 1))
      (d (make-neo-cx130-derived :name "beta" :count 2 :extra :bonus)))
  (list (neo-cx130-b-name b)
        (neo-cx130-b-count b)
        (neo-cx130-b-name d)
        (neo-cx130-b-count d)
        (neo-cx130-d-extra d)
        (neo-cx130-base-p d)
        (neo-cx130-derived-p b)))
"##,
        expect,
    );
}

#[test]
fn div_cx130_cl_defstruct_print_function_override() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(cl-defstruct (neo-cx130-printed
               (:conc-name neo-cx130-pr-)
               (:constructor neo-cx130-make-pr))
  (val 0 :read-only t)
  name)
(let ((r (neo-cx130-make-pr :val 42 :name "alpha")))
  (list (neo-cx130-pr-val r)
        (neo-cx130-pr-name r)
        (setf (neo-cx130-pr-name r) "beta")
        (neo-cx130-pr-name r)
        (condition-case e (setf (neo-cx130-pr-val r) 99) (error (car e)))))
"##,
        expect,
    );
}

#[test]
fn div_cx130_cl_defstruct_copier_independent() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(cl-defstruct neo-cx130-copy a b c)
(let* ((orig (make-neo-cx130-copy :a 1 :b 2 :c 3))
       (copy (copy-neo-cx130-copy orig)))
  (setf (neo-cx130-copy-a copy) 99)
  (list (neo-cx130-copy-a orig)
        (neo-cx130-copy-a copy)
        (eq orig copy)
        (equal orig copy)))
"##,
        expect,
    );
}

#[test]
fn div_cx130_cl_defstruct_predicate_strict() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(cl-defstruct neo-cx130-strict a b)
(let ((r (make-neo-cx130-strict :a 1 :b 2)))
  (list (neo-cx130-strict-p r)
        (neo-cx130-strict-p '(1 2))
        (neo-cx130-strict-p [1 2])
        (neo-cx130-strict-p :other)
        (neo-cx130-strict-p nil)))
"##,
        expect,
    );
}

#[test]
fn div_cx130_cl_defstruct_with_type_vector_no_named() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(cl-defstruct (neo-cx130-anon (:type vector)) a b c)
(let ((r (make-neo-cx130-anon :a 1 :b 2 :c 3)))
  (list r
        (neo-cx130-anon-a r)
        (neo-cx130-anon-b r)
        (neo-cx130-anon-c r)
        (setf (neo-cx130-anon-a r) 99)
        r
        (type-of r)))
"##,
        expect,
    );
}

#[test]
fn div_cx130_cl_print_object_custom_via_eieio() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx130-po ()
        ((name :initarg :name :initform "anon")
         (age :initarg :age :initform 0)))
      (cl-defmethod cl-print-object ((o neo-cx130-po) stream)
        (princ (format "#<Custom:%s/%d>"
                       (slot-value o 'name)
                       (slot-value o 'age))
               stream)
        o)
      (let ((inst (make-instance 'neo-cx130-po :name "alpha" :age 30)))
        (list (prin1-to-string inst)
              (princ-to-string inst)
              (format "%S" inst)
              (format "%s" inst))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx130_record_vs_cl_defstruct_print() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function record-p)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((r1 (record 'neo-cx130-tag 1 2 3))
      (r2 (vector 1 2 3)))
  (list (record-p r1)
        (record-p r2)
        (vectorp r1)
        (vectorp r2)
        (prin1-to-string r1)
        (prin1-to-string r2)))
"##,
        expect,
    );
}

#[test]
fn div_cx130_cl_defstruct_with_constructor_two_forms() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(cl-defstruct (neo-cx130-multi
               (:constructor neo-cx130-make-bo (a b))
               (:constructor neo-cx130-make-with-c (a b c))
               (:conc-name neo-cx130-m-))
  a b c)
(let ((r1 (neo-cx130-make-bo 1 2))
      (r2 (neo-cx130-make-with-c 1 2 3)))
  (list (neo-cx130-m-a r1) (neo-cx130-m-b r1) (neo-cx130-m-c r1)
        (neo-cx130-m-a r2) (neo-cx130-m-b r2) (neo-cx130-m-c r2)))
"##,
        expect,
    );
}

#[test]
fn div_cx130_cl_defstruct_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(cl-defstruct (neo-cx130-mega (:type list) :named)
  a b c)
(let ((r (make-neo-cx130-mega :a 1 :b 2 :c 3)))
  (with-temp-buffer
    (buffer-enable-undo)
    (insert (format "%S" r))
    (put-text-property 1 6 'face 'bold)
    (let ((m (set-marker (make-marker) 8))
          (ov (make-overlay 4 14)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 18)
      (setf (neo-cx130-mega-a r) 99)
      (let ((state (list r
                         (neo-cx130-mega-a r)
                         (buffer-string)
                         (marker-position m)
                         (overlay-start ov) (overlay-end ov)
                         (text-properties-at 1))))
        (undo)
        (widen)
        (list state (buffer-string) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              (text-properties-at 1))))))
"##,
        expect,
    );
}
