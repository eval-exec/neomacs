//! Oracle parity tests for GNU `subr.el` `delete-consecutive-dups`.
//!
//! GNU's implementation is destructive, compares adjacent elements with
//! `equal`, preserves the first element in each run, and optionally treats the
//! first and last elements as adjacent.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_prop_delete_consecutive_dups_basic_runs() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (delete-consecutive-dups (list 'a 'a 'b 'b 'b 'a 'a 'c))
 (delete-consecutive-dups (list 1 1 1 1))
 (delete-consecutive-dups (list 1 2 3))
 (delete-consecutive-dups nil)
 (delete-consecutive-dups (list 'solo)))
"#;

    let expect = expect_test::expect![[r#""OK ((a b a c) (1) (1 2 3) nil (solo))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_delete_consecutive_dups_is_destructive_and_keeps_first() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((first (list 'k))
       (second (list 'k))
       (third (list 'k))
       (other (list 'other))
       (xs (list first second third other))
       (result (delete-consecutive-dups xs)))
  (list result
        (eq result xs)
        (eq (car result) first)
        (memq second result)
        (memq third result)
        (eq (cadr result) other)
        xs))
"#;

    let expect = expect_test::expect![[r#""OK (((k) (other)) t t nil nil t ((k) (other)))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_delete_consecutive_dups_only_removes_adjacent_equal_values() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((xs (list "a" "b" "a" "a" "b" "b" "a")))
  (list (delete-consecutive-dups xs)
        xs))
"#;

    let expect = expect_test::expect![[
        r#""OK ((\"a\" \"b\" \"a\" \"b\" \"a\") (\"a\" \"b\" \"a\" \"b\" \"a\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_delete_consecutive_dups_circular_option() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((non-circular (list 'a 'a 'b 'c 'a 'a))
       (circular (list 'a 'a 'b 'c 'a 'a))
       (same (list 'z 'z 'z)))
  (list
   (delete-consecutive-dups non-circular nil)
   (delete-consecutive-dups circular t)
   (delete-consecutive-dups same t)
   non-circular
   circular
   same))
"#;

    let expect = expect_test::expect![[r#""OK ((a b c a) (a b c) (z) (a b c a) (a b c) (z))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_delete_consecutive_dups_improper_tail_errors_like_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (let ((xs (cons 'a 'tail)))
   (list
    (condition-case err
        (delete-consecutive-dups xs)
      (error (list (car err) (cdr err))))
    xs))
 (let ((xs (cons 'a (cons 'a 'tail))))
   (list
    (condition-case err
        (delete-consecutive-dups xs)
      (error (list (car err) (cdr err))))
    xs))
 (let ((xs (cons 'a (cons 'b 'tail))))
   (list
    (condition-case err
        (delete-consecutive-dups xs)
      (error (list (car err) (cdr err))))
    xs)))
"#;

    let expect = expect_test::expect![[
        r#""OK (((wrong-type-argument (listp tail)) (a . tail)) ((wrong-type-argument (listp tail)) (a . tail)) ((wrong-type-argument (listp tail)) (a b . tail)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
