//! cl-typep/cl-coerce/cl-deftype with complex type specs (integer ranges,
//! or/and/member/satisfies, vector length), cl-coerce across vector/list/
//! string/character/float, type-of comprehensive, bignum bit ops (logand/ior/
//! xor/not/count/ash over 2^100), ash/lsh, number predicates, expt overflow.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn bignum_bit_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (0 1267650600228229401496703205377 64 -1 0 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((big (ash 1 100)))
  (list (logand big (1- big)) (logior big 1) (logcount (1- (ash 1 64)))
        (lognot 0) (ash big -50) (logxor big big)))"##,
        expect,
    );
}

#[test]
fn cl_coerce_char_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (120 \"ab\" \"xy\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'cl-lib)
(list (cl-coerce "x" 'character) (cl-coerce '(?a ?b) 'string)
      (cl-coerce [?x ?y] 'string))"##,
        expect,
    );
}

#[test]
fn cl_coerce_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ([1 2 3] (1 2 3) (97 98 99) 65 5.0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'cl-lib)
(list (cl-coerce '(1 2 3) 'vector) (cl-coerce [1 2 3] 'list)
      (cl-coerce "abc" 'list) (cl-coerce ?A 'character) (cl-coerce 5 'float))"##,
        expect,
    );
}

#[test]
fn cl_deftype_custom() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'cl-lib)
(cl-deftype neo-small-int () '(integer 0 100))
(list (cl-typep 50 'neo-small-int) (cl-typep 200 'neo-small-int))"##,
        expect,
    );
}

#[test]
fn cl_typep_array() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (error \"Bad type spec: (vector * 2)\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'cl-lib)
(list (cl-typep [1 2 3] 'vector) (cl-typep "abc" 'string) (cl-typep [1 2] '(vector * 2))
      (cl-typep '(1 2) 'list) (cl-typep 5 '(and integer (satisfies cl-plusp))))"##,
        expect,
    );
}

#[test]
fn cl_typep_complex() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'cl-lib)
(list (cl-typep 5 '(integer 0 10)) (cl-typep 15 '(integer 0 10))
      (cl-typep "x" '(or string null)) (cl-typep nil '(or string null))
      (cl-typep 3 '(member 1 2 3)) (cl-typep 2.5 '(satisfies floatp)))"##,
        expect,
    );
}

#[test]
fn expt_overflow() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t 1 1 0.01 1024.0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (= (expt 2 62) (ash 1 62)) (integerp (expt 2 70))
        (expt 2 0) (expt 0 0) (expt 10 -2) (expt 2.0 10))"##,
        expect,
    );
}

#[test]
fn logical_shift_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1024 -16 16 -16 16 8 14 6 -6)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (ash 1 10) (ash -1 4) (ash 256 -4) (ash -256 -4)
        (lsh 1 4) (logand 12 10) (logior 12 10) (logxor 12 10) (lognot 5))"##,
        expect,
    );
}

#[test]
fn number_predicates_full() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-plusp)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (integerp 5) (floatp 1.5) (numberp 3) (natnump 0)
        (fixnump 5) (bignump (ash 1 100)) (zerop 0) (cl-plusp 1) (cl-oddp 3)
        (wholenump 5) (characterp ?a))"##,
        expect,
    );
}

#[test]
fn type_of_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (integer float string integer symbol symbol vector cons hash-table marker interpreted-function subr)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (type-of 1) (type-of 1.0) (type-of "s") (type-of ?c)
        (type-of 'sym) (type-of nil) (type-of [1]) (type-of '(1))
        (type-of (make-hash-table)) (type-of (make-marker))
        (type-of (lambda () 1)) (type-of (symbol-function 'car)))"##,
        expect,
    );
}
