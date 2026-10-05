use crate::common::{assert_oracle_parity_expect, return_if_neovm_enable_oracle_proptest_not_set};

#[test]
fn take_bignum() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(list (take (expt 2 100) '(1 2)) (ntake (expt 2 100) (list 1 2)) (take (- (expt 2 100)) '(1 2)) (ntake (- (expt 2 100)) (list 1 2)) (butlast '(1 2 3) (expt 2 100)) (take (1+ most-positive-fixnum) nil) (condition-case e (take (expt 2 100) 'x) (error e)) (condition-case e (take 1.0 '(1 2)) (error e)))"#;
    assert_oracle_parity_expect(
        form,
        expect_test::expect![[
            r#""OK ((1 2) (1 2) nil nil nil nil (wrong-type-argument listp x) (wrong-type-argument integerp 1.0))""#
        ]],
    );
}

#[test]
fn ntake_dotted() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(list (condition-case e (ntake 2 (cons 1 2)) (error e)) (condition-case e (ntake 4 (cons 1 (cons 2 (cons 3 4)))) (error e)) (condition-case e (take 2 (cons 1 2)) (error e)) (ntake 1 (cons 1 2)) (condition-case e (ntake 2 5) (error e)))"#;
    assert_oracle_parity_expect(
        form,
        expect_test::expect![[
            r#""OK ((wrong-type-argument listp (1 . 2)) (wrong-type-argument listp (1 2 3 . 4)) (wrong-type-argument listp 2) (1) (wrong-type-argument listp 5))""#
        ]],
    );
}

#[test]
fn value_lt_exact() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(list (value< most-positive-fixnum (float most-positive-fixnum)) (value< 9007199254740992.0 9007199254740993) (value< 9007199254740993 9007199254740992.0) (value< -9007199254740993 -9007199254740992.0) (value< -9007199254740992.0 -9007199254740993) (value< 1 1.5) (value< 1.5 1) (value< 1 0.0e+NaN) (value< 0.0e+NaN 1) (value< most-positive-fixnum 1.0e+INF) (value< -1.0e+INF most-negative-fixnum) (sort (list 9007199254740993 9007199254740992.0)))"#;
    assert_oracle_parity_expect(
        form,
        expect_test::expect![[r#""OK (t t nil t nil t nil nil nil t t (9007199254740992.0 0))""#]],
    );
}

#[test]
fn constructor_limits() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(let ((memory-signal-data '(error "allocation exhausted"))) (list (condition-case e (make-vector most-positive-fixnum 0) (error e)) (condition-case e (make-string most-positive-fixnum ?a) (error e)) (condition-case e (make-string most-positive-fixnum ?é) (error e)) (condition-case e (make-bool-vector most-positive-fixnum t) (error e)) (make-vector 0 7) (make-string 3 ?é) (length (make-bool-vector 65 t))))"#;
    assert_oracle_parity_expect(
        form,
        expect_test::expect![[
            r#""OK ((error \"allocation exhausted\") (error \"allocation exhausted\") (error \"Maximum string size exceeded\") (error \"allocation exhausted\") [] \"ééé\" 65)""#
        ]],
    );
}

#[test]
fn compiled_sequence_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn (require 'bytecomp) (defun gdl--take (n l) (take n l)) (defun gdl--ntake (n l) (ntake n l)) (defun gdl--value (a b) (value< a b)) (byte-compile 'gdl--take) (byte-compile 'gdl--ntake) (byte-compile 'gdl--value) (list (gdl--take (expt 2 100) '(1 2)) (gdl--ntake (- (expt 2 100)) (list 1 2)) (condition-case e (gdl--ntake 2 (cons 1 2)) (error e)) (gdl--value 9007199254740992.0 9007199254740993)))"#;
    assert_oracle_parity_expect(
        form,
        expect_test::expect![[r#""OK ((1 2) nil (wrong-type-argument listp (1 . 2)) t)""#]],
    );
}
