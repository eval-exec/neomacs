//! Deep combo: seq-unique + sort + merge + delete-dups + list operations.
//! Tests list deduplication, sorting, and merge operations.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn deficiency_delete_dups_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((1 2 3 4 5) (a b c d))""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (list (delete-dups '(1 2 3 2 1 4 3 5))\n\
         (delete-dups '(a b a c b d))))",
        expect,
    );
}

#[test]
fn deficiency_seq_unique_preserves_order() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function seq-unique)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (list (seq-unique '(3 1 4 1 5 9 2 6 5 3))\n\
         (seq-unique '(a b c a d b e))))",
        expect,
    );
}

#[test]
fn deficiency_sort_with_custom_predicate() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (((a . 1) (b . 2) (c . 3) (d . 4) (e . 5)) ((a . 1) (b . 2) (c . 3) (d . 4) (e . 5)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((pairs '((c . 3) (a . 1) (b . 2) (e . 5) (d . 4))))\n\
         (let ((by-car (sort (copy-sequence pairs)\n\
         (lambda (a b) (string< (symbol-name (car a))\n\
         (symbol-name (car b))))))\n\
         (by-cdr (sort (copy-sequence pairs)\n\
         (lambda (a b) (< (cdr a) (cdr b))))))\n\
         (list by-car by-cdr))))",
        expect,
    );
}

#[test]
fn deficiency_seq_group_by() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-evenp)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((data '(1 2 3 4 5 6 7 8 9 10)))\n\
         (seq-group-by #'cl-evenp data)))",
        expect,
    );
}

#[test]
fn deficiency_seq_partition() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""ERR (wrong-type-argument number-or-marker-p (a b 1 c 2 d 3 e 4))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((data '(a b 1 c 2 d 3 e 4)))\n\
         (seq-partition (lambda (x) (numberp x)) data)))",
        expect,
    );
}

#[test]
fn deficiency_merge_two_sorted_lists() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function merge)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((a '(1 3 5 7 9))\n\
         (b '(2 4 6 8 10)))\n\
         (merge 'list (copy-sequence a) (copy-sequence b) #'<)))",
        expect,
    );
}

#[test]
fn deficiency_seq_mapcat_with_index() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 1 2 4 3 9 4 16 5 25)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (seq-mapcat (lambda (x) (list x (* x x)))\n\
         '(1 2 3 4 5)))",
        expect,
    );
}

#[test]
fn deficiency_seq_reduce_with_initial() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (15 120 (d c b a))""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (list (seq-reduce #'+ '(1 2 3 4 5) 0)\n\
         (seq-reduce #'* '(1 2 3 4 5) 1)\n\
         (seq-reduce (lambda (acc x) (cons x acc))\n\
         '(a b c d) nil)))",
        expect,
    );
}

#[test]
fn deficiency_seq_filter_remove_combined() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-evenp)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((data '(1 2 3 4 5 6 7 8 9 10)))\n\
         (list (seq-filter #'cl-evenp data)\n\
         (seq-remove #'cl-evenp data)\n\
         (seq-filter (lambda (x) (> x 5)) data)\n\
         (length (seq-filter #'cl-evenp data)))))",
        expect,
    );
}

#[test]
fn deficiency_sort_stability_with_equal_keys() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((items '((a . 2) (b . 1) (c . 2) (d . 1) (e . 3))))\n\
         (sort (copy-sequence items)\n\
         (lambda (a b) (< (cdr a) (cdr b)))))",
        expect,
    );
}
