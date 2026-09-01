//! Complex combo batch 72 — sequences / seq library deep: seq-map,
//! seq-filter, seq-reduce, seq-group-by, seq-union, subseq, take/drop,
//! with strings, vectors, lists and mixed inputs.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx72_seq_map_filter_reduce_across_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((1 4 9 16) (1 4 9 16) (9409 9604 9801 10000) (3 4 5) (3 4 5) 15 (3 2 1))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list
 (seq-map (lambda (x) (* x x)) '(1 2 3 4))
 (seq-map (lambda (x) (* x x)) [1 2 3 4])
 (seq-map (lambda (x) (* x x)) "abcd")
 (seq-filter (lambda (x) (> x 2)) '(1 2 3 4 5))
 (seq-filter (lambda (x) (> x 2)) [1 2 3 4 5])
 (seq-reduce #'+ '(1 2 3 4 5) 0)
 (seq-reduce (lambda (acc x) (cons x acc)) '(1 2 3) nil))
"##,
        expect,
    );
}

#[test]
fn div_cx72_seq_group_by_partition() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (((:odd 1 3 5 7 9) (:even 2 4 6 8 10)) ((1 2 3) (4 5 6) (7 8 9) (10)) ((1 2 3 4) (5 6 7 8) (9 10)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((data '(1 2 3 4 5 6 7 8 9 10)))
  (list (seq-group-by (lambda (x) (if (evenp x) :even :odd)) data)
        (seq-partition data 3)
        (seq-partition data 4)))
"##,
        expect,
    );
}

#[test]
fn div_cx72_seq_set_operations_unique_union_difference() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((1 2 3 4) (104 101 108 111) (1 2 3 4) (2 4) (1 3 5))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list
 (seq-uniq '(1 2 2 3 3 3 4))
 (seq-uniq "hello")
 (seq-union '(1 2 3) '(2 3 4))
 (seq-intersection '(1 2 3 4) '(2 4 6))
 (seq-difference '(1 2 3 4 5) '(2 4)))
"##,
        expect,
    );
}

#[test]
fn div_cx72_subseq_take_drop_nth() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((2 3 4 5) (2 3) (1 2 3 4) \"hello\" [3 4 5] (1 2 3) (3 4 5) (1 2 3) (4 5))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list
 (seq-subseq '(1 2 3 4 5) 1)
 (seq-subseq '(1 2 3 4 5) 1 3)
 (seq-subseq '(1 2 3 4 5) 0 -1)
 (seq-subseq "hello world" 0 5)
 (seq-subseq [1 2 3 4 5] 2)
 (seq-take '(1 2 3 4 5) 3)
 (seq-drop '(1 2 3 4 5) 2)
 (seq-take-while (lambda (x) (< x 4)) '(1 2 3 4 5))
 (seq-drop-while (lambda (x) (< x 4)) '(1 2 3 4 5)))
"##,
        expect,
    );
}

#[test]
fn div_cx72_seq_sort_sort_by_min_max() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((1 1 2 3 4 5 6 9) (1 1 -3 -4 -5 9) 1 9 101 111)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list
 (seq-sort #'< '(3 1 4 1 5 9 2 6))
 (seq-sort-by #'abs #'< '(-3 1 -4 1 -5 9))
 (seq-min '(3 1 4 1 5 9))
 (seq-max '(3 1 4 1 5 9))
 (seq-min "hello")
 (seq-max "hello"))
"##,
        expect,
    );
}

#[test]
fn div_cx72_seq_find_position_contains_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (4 2 4 t nil nil 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list
 (seq-find (lambda (x) (> x 3)) '(1 2 3 4 5))
 (seq-position '(1 2 3 4 5) 3)
 (seq-position "hello world" ?o)
 (seq-contains-p '(1 2 3 4 5) 3)
 (seq-contains-p "hello" ?x)
 (seq-contains-p '(1 2 3) 99)
 (seq-count (lambda (x) (evenp x)) '(1 2 3 4 5 6)))
"##,
        expect,
    );
}

#[test]
fn div_cx72_seq_do_each_for_each_with_index() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((10 20 30) ((0 . 10) (1 . 20) (2 . 30)))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (acc)
  (seq-do (lambda (x) (push (* x 10) acc)) '(1 2 3))
  (let ((after-list (nreverse acc)))
    (setq acc nil)
    (seq-do-indexed (lambda (x i) (push (cons i x) acc)) [10 20 30])
    (list after-list (nreverse acc))))
"##,
        expect,
    );
}

#[test]
fn div_cx72_seq_concatenate_into_various_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((1 2 3 4 53 54) [1 2 3 4 53 54] \"ABcd\" [1 2 3] (1 2 3) (97 98 99))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list
 (seq-concatenate 'list '(1 2) [3 4] "56")
 (seq-concatenate 'vector '(1 2) [3 4] "56")
 (seq-concatenate 'string '(65 66) "cd")
 (seq-into '(1 2 3) 'vector)
 (seq-into [1 2 3] 'list)
 (seq-into "abc" 'list))
"##,
        expect,
    );
}

#[test]
fn div_cx72_cl_loop_iterating_sequences_and_vectors() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list
 (cl-loop for x across [1 2 3 4] collect (* x x))
 (cl-loop for x across "abc" collect x)
 (cl-loop for x in-string "hello" collect x)
 (cl-loop for x being the elements of [1 2 3] collect x)
 (cl-loop for x being the elements of "abcd" collect x))
"##,
        expect,
    );
}

#[test]
fn div_cx72_cl_subseq_with_default_and_setf() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-subseq)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((v (vector 1 2 3 4 5))
      (s "hello")
      (l '(1 2 3 4 5)))
  (list (cl-subseq v 1 3)
        (cl-subseq s 0 2)
        (cl-subseq l 2)
        (setf (cl-subseq v 1 3) [99 88])
        v
        (setf (cl-subseq s 0 2) "XX")
        s))
"##,
        expect,
    );
}

#[test]
fn div_cx72_cl_remove_remove_if_substitute_with_count() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-remove)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list
 (cl-remove 1 '(1 2 1 3 1 4))
 (cl-remove 1 '(1 2 1 3 1 4) :count 2)
 (cl-remove 1 '(1 2 1 3 1 4) :count 2 :from-end t)
 (cl-remove-if #'evenp '(1 2 3 4 5 6))
 (cl-substitute 9 1 '(1 2 1 3 1 4))
 (cl-substitute 9 1 '(1 2 1 3 1 4) :count 2))
"##,
        expect,
    );
}

#[test]
fn div_cx72_cl_position_find_count_member_if() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-position)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list
 (cl-position 3 '(1 2 3 4 5))
 (cl-position 3 '(1 2 3 4 5) :from-end t)
 (cl-position 99 '(1 2 3 4 5))
 (cl-find 3 '(1 2 3 4 5))
 (cl-find 99 '(1 2 3 4 5))
 (cl-count 1 '(1 2 1 3 1 4))
 (cl-member-if (lambda (x) (> x 3)) '(1 2 3 4 5)))
"##,
        expect,
    );
}

#[test]
fn div_cx72_sequence_copy_map_into_reverse_combinations_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function seq-unique)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((input '((1 . "a") (2 . "b") (3 . "c") (4 . "d")))
       (keys (seq-map #'car input))
       (vals (seq-map #'cdr input))
       (copy-keys (copy-sequence keys))
       (reversed (seq-reverse keys))
       (sorted (seq-sort #'> keys))
       (indexed (seq-map-indexed (lambda (x i) (cons i x)) vals))
       (unique (seq-unique (append keys keys)))
       (partitioned (seq-partition input 2)))
  (list keys vals copy-keys reversed sorted indexed unique partitioned))
"##,
        expect,
    );
}
