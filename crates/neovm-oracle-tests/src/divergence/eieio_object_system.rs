//! EIEIO/CLOS object-system divergence probes (calibration).
//!
//! Probes defclass, slots (initform/initarg/type/accessor/reader/writer),
//! make-instance, oref/oset, single + multiple inheritance, cl-defmethod with
//! qualifiers (:primary/:before/:after/:around/:static), call-next-method,
//! :allocation :class, slot-boundp, class-of/child-of-class-p/same-class-p.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

// --- defclass, make-instance, slot access -----------------------------------

#[test]
fn div_eo_defclass_make_instance_accessor() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument symbol 'name slot)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (defclass neo-animal ()
    ((name :initarg :name :initform "anon" :accessor animal-name)))
  (let ((a (neo-animal :name "rex")))
    (list (animal-name a) (oref a name) (oref-default 'neo-animal 'name))))
"##,
        expect,
    );
}

#[test]
fn div_eo_initform_evaluation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument symbol 'count slot)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (defclass neo-ifm () ((count :initarg :count :initform 0)))
  (list (oref (neo-ifm) count)
        (oref-default 'neo-ifm 'count)))
"##,
        expect,
    );
}

#[test]
fn div_eo_reader_writer_accessor() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (cl-no-applicable-method set-s 9 #s(neo-rwa 5))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (defclass neo-rwa ()
    ((s :initarg :s :reader get-s :writer set-s :accessor acc-s)))
  (let ((o (neo-rwa :s 5)))
    (set-s 9 o)
    (list (get-s o) (acc-s o))))
"##,
        expect,
    );
}

#[test]
fn div_eo_slot_boundp_unbound() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (defclass neo-sb () ((s :initarg :s)))
  (let ((o (neo-sb)))
    (list (slot-boundp o 's)
          (progn (oset o s 7) (slot-boundp o 's)))))
"##,
        expect,
    );
}

#[test]
fn div_eo_slot_type_validation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (invalid-slot-type 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (defclass neo-ty () ((n :type number :initarg :n :initform 0)))
  (list (condition-case err (neo-ty :n "string") (error (car err)))
        (oref (neo-ty :n 3) n)))
"##,
        expect,
    );
}

// --- inheritance ------------------------------------------------------------

#[test]
fn div_eo_single_inheritance_slots() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (10 20 t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (defclass neo-base () ((a :initarg :a :initform 1)))
  (defclass neo-sub (neo-base) ((b :initarg :b :initform 2)))
  (let ((o (neo-sub :a 10 :b 20)))
    (list (oref o a) (oref o b)
          (object-of-class-p o 'neo-base)
          (child-of-class-p 'neo-sub 'neo-base))))
"##,
        expect,
    );
}

#[test]
fn div_eo_multiple_inheritance_slots() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 2 t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (defclass neo-m1 () ((x :initarg :x :initform 0)))
  (defclass neo-m2 () ((y :initarg :y :initform 0)))
  (defclass neo-multi (neo-m1 neo-m2) ())
  (let ((o (neo-multi :x 1 :y 2)))
    (list (oref o x) (oref o y)
          (object-of-class-p o 'neo-m1)
          (object-of-class-p o 'neo-m2))))
"##,
        expect,
    );
}

#[test]
fn div_eo_initform_inheritance_override() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:parent :child)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (defclass neo-pi () ((s :initarg :s :initform :parent)))
  (defclass neo-ci (neo-pi) ((s :initarg :s :initform :child)))
  (list (oref (neo-pi) s) (oref (neo-ci) s)))
"##,
        expect,
    );
}

#[test]
fn div_eo_class_identity_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (defclass neo-id () ())
  (defclass neo-id2 (neo-id) ())
  (let ((o (neo-id2)))
    (list (same-class-p o 'neo-id2)
          (same-class-p o 'neo-id)
          (object-of-class-p o 'neo-id)
          (child-of-class-p 'neo-id2 'neo-id))))
"##,
        expect,
    );
}

#[test]
fn div_eo_class_of_and_find_class() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (neo-co t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (defclass neo-co () ())
  (let ((o (neo-co)))
    (list (eieio-object-class o)
          (eq (eieio-object-class o) (eieio-object-class (neo-co))))))
"##,
        expect,
    );
}

// --- cl-defmethod dispatch & qualifiers -------------------------------------

#[test]
fn div_eo_method_primary_dispatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (shape circle)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (defclass neo-shape () ())
  (defclass neo-circle (neo-shape) ())
  (cl-defgeneric neo-area (obj))
  (cl-defmethod neo-area ((obj neo-shape)) 'shape)
  (cl-defmethod neo-area ((obj neo-circle)) 'circle)
  (list (neo-area (neo-shape)) (neo-area (neo-circle))))
"##,
        expect,
    );
}

#[test]
fn div_eo_method_no_specializer_default() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (specialized unspecialized)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (defclass neo-cd () ())
  (cl-defgeneric neo-gn (obj))
  (cl-defmethod neo-gn ((obj neo-cd)) 'specialized)
  (cl-defmethod neo-gn (obj) 'unspecialized)
  (list (neo-gn (neo-cd)) (neo-gn 42)))
"##,
        expect,
    );
}

#[test]
fn div_eo_method_before_after_order() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:before :primary :after)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (defclass neo-ba () ())
  (let (order)
    (cl-defgeneric neo-gba (obj))
    (cl-defmethod neo-gba :before ((obj neo-ba)) (push :before order))
    (cl-defmethod neo-gba ((obj neo-ba)) (push :primary order))
    (cl-defmethod neo-gba :after ((obj neo-ba)) (push :after order))
    (neo-gba (neo-ba))
    (reverse order)))
"##,
        expect,
    );
}

#[test]
fn div_eo_method_around_call_next_method() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:around primary)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (defclass neo-ar () ())
  (cl-defgeneric neo-gar (obj))
  (cl-defmethod neo-gar ((obj neo-ar)) 'primary)
  (cl-defmethod neo-gar :around ((obj neo-ar)) (list :around (cl-call-next-method)))
  (neo-gar (neo-ar)))
"##,
        expect,
    );
}

#[test]
fn div_eo_method_call_next_method_no_next() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function next-method-p)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (defclass neo-nn () ())
  (cl-defgeneric neo-gnn (obj))
  (cl-defmethod neo-gnn ((obj neo-nn))
    (list (next-method-p) (condition-case err (cl-call-next-method) (error (car err)))))
  (neo-gnn (neo-nn)))
"##,
        expect,
    );
}

#[test]
fn div_eo_method_inherited_dispatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK from-base""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (defclass neo-mbase () ())
  (defclass neo-msub (neo-mbase) ())
  (cl-defgeneric neo-gm (obj))
  (cl-defmethod neo-gm ((obj neo-mbase)) 'from-base)
  (neo-gm (neo-msub)))
"##,
        expect,
    );
}

#[test]
fn div_eo_method_around_only_no_primary() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (cl-no-primary-method neo-gao #s(neo-ao))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (defclass neo-ao () ())
  (cl-defgeneric neo-gao (obj))
  (cl-defmethod neo-gao :around ((obj neo-ao)) (list :around (next-method-p)))
  (neo-gao (neo-ao)))
"##,
        expect,
    );
}

#[test]
fn div_eo_static_method() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (cl-no-applicable-method neo-sm-doit neo-sm)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (defclass neo-sm () ())
  (cl-defmethod neo-sm-doit :static ((c neo-sm)) 'static)
  (let ((o (neo-sm)))
    (list (neo-sm-doit 'neo-sm)
          (neo-sm-doit o))))
"##,
        expect,
    );
}

#[test]
fn div_eo_eql_specializer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (class-eql instance)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (defclass neo-eq () ())
  (let ((o (neo-eq)))
    (cl-defgeneric neo-geql (obj))
    (cl-defmethod neo-geql ((obj (eql neo-eq))) 'class-eql)
    (cl-defmethod neo-geql ((obj neo-eq)) 'instance)
    (list (neo-geql 'neo-eq) (neo-geql o))))
"##,
        expect,
    );
}

// --- :allocation :class (shared slots) --------------------------------------

#[test]
fn div_eo_class_allocation_shared() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (5 5)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (defclass neo-ca ()
    ((shared :initform 0 :allocation :class :accessor neo-shared)))
  (let ((o1 (neo-ca)) (o2 (neo-ca)))
    (oset o1 shared 5)
    (list (oref o1 shared) (oref o2 shared))))
"##,
        expect,
    );
}

#[test]
fn div_eo_oset_default_changes_default() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument symbol 's slot)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (defclass neo-od () ((s :initarg :s :initform :a)))
  (let ((o1 (neo-od)))
    (oset-default 'neo-od 's :b)
    (let ((o2 (neo-od)))
      (list (oref o1 s) (oref o2 s)))))
"##,
        expect,
    );
}

// --- slot iteration / with-slots --------------------------------------------

#[test]
fn div_eo_with_slots_macro() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 3""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (defclass neo-ws () ((a :initarg :a) (b :initarg :b)))
  (let ((o (neo-ws :a 1 :b 2)))
    (with-slots (a b) o (+ a b))))
"##,
        expect,
    );
}

#[test]
fn div_eo_object_slots_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (wrong-type-argument symbolp #s(cl-slot-descriptor a 'eieio--unbound t nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (defclass neo-sl () ((a :initarg :a) (b :initarg :b)))
  (let ((o (neo-sl :a 1 :b 2)))
    (sort (mapcar #'symbol-name (eieio-class-slots 'neo-sl)) #'string<)))
"##,
        expect,
    );
}

// --- cl-defgeneric method combination via qualifiers on subclass ------------

#[test]
fn div_eo_qualified_methods_accumulate_subclass() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:base :sub-after)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (defclass neo-q-base () ())
  (defclass neo-q-sub (neo-q-base) ())
  (let (order)
    (cl-defgeneric neo-gq (obj))
    (cl-defmethod neo-gq ((obj neo-q-base)) (push :base order))
    (cl-defmethod neo-gq :after ((obj neo-q-sub)) (push :sub-after order))
    (neo-gq (neo-q-sub))
    (reverse order)))
"##,
        expect,
    );
}
