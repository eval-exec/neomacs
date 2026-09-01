/// Batch 483: cl-defstruct deep, cl-package, cl-macro, cl-declarations, cl-optimize.
use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx483_cl_defstruct_create() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (cl-defstruct neo-cx483-point x y)
  (let ((p (make-neo-cx483-point :x 3 :y 4)))
    (list (neo-cx483-point-x p) (neo-cx483-point-y p))))
"##,
        expect,
    );
}

#[test]
fn div_cx483_cl_defstruct_inherit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (cl-defstruct neo-cx483-parent "Parent" (x 0))
  (cl-defstruct (neo-cx483-child (:include neo-cx483-parent)) y)
  (let ((c (make-neo-cx483-child :x 10 :y 20)))
    (list (neo-cx483-parent-x c) (neo-cx483-child-y c))))
"##,
        expect,
    );
}

#[test]
fn div_cx483_cl_defstruct_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (cl-defstruct (neo-cx483-vec (:type vector)) a b)
  (let ((v (make-neo-cx483-vec :a 1 :b 2)))
    (list (aref v 0) (aref v 1))))
"##,
        expect,
    );
}

#[test]
fn div_cx483_cl_defstruct_named() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (cl-defstruct (neo-cx483-n (:named)) x y)
  (let ((s (make-neo-cx483-n :x 5 :y 6)))
    (list (neo-cx483-n-p s) (neo-cx483-n-x s))))
"##,
        expect,
    );
}

#[test]
fn div_cx483_cl_defstruct_print() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (cl-defstruct (neo-cx483-print (:print-function (lambda (s _) (princ \"[CUSTOM]\" s)))) x)
  (make-neo-cx483-print :x 1))
"##,
        expect,
    );
}

#[test]
fn div_cx483_cl_defstruct_copier() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (cl-defstruct neo-cx483-cp x y)
  (let ((o (make-neo-cx483-cp :x 1 :y 2))
        (c (copy-neo-cx483-cp o)))
    (list (neo-cx483-cp-x o) (neo-cx483-cp-x c))))
"##,
        expect,
    );
}

#[test]
fn div_cx483_cl_defstruct_accessors() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (cl-defstruct neo-cx483-ac (x 0) (y 0))
  (let ((obj (make-neo-cx483-ac)))
    (setf (neo-cx483-ac-x obj) 42)
    (neo-cx483-ac-x obj)))
"##,
        expect,
    );
}

#[test]
fn div_cx483_cl_defstruct_predicate() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (cl-defstruct neo-cx483-pd (x))
  (list (neo-cx483-pd-p (make-neo-cx483-pd))
        (neo-cx483-pd-p nil)
        (neo-cx483-pd-p "string")))
"##,
        expect,
    );
}

#[test]
fn div_cx483_cl_defstruct_conc_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (cl-defstruct (neo-cx483-cn (:conc-name cx483-)) x)
  (let ((o (make-neo-cx483-cn :x 7)))
    (cx483-x o)))
"##,
        expect,
    );
}

#[test]
fn div_cx483_cl_defstruct_constructor() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (cl-defstruct (neo-cx483-con (:constructor cx483-create)) x y)
  (let ((o (cx483-create :x 1 :y 2)))
    (neo-cx483-con-x o)))
"##,
        expect,
    );
}

#[test]
fn div_cx483_cl_defstruct_boac() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (cl-defstruct (neo-cx483-boac (:type list) :named) a b)
  (let ((o (make-neo-cx483-boac :a 'x :b 'y)))
    (list (car o) (cadr o) (caddr o))))
"##,
        expect,
    );
}

#[test]
fn div_cx483_cl_defstruct_niltype() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (cl-defstruct (neo-cx483-nt (:type list)) x)
  (listp (make-neo-cx483-nt :x 1)))
"##,
        expect,
    );
}

#[test]
fn div_cx483_cl_defstruct_slot_opts() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (cl-defstruct neo-cx483-so
    x
    (y 0 :type integer)
    (z nil :read-only t))
  (let ((o (make-neo-cx483-so :x 1 :z 'fixed)))
    (list (neo-cx483-so-x o) (neo-cx483-so-y o) (neo-cx483-so-z o))))
"##,
        expect,
    );
}

#[test]
fn div_cx483_cl_defstruct_doc() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (cl-defstruct neo-cx483-doc "Doc struct" x y)
  (documentation 'neo-cx483-doc 'struct))
"##,
        expect,
    );
}

#[test]
fn div_cx483_cl_defstruct_no_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (cl-defstruct neo-cx483-nt2 x)
  (type-of (make-neo-cx483-nt2 :x 1)))
"##,
        expect,
    );
}
