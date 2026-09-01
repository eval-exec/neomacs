//! cl-lib numeric (cl-floor/ceiling/truncate/round two-arg + remainders,
//! cl-gcd/lcm/isqrt/signum, cl-parse-integer, cl-multiple-value-bind),
//! cl-loop advanced (being hash-keys/values, maximize/minimize/count/thereis/
//! always/never, for=then, append/nconc), seq advanced (mapcat/keep/positions/
//! contains/set-equal/split/partition/take-while); plus the cl-floor single-arg
//! divergence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn cn_cl_floor_round_twoarg() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK ((3 1) (4 -1) (3 1) (4 -1) (2 0.5) (8 -0.5) 1 -1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'cl-lib)
(list (cl-floor 7 2) (cl-ceiling 7 2) (cl-truncate 7 2) (cl-round 7 2)
      (cl-round 2.5) (cl-round 7.5) (cl-mod 7 3) (cl-rem -7 3))"##,
        expect,
    );
}

#[test]
fn cn_cl_gcd_lcm_etc() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (12 12 4 -1 0 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'cl-lib)
(list (cl-gcd 48 36) (cl-lcm 4 6) (cl-isqrt 17) (cl-signum -5) (cl-signum 0) (cl-signum 3.0))"##,
        expect,
    );
}

#[test]
fn cn_cl_loop_being_hash() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((a b c) 6)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'cl-lib)
(let ((h (make-hash-table :test 'eq)))
  (puthash 'a 1 h) (puthash 'b 2 h) (puthash 'c 3 h)
  (list (sort (cl-loop for k being the hash-keys of h collect k) #'string<)
        (cl-loop for v being the hash-values of h sum v)))"##,
        expect,
    );
}

#[test]
fn cn_cl_loop_for_then() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((1 2 4 8 16) ((0 . 97) (1 . 98) (2 . 99)))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'cl-lib)
(list (cl-loop for x = 1 then (* x 2) repeat 5 collect x)
      (cl-loop for i from 0 for c across "abc" collect (cons i c)))"##,
        expect,
    );
}

#[test]
fn cn_cl_loop_minmax_count() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (5 1 2 t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'cl-lib)
(list (cl-loop for x in '(3 1 4 1 5) maximize x)
      (cl-loop for x in '(3 1 4 1 5) minimize x)
      (cl-loop for x in '(1 2 3 4) count (cl-evenp x))
      (cl-loop for x in '(1 2 3) thereis (> x 2))
      (cl-loop for x in '(1 2 3) always (> x 0))
      (cl-loop for x in '(1 2 3) never (> x 5)))"##,
        expect,
    );
}

#[test]
fn cn_cl_loop_nconc_append() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((1 1 2 2 3 3) (1 2 3 4))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'cl-lib)
(list (cl-loop for x in '(1 2 3) append (list x x))
      (cl-loop for x in '((1 2) (3 4)) nconc (copy-sequence x)))"##,
        expect,
    );
}

#[test]
fn cn_cl_parse_integer_edge() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (42 255 0 -7)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'cl-lib)
(list (cl-parse-integer "  42  ") (cl-parse-integer "ff" :radix 16)
      (cl-parse-integer "10" :start 1) (cl-parse-integer "-7"))"##,
        expect,
    );
}

#[test]
fn cn_cl_values_floor_bind() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (3 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'cl-lib)
(cl-multiple-value-bind (q r) (cl-floor 17 5) (list q r))"##,
        expect,
    );
}

#[test]
fn cn_seq_advanced_2() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-evenp)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'seq)
(list (seq-mapcat (lambda (x) (list x x)) '(1 2 3))
      (seq-keep (lambda (x) (and (cl-evenp x) (* x 10))) '(1 2 3 4))
      (seq-positions '(a b a c a) 'a)
      (seq-contains-p '(1 2 3) 2)
      (seq-set-equal-p '(1 2 3) '(3 2 1)))"##,
        expect,
    );
}

#[test]
fn cn_seq_split_partition() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-oddp)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'seq)
(list (seq-split [1 2 3 4 5] 2)
      (seq-partition '(1 2 3 4 5 6 7) 3)
      (seq-drop-while #'cl-oddp '(1 3 4 5))
      (seq-take-while #'cl-oddp '(1 3 4 5)))"##,
        expect,
    );
}

#[test]
fn divergence_cl_floor_single_arg() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((7 0.5) (4 -0.7999999999999998) (-3 -0.7000000000000002) (7 0))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'cl-lib)
(list (cl-floor 7.5) (cl-ceiling 3.2) (cl-truncate -3.7) (cl-floor 7))"##,
        expect,
    );
}
