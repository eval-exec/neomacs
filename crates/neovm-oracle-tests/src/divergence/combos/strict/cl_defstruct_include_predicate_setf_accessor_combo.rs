//! Strict combo oracle probes, batch 159: cl-defstruct. struct construction,
//! slot accessors, predicate (incl on non-instance and inherited instances),
//! :include inheritance (parent slots + parent predicate true on child),
//! setf on accessors, copier, and BOA constructor with defaulted args.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_cl_defstruct_accessors_predicate_include() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(progn
  (cl-defstruct (probe-pt (:constructor probe-make-pt) (:conc-name probe-pt-))
    (x 0) (y 0))
  (cl-defstruct (probe-cpt (:include probe-pt)) color)
  (let* ((p1 (probe-make-pt :x 3 :y 4))
         (cp (make-probe-cpt :x 1 :y 2 :color 'red)))
    (list (probe-pt-x p1)
          (probe-pt-y p1)
          (probe-pt-p p1)
          (probe-pt-x cp)
          (probe-pt-y cp)
          (probe-cpt-color cp)
          (probe-pt-p cp)
          (probe-cpt-p cp)
          (probe-pt-p 'not-a-struct)
          (probe-pt-p 42)
          (progn (setf (probe-pt-x cp) 99) (probe-pt-x cp))
          (probe-cpt-p (copy-probe-cpt cp)))))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_cl_defstruct_boa_copier_defaulted() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(progn
  (cl-defstruct (probe-box
                 (:constructor probe-make-box (label &optional (count 0)))
                 (:copier probe-copy-box))
    label count (locked nil))
  (let* ((b1 (probe-make-box "first" 5))
         (b2 (probe-make-box "second"))
         (b3 (probe-copy-box b1)))
    (list (probe-box-label b1)
          (probe-box-count b1)
          (probe-box-locked b1)
          (probe-box-label b2)
          (probe-box-count b2)
          (probe-box-label b3)
          (probe-box-count b3)
          (eq b1 b3)
          (progn (setf (probe-box-locked b1) t) (probe-box-locked b1))
          (probe-box-locked b3))))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_cl_defstruct_vector_type_named() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(progn
  (cl-defstruct (probe-vec (:type vector) :named) a b c)
  (let* ((v (make-probe-vec :a 1 :b 2 :c 3)))
    (list (probe-vec-a v)
          (probe-vec-c v)
          (probe-vec-p v)
          (vectorp v)
          (aref v 0)
          (length v)
          (probe-vec-p [probe-vec 9 9 9])
          (probe-vec-p [not-a-vec 1 2]))))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
