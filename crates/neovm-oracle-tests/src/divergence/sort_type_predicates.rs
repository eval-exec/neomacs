//! Divergence tests: sort, comparison predicates, type checks deep.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_sort_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 1 2 3 4 5 6 9)""#]];
    crate::common::assert_oracle_parity_expect(r#"(sort '(3 1 4 1 5 9 2 6) #'<)"#, expect);
}

#[test]
fn divergence_sort_strings() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"apple\" \"banana\" \"cherry\" \"date\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(sort '("banana" "apple" "cherry" "date") #'string<)"#,
        expect,
    );
}

#[test]
fn divergence_type_predicates_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t nil t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (listp '(1 2 3))
  (listp '(1 . 2))
  (listp nil)
  (consp '(1 2))
  (consp '(1 . 2))
  (consp nil)
  (null nil)
  (atom 42)
  (atom "string")
  (atom '(1 2)))"#,
        expect,
    );
}

#[test]
fn divergence_sequence_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t nil t t nil t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (sequencep '(1 2 3))
  (sequencep [1 2 3])
  (sequencep "abc")
  (sequencep 42)
  (arrayp [1 2 3])
  (arrayp "abc")
  (arrayp '(1 2 3))
  (char-table-p (make-char-table 'foo))
  (bool-vector-p (make-bool-vector 5 t))
  (vectorp [1 2 3]))"#,
        expect,
    );
}

#[test]
fn divergence_number_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil t nil t t nil t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (integerp 42)
  (integerp 1.0)
  (floatp 1.0)
  (floatp 42)
  (numberp 42)
  (numberp 1.0)
  (numberp "42")
  (natnump 5)
  (natnump -1)
  (zerop 0))"#,
        expect,
    );
}

#[test]
fn divergence_comparison_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (eq 'foo 'foo)
  (eq 'foo "foo")
  (eql 1.0 1.0)
  (eql 42 42)
  (equal '(1 2 3) '(1 2 3))
  (equal-including-properties
    (propertize "foo" 'face 'bold)
    (propertize "foo" 'face 'bold))
  (string-equal "foo" "foo"))"#,
        expect,
    );
}

#[test]
fn divergence_max_min() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (9 1 42 42)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (max 1 5 3 9 2)
  (min 1 5 3 9 2)
  (abs -42)
  (abs 42))"#,
        expect,
    );
}

#[test]
fn divergence_arithmetic_edge() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 2 3 4 4 3 3.0 4.0)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (mod 10 3)
  (mod -10 3)
  (floor 3.7)
  (ceiling 3.2)
  (round 3.5)
  (truncate 3.7)
  (ffloor 3.7)
  (fround 3.5))"#,
        expect,
    );
}

#[test]
fn divergence_float_special() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (floatp most-positive-fixnum)
  (floatp most-negative-fixnum)
  (> most-positive-fixnum 0)
  (< most-negative-fixnum 0)
  (number-or-marker-p (point-marker))
  (integer-or-marker-p 42)
  (integer-or-marker-p (point-marker)))"#,
        expect,
    );
}

#[test]
fn divergence_string_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil t t nil t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (stringp "hello")
  (stringp 42)
  (string-or-null-p "hello")
  (string-or-null-p nil)
  (string-or-null-p 42)
  (char-or-string-p ?A)
  (char-or-string-p "A")
  (char-or-string-p 42))"#,
        expect,
    );
}
