//! Input-validation / bad-argument error parity (condition-case error
//! symbol): make-* negative sizes, aref/aset bounds, sqrt/log domain, char
//! range, sequence index edges, number parsing; concat/apply/funcall/arith
//! type coercion + errors, string-ops bounds, list destructuring errors,
//! hash-table bad args.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn aref_aset_bounds() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((ERR . args-out-of-range) (ERR . args-out-of-range) (ERR . args-out-of-range) (ERR . args-out-of-range))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (condition-case e (aref [1 2] 5) (error (cons (quote ERR) (car e)))) (condition-case e (aref "ab" -1) (error (cons (quote ERR) (car e)))) (condition-case e (aset (make-vector 2 0) 9 1) (error (cons (quote ERR) (car e)))) (condition-case e (aref (make-bool-vector 3 nil) 8) (error (cons (quote ERR) (car e)))))"##,
        expect,
    );
}

#[test]
fn char_range() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((ERR . wrong-type-argument) (ERR . wrong-type-argument) (ERR . args-out-of-range) \"����\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (condition-case e (char-to-string -1) (error (cons (quote ERR) (car e)))) (condition-case e (char-to-string 5000000) (error (cons (quote ERR) (car e)))) (condition-case e (make-char 'ascii 300) (error (cons (quote ERR) (car e)))) (condition-case e (string 1114112) (error (cons (quote ERR) (car e)))))"##,
        expect,
    );
}

#[test]
fn make_negative_sizes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((ERR . wrong-type-argument) (ERR . wrong-type-argument) (ERR . wrong-type-argument) (ERR . wrong-type-argument) (ERR . error))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (condition-case e (make-string -1 ?x) (error (cons (quote ERR) (car e)))) (condition-case e (make-vector -1 0) (error (cons (quote ERR) (car e)))) (condition-case e (make-list -1 0) (error (cons (quote ERR) (car e)))) (condition-case e (make-bool-vector -1 nil) (error (cons (quote ERR) (car e)))) (condition-case e (make-hash-table :size -1) (error (cons (quote ERR) (car e)))))"##,
        expect,
    );
}

#[test]
fn number_parse_edge() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (0 0 12 0 1.5)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (condition-case e (string-to-number "") (error (cons (quote ERR) (car e)))) (condition-case e (string-to-number "xyz") (error (cons (quote ERR) (car e)))) (condition-case e (string-to-number "  12  ") (error (cons (quote ERR) (car e)))) (condition-case e (string-to-number "0x1F") (error (cons (quote ERR) (car e)))) (condition-case e (string-to-number "1.5.6") (error (cons (quote ERR) (car e)))))"##,
        expect,
    );
}

#[test]
fn seq_index_edge() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (1 (1 2 3) (ERR . args-out-of-range) (ERR . wrong-type-argument) nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (condition-case e (nth -1 '(1 2 3)) (error (cons (quote ERR) (car e)))) (condition-case e (nthcdr -1 '(1 2 3)) (error (cons (quote ERR) (car e)))) (condition-case e (elt [1 2] 9) (error (cons (quote ERR) (car e)))) (condition-case e (substring '(1 2) 0 1) (error (cons (quote ERR) (car e)))) (condition-case e (last '(1 2) -1) (error (cons (quote ERR) (car e)))))"##,
        expect,
    );
}

#[test]
fn sqrt_log_domain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK (-0.0e+NaN -0.0e+NaN -1.0e+INF 1.0e+INF 1.0e+INF)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (condition-case e (sqrt -4.0) (error (cons (quote ERR) (car e)))) (condition-case e (log -1.0) (error (cons (quote ERR) (car e)))) (condition-case e (log 0.0) (error (cons (quote ERR) (car e)))) (condition-case e (expt 0 -1) (error (cons (quote ERR) (car e)))) (condition-case e (/ 1.0 0.0) (error (cons (quote ERR) (car e)))))"##,
        expect,
    );
}

#[test]
fn apply_funcall_bad() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (6 (ERR . wrong-type-argument) (ERR . invalid-function) (ERR . wrong-number-of-arguments))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (condition-case e (apply '+ 1 2 '(3)) (error (cons (quote ERR) (car e)))) (condition-case e (apply '+ 5) (error (cons (quote ERR) (car e)))) (condition-case e (funcall 5) (error (cons (quote ERR) (car e)))) (condition-case e (apply 'car nil) (error (cons (quote ERR) (car e)))))"##,
        expect,
    );
}

#[test]
fn arith_type_mix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((ERR . wrong-type-argument) (ERR . wrong-type-argument) (ERR . wrong-type-argument) (ERR . wrong-number-of-arguments) (ERR . wrong-type-argument))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (condition-case e (+ 1 'a) (error (cons (quote ERR) (car e)))) (condition-case e (* nil 2) (error (cons (quote ERR) (car e)))) (condition-case e (1+ "x") (error (cons (quote ERR) (car e)))) (condition-case e (max) (error (cons (quote ERR) (car e)))) (condition-case e (min 1 2 'z) (error (cons (quote ERR) (car e)))))"##,
        expect,
    );
}

#[test]
fn concat_type_coerce() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"\u{1}\u{2}\u{3}\" \"AB\" (ERR . wrong-type-argument) [97 98 1] (97 98))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (condition-case e (concat '(1 2 3)) (error (cons (quote ERR) (car e)))) (condition-case e (concat [65 66]) (error (cons (quote ERR) (car e)))) (condition-case e (concat 5) (error (cons (quote ERR) (car e)))) (condition-case e (vconcat "ab" '(1)) (error (cons (quote ERR) (car e)))) (condition-case e (append "ab" nil) (error (cons (quote ERR) (car e)))))"##,
        expect,
    );
}

#[test]
fn hash_bad_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((ERR . error) (ERR . wrong-type-argument) (ERR . wrong-type-argument) (ERR . wrong-type-argument))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (condition-case e (make-hash-table :test 'nonexistent) (error (cons (quote ERR) (car e)))) (condition-case e (gethash 'k 5) (error (cons (quote ERR) (car e)))) (condition-case e (puthash 'k 1 [1 2]) (error (cons (quote ERR) (car e)))) (condition-case e (maphash 'identity nil) (error (cons (quote ERR) (car e)))))"##,
        expect,
    );
}

#[test]
fn list_destruct_edge() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((ERR . wrong-type-argument) (ERR . wrong-type-argument) 2 (ERR . wrong-type-argument) (ERR . wrong-type-argument))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (condition-case e (car "x") (error (cons (quote ERR) (car e)))) (condition-case e (cdr [1 2]) (error (cons (quote ERR) (car e)))) (condition-case e (setcar '(1) 2) (error (cons (quote ERR) (car e)))) (condition-case e (nconc 1 2) (error (cons (quote ERR) (car e)))) (condition-case e (length 5) (error (cons (quote ERR) (car e)))))"##,
        expect,
    );
}

#[test]
fn string_ops_edge() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((ERR . args-out-of-range) (ERR . args-out-of-range) 0 (ERR . args-out-of-range) (ERR . wrong-type-argument))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (condition-case e (substring "abc" 5) (error (cons (quote ERR) (car e)))) (condition-case e (substring "abc" -5) (error (cons (quote ERR) (car e)))) (condition-case e (string-to-char "") (error (cons (quote ERR) (car e)))) (condition-case e (aref "" 0) (error (cons (quote ERR) (car e)))) (condition-case e (downcase nil) (error (cons (quote ERR) (car e)))))"##,
        expect,
    );
}
