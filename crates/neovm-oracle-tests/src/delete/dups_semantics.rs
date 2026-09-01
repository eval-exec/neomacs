//! Oracle parity tests for GNU `subr.el` `delete-dups`.
//!
//! GNU implements `delete-dups` in Lisp.  The small-list path repeatedly calls
//! `delete`, so it inherits GNU `delete`'s destructive mutation and tail-check
//! ordering on malformed input lists.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_delete_dups_small_list_is_destructive_and_keeps_first() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((first (list 'k 1))
       (second (list 'k 1))
       (third (list 'other 2))
       (fourth (list 'k 1))
       (xs (list first second third fourth))
       (result (delete-dups xs)))
  (list result
        xs
        (eq result xs)
        (eq (car result) first)
        (memq second result)
        (eq (cadr result) third)
        (memq fourth result)))
"#;

    let expect =
        expect_test::expect![[r#""OK (((k 1) (other 2)) ((k 1) (other 2)) t t nil t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_delete_dups_small_list_mutates_before_improper_tail_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((xs (cons 'a (cons 'b (cons 'b 'tail)))))
  (list
   (condition-case err
       (delete-dups xs)
     (error (list (car err) (cdr err))))
   xs))
"#;

    let expect =
        expect_test::expect![[r#""OK ((wrong-type-argument (listp tail)) (a b b . tail))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_delete_dups_large_list_uses_hash_path_and_keeps_first() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((first (list 'same))
       (middle (list 'same))
       (last (list 'same))
       (xs (append (list first)
                   (number-sequence 0 100)
                   (list middle last)))
       (result (delete-dups xs)))
  (list
   (eq result xs)
   (eq (car result) first)
   (memq middle result)
   (memq last result)
   (length result)
   (nth 1 result)
   (nth 101 result)
   xs))
"#;

    let expect = expect_test::expect![[
        r#""OK (t t nil nil 102 0 100 ((same) 0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31 32 33 34 35 36 37 38 39 40 41 42 43 44 45 46 47 48 49 50 51 52 53 54 55 56 57 58 59 60 61 62 63 64 65 66 67 68 69 70 71 72 73 74 75 76 77 78 79 80 81 82 83 84 85 86 87 88 89 90 91 92 93 94 95 96 97 98 99 100))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_delete_dups_large_list_rejects_improper_tail_before_hash_walk() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((xs (append (number-sequence 0 100) 'tail)))
  (list
   (condition-case err
       (delete-dups xs)
     (error (list (car err) (cdr err))))
   xs))
"#;

    let expect = expect_test::expect![[
        r#""OK ((wrong-type-argument (listp tail)) (0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31 32 33 34 35 36 37 38 39 40 41 42 43 44 45 46 47 48 49 50 51 52 53 54 55 56 57 58 59 60 61 62 63 64 65 66 67 68 69 70 71 72 73 74 75 76 77 78 79 80 81 82 83 84 85 86 87 88 89 90 91 92 93 94 95 96 97 98 99 100 . tail))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
