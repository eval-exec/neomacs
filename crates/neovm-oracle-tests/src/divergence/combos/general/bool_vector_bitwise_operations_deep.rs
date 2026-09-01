//! Deep combo: bool-vector + bitwise operations + set membership + tests.
//! Tests boolean vector operations with set semantics.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn deficiency_bool_vector_create_and_access() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil t t nil 10)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((bv (make-bool-vector 10 nil)))\n\
         (aset bv 0 t)\n\
         (aset bv 3 t)\n\
         (aset bv 7 t)\n\
         (list (aref bv 0) (aref bv 1) (aref bv 3)\n\
         (aref bv 7) (aref bv 9)\n\
         (length bv))))",
        expect,
    );
}

#[test]
fn deficiency_bool_vector_union_intersection() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((t t t nil t t t nil) (nil nil t nil nil nil t nil) (t nil nil nil t nil nil nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((a (make-bool-vector 8 nil))\n\
         (b (make-bool-vector 8 nil)))\n\
         (aset a 0 t) (aset a 2 t) (aset a 4 t) (aset a 6 t)\n\
         (aset b 1 t) (aset b 2 t) (aset b 5 t) (aset b 6 t)\n\
         (let ((union (bool-vector-union a b))\n\
         (intersection (bool-vector-intersection a b))\n\
         (diff (bool-vector-set-difference a b)))\n\
         (list (append union nil)\n\
         (append intersection nil)\n\
         (append diff nil)))))",
        expect,
    );
}

#[test]
fn deficiency_bool_vector_complement() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function bool-vector-complement)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((bv (make-bool-vector 8 nil)))\n\
         (aset bv 1 t) (aset bv 3 t) (aset bv 5 t)\n\
         (let ((comp (bool-vector-complement bv)))\n\
         (list (append bv nil)\n\
         (append comp nil)))))",
        expect,
    );
}

#[test]
fn deficiency_bool_vector_subsetp() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((a (make-bool-vector 8 nil))\n\
         (b (make-bool-vector 8 nil)))\n\
         (aset a 0 t) (aset a 2 t)\n\
         (aset b 0 t) (aset b 2 t) (aset b 4 t)\n\
         (list (bool-vector-subsetp a b)\n\
         (bool-vector-subsetp b a)\n\
         (bool-vector-subsetp a a))))",
        expect,
    );
}

#[test]
fn deficiency_bool_vector_xor() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil nil nil nil nil t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((a (make-bool-vector 8 nil))\n\
         (b (make-bool-vector 8 nil)))\n\
         (aset a 0 t) (aset a 2 t) (aset a 4 t)\n\
         (aset b 2 t) (aset b 4 t) (aset b 6 t)\n\
         (let ((xor (bool-vector-exclusive-or a b)))\n\
         (append xor nil))))",
        expect,
    );
}

#[test]
fn deficiency_bool_vector_population_count() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function bool-vector-count-matches)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((bv (make-bool-vector 16 nil)))\n\
         (aset bv 0 t) (aset bv 3 t) (aset bv 7 t)\n\
         (aset bv 10 t) (aset bv 15 t)\n\
         (bool-vector-count-matches bv t)))",
        expect,
    );
}

#[test]
fn deficiency_bool_vector_from_list_and_back() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (8 (t nil t t nil nil t nil) t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let* ((bits '(t nil t t nil nil t nil))\n\
         (bv (apply 'bool-vector bits)))\n\
         (list (length bv)\n\
         (append bv nil)\n\
         (equal bits (append bv nil)))))",
        expect,
    );
}

#[test]
fn deficiency_bool_vector_not_in_place() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((bv (make-bool-vector 4 nil)))\n\
         (aset bv 1 t) (aset bv 3 t)\n\
         (bool-vector-not bv)\n\
         (append bv nil)))",
        expect,
    );
}

#[test]
fn deficiency_bool_vector_large_intersection() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-evenp)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((a (make-bool-vector 64 nil))\n\
         (b (make-bool-vector 64 nil)))\n\
         (dotimes (i 64)\n\
         (when (cl-evenp i) (aset a i t))\n\
         (when (= (% i 3) 0) (aset b i t)))\n\
         (let ((inter (bool-vector-intersection a b)))\n\
         (cl-loop for i from 0 to 63\n\
         when (aref inter i) collect i))))",
        expect,
    );
}

#[test]
fn deficiency_bool_vector_empty_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function bool-vector-count-matches)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((empty (make-bool-vector 8 nil))\n\
         (full (make-bool-vector 8 t)))\n\
         (list (bool-vector-subsetp empty full)\n\
         (bool-vector-subsetp full empty)\n\
         (bool-vector-count-matches empty t)\n\
         (bool-vector-count-matches full t)\n\
         (append (bool-vector-union empty full) nil))))",
        expect,
    );
}
