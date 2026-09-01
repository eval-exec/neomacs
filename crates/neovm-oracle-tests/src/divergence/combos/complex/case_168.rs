//! Complex combo batch 168 — `cl-lib` deep `cl-defstruct` records with
//! `:predicate` and `:copier` named options, `cl-defstruct` with
//! `:constructor` multiple variants.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx168_cl_defstruct_named_predicate_explicit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(cl-defstruct (neo-cx168-pred
               (:predicate neo-cx168-is-pred?))
  a b c)
(let ((r (make-neo-cx168-pred :a 1 :b 2 :c 3)))
  (list (neo-cx168-is-pred? r)
        (neo-cx168-is-pred? '(1 2 3))
        (neo-cx168-is-pred? [1 2 3])
        (fboundp 'neo-cx168-pred-p)))
"##,
        expect,
    );
}

#[test]
fn div_cx168_cl_defstruct_named_copier_explicit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(cl-defstruct (neo-cx168-copy
               (:copier neo-cx168-clone-it))
  a b c)
(let* ((orig (make-neo-cx168-copy :a 1 :b 2 :c 3))
       (cloned (neo-cx168-clone-it orig)))
  (setf (neo-cx168-copy-a cloned) 99)
  (list (neo-cx168-copy-a orig)
        (neo-cx168-copy-a cloned)
        (eq orig cloned)))
"##,
        expect,
    );
}

#[test]
fn div_cx168_cl_defstruct_multiple_constructors() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(cl-defstruct (neo-cx168-multi
               (:constructor neo-cx168-make-1 (a b))
               (:constructor neo-cx168-make-2 (a b &optional c)))
  a b c)
(let ((r1 (neo-cx168-make-1 1 2))
      (r2 (neo-cx168-make-2 1 2))
      (r3 (neo-cx168-make-2 1 2 3)))
  (list (neo-cx168-multi-a r1) (neo-cx168-multi-b r1) (neo-cx168-multi-c r1)
        (neo-cx168-multi-a r2) (neo-cx168-multi-b r2) (neo-cx168-multi-c r2)
        (neo-cx168-multi-a r3) (neo-cx168-multi-b r3) (neo-cx168-multi-c r3)))
"##,
        expect,
    );
}

#[test]
fn div_cx168_cl_defstruct_with_no_constructor() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(cl-defstruct (neo-cx168-noctor
               (:constructor nil)
               (:type vector)
               :named)
  a b c)
(let ((r (vector 'neo-cx168-noctor 1 2 3)))
  (list (neo-cx168-noctor-p r)
        (eq (aref r 0) 'neo-cx168-noctor)))
"##,
        expect,
    );
}

#[test]
fn div_cx168_cl_defstruct_slot_initform_evaluation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((counter 0))
  (cl-defstruct (neo-cx168-initform (:type vector) :named)
    (a (cl-incf counter))
    (b (* 2 (cl-incf counter))))
  (let ((r1 (vector 'neo-cx168-initform nil nil)))
    counter))
"##,
        expect,
    );
}

#[test]
fn div_cx168_cl_defstruct_with_documentation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(cl-defstruct (neo-cx168-doc (:conc-name neo-cx168-d-))
  "Struct documentation string."
  (a 0 :documentation "Slot a doc")
  (b nil :documentation "Slot b doc"))
(let ((r (make-ne-cx168-doc :a 1 :b 2)))
  (list (documentation-property 'neo-cx168-doc 'structure-documentation)))
"##,
        expect,
    );
}

#[test]
fn div_cx168_cl_defstruct_inherits_default_initforms() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(cl-defstruct (neo-cx168-base (:conc-name neo-cx168-b-))
  (name "anon" :read-only t)
  (count 0))
(cl-defstruct (neo-cx168-deriv (:include neo-cx168-base)
                               (:conc-name neo-cx168-d-))
  extra)
(let ((b (make-neo-cx168-base))
      (d (make-neo-cx168-deriv)))
  (list (neo-cx168-b-name b)
        (neo-cx168-b-count b)
        (neo-cx168-b-name d)
        (neo-cx168-b-count d)
        (neo-cx168-d-extra d)))
"##,
        expect,
    );
}

#[test]
fn div_cx168_cl_defstruct_setf_chain_through_accessor() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function make-neo-cx168-deriv)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((v (make-vector 5 nil)))
  (dotimes (i 5) (aset v i i))
  (let ((struct-list (mapcar (lambda (n)
                               (make-neo-cx168-deriv :name (format "n%d" n)
                                                       :count n))
                              (append v nil))))
    (setf (neo-cx168-b-count (car struct-list)) 99)
    (list (neo-cx168-b-name (car struct-list))
          (neo-cx168-b-count (car struct-list))
          (neo-cx168-b-count (cadr struct-list))
          (length struct-list))))
"##,
        expect,
    );
}

#[test]
fn div_cx168_cl_defstruct_with_named_via_type_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(cl-defstruct (neo-cx168-named-list (:type list) :named)
  a b c)
(let ((r (make-neo-cx168-named-list :a 1 :b 2 :c 3)))
  (list r
        (neo-cx168-named-list-p r)
        (neo-cx168-named-list-a r)
        (eq (car r) 'neo-cx168-named-list)))
"##,
        expect,
    );
}

#[test]
fn div_cx168_cl_defstruct_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(cl-defstruct (neo-cx168-mega (:type vector) :named)
  a b c)
(let ((r (make-neo-cx168-mega :a 1 :b 2 :c 3)))
  (with-temp-buffer
    (buffer-enable-undo)
    (insert (format "Record mega: %S" r))
    (put-text-property 1 6 'face 'bold)
    (let ((m (set-marker (make-marker) 8))
          (ov (make-overlay 4 14)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 18)
      (setf (aref r 1) 99)
      (let ((state (list r (neo-cx168-mega-p r)
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
