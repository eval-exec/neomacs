//! Complex combo batch 347 — `cl-seq`/`seq` library ultimate: seq-map/
//! filter/reduce/group-by/partition/uniq/sort/union/intersection/difference/
//! find/position/count/take/drop/subseq/concatenate across list/vector/string.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx347_seq_map_filter_reduce_across_types() {
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
    )
}

#[test]
fn div_cx347_seq_group_by_partition_sort() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (((:odd 1 3 5 7 9) (:even 2 4 6 8 10)) ((1 2 3) (4 5 6) (7 8 9) (10)) (1 1 2 3 4 5 6 9) (1 1 -3 -4 -5 9))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((data '(1 2 3 4 5 6 7 8 9 10)))
  (list (seq-group-by (lambda (x) (if (evenp x) :even :odd)) data)
        (seq-partition data 3)
        (seq-sort #'< '(3 1 4 1 5 9 2 6))
        (seq-sort-by #'abs #'< '(-3 1 -4 1 -5 9))))
"##,
        expect,
    )
}

#[test]
fn div_cx347_seq_set_operations_with_key() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (2 . 3) 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((a '((1 . "a") (2 . "b") (3 . "c")))
      (b '((2 . "x") (4 . "y"))))
  (list (sort (seq-union a b :key #'car) (lambda (x y) (< (car x) (car y))))
        (sort (seq-intersection a b :key #'car) (lambda (x y) (< (car x) (car y))))
        (sort (seq-difference a b :key #'car) (lambda (x y) (< (car x) (car y))))))
"##,
        expect,
    )
}

#[test]
fn div_cx347_seq_find_position_contains_count() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (4 2 nil t nil 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((data '(1 2 3 4 5)))
  (list (seq-find (lambda (x) (> x 3)) data)
        (seq-position data 3)
        (seq-position data 99)
        (seq-contains-p data 3)
        (seq-contains-p data 99)
        (seq-count (lambda (x) (evenp x)) data)))
"##,
        expect,
    )
}

#[test]
fn div_cx347_seq_subseq_take_drop_concatenate() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((2 3 4 5) (2 3) (1 2 3 4) (1 2 3) (3 4 5) (1 2 3 4 53 54) [1 2 3 4] (1 2 3))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (seq-subseq '(1 2 3 4 5) 1)
      (seq-subseq '(1 2 3 4 5) 1 3)
      (seq-subseq '(1 2 3 4 5) 0 -1)
      (seq-take '(1 2 3 4 5) 3)
      (seq-drop '(1 2 3 4 5) 2)
      (seq-concatenate 'list '(1 2) [3 4] "56")
      (seq-concatenate 'vector '(1 2) [3 4])
      (seq-into [1 2 3] 'list))
"##,
        expect,
    )
}

#[test]
fn div_cx347_cl_sort_stable_sort_merge_variants() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-sort)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((data (copy-sequence '((3 . "c") (1 . "a") (4 . "d") (1 . "e") (5 . "b")))))
  (list (cl-sort (copy-sequence data) #'< :key #'car)
        (cl-stable-sort (copy-sequence data) #'< :key #'car)
        (cl-merge 'list '(1 3 5) '(2 4 6) #'<)
        (cl-sort (copy-sequence '("apple" "berry" "cherry")) #'string<)))
"##,
        expect,
    )
}

#[test]
fn div_cx347_cl_some_every_notany_notevery() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-some)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((nums '(1 2 3 4 5)))
  (list (cl-some #'evenp nums)
        (cl-some (lambda (x) (> x 100)) nums)
        (cl-every #'integerp nums)
        (cl-every (lambda (x) (> x 0)) nums)
        (cl-notany #'oddp '(2 4 6))
        (cl-notevery #'evenp nums)))
"##,
        expect,
    )
}

#[test]
fn div_cx347_cl_position_find_count_member_if() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-position)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (cl-position 3 '(1 2 3 4 5))
      (cl-position 3 '(1 2 3 4 5) :from-end t)
      (cl-position 99 '(1 2 3 4 5))
      (cl-find 3 '(1 2 3 4 5))
      (cl-find 99 '(1 2 3 4 5))
      (cl-count 1 '(1 2 1 3 1 4))
      (cl-member-if (lambda (x) (> x 3)) '(1 2 3 4 5)))
"##,
        expect,
    )
}

#[test]
fn div_cx347_cl_remove_substitute_adjoin_pushnew() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-remove)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (cl-remove 1 '(1 2 1 3 1 4))
      (cl-remove 1 '(1 2 1 3 1 4) :count 2)
      (cl-remove-if #'evenp '(1 2 3 4 5 6))
      (cl-substitute 9 1 '(1 2 1 3 1 4))
      (cl-substitute 9 1 '(1 2 1 3 1 4) :count 2)
      (cl-adjoin '(3 . "c") '((1 . "a") (2 . "b")) :key #'car)
      (cl-adjoin '(1 . "x") '((1 . "a") (2 . "b")) :key #'car)))
"##,
        expect,
    )
}

#[test]
fn div_cx347_seq_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (invalid-function (\"alpha\" \"beta\" \"gamma\" \"delta\" \"epsilon\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((items '("alpha" "beta" "gamma" "delta" "epsilon")))
  (with-temp-buffer
    (buffer-enable-undo)
    (insert (mapconcat #'identity items "\n"))
    (put-text-property 1 5 'face 'bold)
    (let ((m (set-marker (make-marker) 8))
          (ov (make-overlay 4 14)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 20)
      (let ((state (list (seq-sort (copy-sequence items)
                                   (lambda (a b) (< (length a) (length b))))
                         (seq-uniq items)
                         (seq-find (lambda (s) (> (length s) 5)) items)
                         (seq-count (lambda (s) (> (length s) 4)) items)
                         (buffer-string)
                         (marker-position m)
                         (overlay-start ov) (overlay-end ov)
                         (text-properties-at 1))))
        (undo)
        (widen)
        (list state (buffer-string) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              (text-properties-at 1))))))
"##,
        expect,
    )
}
