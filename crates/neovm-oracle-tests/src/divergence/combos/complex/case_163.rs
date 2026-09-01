//! Complex combo batch 163 — `cl-struct` / `record` / `type-of` /
//! `cl-typep` with custom `cl-defstruct` types, predicate chains.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx163_cl_defstruct_type_of_returns_tag() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(cl-defstruct (neo-cx163-tagged (:type vector) :named) a b c)
(let ((r (make-neo-cx163-tagged :a 1 :b 2 :c 3)))
  (list (neo-cx163-tagged-p r)
        (type-of r)
        (aref r 0)
        (aref r 1)
        (aref r 2)
        (aref r 3)
        (length r)))
"##,
        expect,
    );
}

#[test]
fn div_cx163_cl_typep_with_custom_defstruct() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(cl-defstruct neo-cx163-thing name value)
(let ((r (make-neo-cx163-thing :name "alpha" :value 42)))
  (list (cl-typep r 'neo-cx163-thing)
        (cl-typep r 'vector)
        (cl-typep r 'cons)
        (cl-typep r '(or neo-cx163-thing integer))
        (cl-typep r '(and neo-cx163-thing (satisfies neo-cx163-thing-p)))))
"##,
        expect,
    );
}

#[test]
fn div_cx163_record_type_of_returns_record() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function record-p)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((rec (record 'neo-cx163-rec-tag :a :b :c)))
  (list (record-p rec)
        (type-of rec)
        (record-type rec)
        (record-length rec)
        (aref rec 0)
        (aref rec 1)
        (aref rec 2)))
"##,
        expect,
    );
}

#[test]
fn div_cx163_cl_defstruct_with_included_slots_inheritance() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(cl-defstruct (neo-cx163-animal
               (:conc-name neo-cx163-animal-))
  name sound)
(cl-defstruct (neo-cx163-dog
               (:include neo-cx163-animal)
               (:conc-name neo-cx163-dog-))
  breed)
(let ((a (make-neo-cx163-animal :name "Generic" :sound "..."))
      (d (make-neo-cx163-dog :name "Rex" :sound "Woof" :breed "Lab")))
  (list (neo-cx163-animal-name a)
        (neo-cx163-animal-sound a)
        (neo-cx163-animal-name d)
        (neo-cx163-animal-sound d)
        (neo-cx163-dog-breed d)
        (neo-cx163-animal-p d)
        (neo-cx163-dog-p a)))
"##,
        expect,
    );
}

#[test]
fn div_cx163_cl_defstruct_copier_independence() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(cl-defstruct neo-cx163-copy name value)
(let* ((orig (make-neo-cx163-copy :name "alpha" :value 1))
       (copy (copy-neo-cx163-copy orig)))
  (setf (neo-cx163-copy-name copy) "modified")
  (list (neo-cx163-copy-name orig)
        (neo-cx163-copy-name copy)
        (eq orig copy)
        (equal orig copy)))
"##,
        expect,
    );
}

#[test]
fn div_cx163_cl_defstruct_read_only_slot_rejects_setf() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(cl-defstruct neo-cx163-readonly
  (val 0 :read-only t)
  (mutable 1))
(let ((r (make-neo-cx163-readonly :val 99 :mutable 42)))
  (list (neo-cx163-readonly-val r)
        (neo-cx163-readonly-mutable r)
        (setf (neo-cx163-readonly-mutable r) 100)
        (neo-cx163-readonly-mutable r)
        (condition-case e (setf (neo-cx163-readonly-val r) 100) (error (car e)))))
"##,
        expect,
    );
}

#[test]
fn div_cx163_cl_defstruct_with_named_predicate_strict() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(cl-defstruct (neo-cx163-strict (:type list) :named) a b)
(let ((r (make-neo-cx163-strict :a 1 :b 2)))
  (list (neo-cx163-strict-p r)
        (neo-cx163-strict-p '(neo-cx163-strict 1 2))
        (neo-cx163-strict-p '(1 2))
        (neo-cx163-strict-p [1 2])
        (neo-cx163-strict-p nil)))
"##,
        expect,
    );
}

#[test]
fn div_cx163_cl_defstruct_anon_type_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(cl-defstruct (neo-cx163-anon (:type list)) a b)
(let ((r (make-neo-cx163-anon :a 1 :b 2)))
  (list r
        (neo-cx163-anon-a r)
        (neo-cx163-anon-b r)
        (eq (car r) 1)
        (eq (cadr r) 2)
        (type-of r)))
"##,
        expect,
    );
}

#[test]
fn div_cx163_cl_defstruct_anon_type_vector() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(cl-defstruct (neo-cx163-avec (:type vector)) x y z)
(let ((r (make-neo-cx163-avec :x 1 :y 2 :z 3)))
  (list r
        (neo-cx163-avec-x r)
        (neo-cx163-avec-y r)
        (neo-cx163-avec-z r)
        (vectorp r)
        (type-of r)))
"##,
        expect,
    );
}

#[test]
fn div_cx163_record_setf_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((rec (record 'neo-cx163-mega-tag :initial :data)))
  (with-temp-buffer
    (buffer-enable-undo)
    (insert (format "Record: %S" rec))
    (put-text-property 1 6 'face 'bold)
    (let ((m (set-marker (make-marker) 8))
          (ov (make-overlay 4 14)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 18)
      (setf (aref rec 2) :modified)
      (let ((state (list rec
                         (aref rec 0) (aref rec 1) (aref rec 2)
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
