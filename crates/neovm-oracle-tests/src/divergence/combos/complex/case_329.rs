//! Complex combo batch 329 — `cl-defstruct` ultimate: all options
//! (constructor/copier/predicate/conc-name/type/named), slot :read-only,
//! :documentation, inheritance with :include, setf chain through accessor.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx329_cl_defstruct_full_options_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(cl-defstruct (neo-cx329-full
               (:constructor neo-cx329-make (a b))
               (:constructor neo-cx329-new (a b &optional c))
               (:copier neo-cx329-copy)
               (:predicate neo-cx329-is?)
               (:conc-name neo-cx329-f-)
               (:type vector)
               :named)
  a b c)
(let ((r1 (neo-cx329-make 1 2))
      (r2 (neo-cx329-new 1 2 3)))
  (list (neo-cx329-is? r1)
        (neo-cx329-is? r2)
        (neo-cx329-is? [1 2])
        (neo-cx329-f-a r1) (neo-cx329-f-b r1) (neo-cx329-f-c r1)
        (neo-cx329-f-a r2) (neo-cx329-f-b r2) (neo-cx329-f-c r2)
        (let ((c (neo-cx329-copy r2)))
          (setf (neo-cx329-f-a c) 99)
          (list (neo-cx329-f-a c) (neo-cx329-f-a r2)))))
"##,
        expect,
    )
}

#[test]
fn div_cx329_cl_defstruct_inheritance_with_extra_slots() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(cl-defstruct (neo-cx329-animal (:conc-name neo-cx329-an-)) name sound)
(cl-defstruct (neo-cx329-dog (:include neo-cx329-animal)
                            (:conc-name neo-cx329-dog-)) breed)
(let ((d (make-neo-cx329-dog :name "Rex" :sound "Woof" :breed "Lab"))
      (a (make-neo-cx329-animal :name "Generic" :sound "...")))
  (list (neo-cx329-an-name d) (neo-cx329-an-sound d) (neo-cx329-dog-breed d)
        (neo-cx329-an-name a) (neo-cx329-an-sound a)
        (neo-cx329-animal-p d) (neo-cx329-dog-p a)))
"##,
        expect,
    )
}

#[test]
fn div_cx329_cl_defstruct_no_constructor_anonymous() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(cl-defstruct (neo-cx329-noc (:constructor nil) (:type vector) :named) x y)
(let ((r (vector 'neo-cx329-noc 1 2)))
  (list (neo-cx329-noc-p r) (aref r 0) (aref r 1) (aref r 2)))
"##,
        expect,
    )
}

#[test]
fn div_cx329_cl_defstruct_read_only_and_mutable_slots() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(cl-defstruct neo-cx329-ro (id 0 :read-only t) (mutable 0))
(let ((r (make-neo-cx329-ro :id 99 :mutable 1)))
  (list (neo-cx329-ro-id r)
        (neo-cx329-ro-mutable r)
        (setf (neo-cx329-ro-mutable r) 100)
        (neo-cx329-ro-mutable r)
        (condition-case e (setf (neo-cx329-ro-id r) 100) (error (car e)))))
"##,
        expect,
    )
}

#[test]
fn div_cx329_cl_defstruct_type_list_anonymous() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(cl-defstruct (neo-cx329-anon (:type list)) a b c)
(let ((r (make-neo-cx329-anon :a 1 :b 2 :c 3)))
  (list r (neo-cx329-anon-a r) (neo-cx329-anon-b r) (neo-cx329-anon-c r)
        (eq (car r) 1) (eq (cadr r) 2) (eq (caddr r) 3) (type-of r)))
"##,
        expect,
    )
}

#[test]
fn div_cx329_cl_defstruct_setf_chain_through_accessor() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function make-neo-cx329-ro)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((v (vector 0 1 2 3 4))
       (structs (mapcar (lambda (n) (make-neo-cx329-ro :id n :mutable (* n 10)))
                        (append v nil))))
  (setf (neo-cx329-ro-mutable (car structs)) 99)
  (setf (neo-cx329-ro-mutable (caddr structs)) 999)
  (list (neo-cx329-ro-mutable (car structs))
        (neo-cx329-ro-mutable (cadr structs))
        (neo-cx329-ro-mutable (caddr structs))
        (length structs)))
"##,
        expect,
    )
}

#[test]
fn div_cx329_cl_defstruct_equal_vs_eq() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(cl-defstruct neo-cx329-eq name value)
(let ((r1 (make-neo-cx329-eq :name "alpha" :value 1))
      (r2 (make-neo-cx329-eq :name "alpha" :value 1)))
  (list (eq r1 r2) (equal r1 r2)
        (equal (neo-cx329-eq-name r1) (neo-cx329-eq-name r2))))
"##,
        expect,
    )
}

#[test]
fn div_cx329_cl_defstruct_copier_independence() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(cl-defstruct neo-cx329-indep a b)
(let* ((orig (make-neo-cx329-indep :a 1 :b 2))
       (copy (copy-neo-cx329-indep orig)))
  (setf (neo-cx329-indep-a copy) 99)
  (list (neo-cx329-indep-a orig)
        (neo-cx329-indep-a copy)
        (eq orig copy)
        (equal orig copy)))
"##,
        expect,
    )
}

#[test]
fn div_cx329_cl_defstruct_slot_documentation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (cl-defstruct neo-cx329-documented
        "Struct doc."
        (name "init" :documentation "Name slot doc.")
        (value 0 :documentation "Value slot doc."))
      (let ((r (make-neo-cx329-documented :name "alpha" :value 42)))
        (list (neo-cx329-documented-name r) (neo-cx329-documented-value r))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx329_cl_defstruct_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(cl-defstruct (neo-cx329-mega (:type vector) :named) a b c d)
(let ((r (make-neo-cx329-mega :a 1 :b 2 :c 3 :d 4)))
  (with-temp-buffer
    (buffer-enable-undo)
    (insert (format "Struct mega: %S" r))
    (put-text-property 1 6 'face 'bold)
    (let ((m (set-marker (make-marker) 8))
          (ov (make-overlay 4 18)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 25)
      (setf (aref r 2) 99)
      (let ((state (list r (neo-cx329-mega-p r) (length r)
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
    )
}
