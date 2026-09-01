//! Complex combo batch 377 — `seq` library ultimate: seq-map-indexed,
//! sort-by, group-by, partition, uniq, union/intersection/difference,
//! find/position/contains-p/count, reduce, subseq/take/drop/concatenate.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx377_seq_map_indexed_across_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (((0 . a) (1 . b) (2 . c)) ((0 . 10) (1 . 20) (2 . 30)) ((0 . 97) (1 . 98) (2 . 99)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list
 (seq-map-indexed (lambda (x i) (cons i x)) '(a b c))
 (seq-map-indexed (lambda (x i) (cons i x)) [10 20 30])
 (seq-map-indexed (lambda (x i) (cons i x)) "abc"))
"##,
        expect,
    )
}

#[test]
fn div_cx377_seq_sort_by_and_group_by_partition() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (((:odd 1 3 5 7 9) (:even 2 4 6 8 10)) ((1 2 3) (4 5 6) (7 8 9) (10)) ((1 2 3 4) (5 6 7 8) (9 10)) (\"b\" \"e\" \"cc\" \"aaa\" \"dddd\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((data '(1 2 3 4 5 6 7 8 9 10)))
  (list (seq-group-by (lambda (x) (if (evenp x) :even :odd)) data)
        (seq-partition data 3)
        (seq-partition data 4)
        (seq-sort-by #'length #'< '("aaa" "b" "cc" "dddd" "e"))))
"##,
        expect,
    )
}

#[test]
fn div_cx377_seq_uniq_with_test() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((1 2 3 4) (\"A\" \"a\" \"B\" \"b\") (104 101 108 111) (1 2 3))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (seq-uniq '(1 2 2 3 3 3 4))
      (seq-uniq '("A" "a" "B" "b") #'string-equal)
      (seq-uniq "hello")
      (seq-uniq [1 1 2 2 3 3]))
"##,
        expect,
    )
}

#[test]
fn div_cx377_seq_find_position_contains_count() {
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
fn div_cx377_seq_reduce_with_initial_and_from_end() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (15 (3 2 1) 9 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (seq-reduce #'+ '(1 2 3 4 5) 0)
      (seq-reduce (lambda (acc x) (cons x acc)) '(1 2 3) nil)
      (seq-reduce #'max '(3 1 4 1 5 9 2 6) 0)
      (seq-reduce #'min '(3 1 4 1 5 9 2 6) 99))
"##,
        expect,
    )
}

#[test]
fn div_cx377_seq_subseq_take_drop_variants() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((2 3 4 5) (2 3) (1 2 3 4) (1 2 3) (3 4 5) (1 2 3) (4 5))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (seq-subseq '(1 2 3 4 5) 1)
      (seq-subseq '(1 2 3 4 5) 1 3)
      (seq-subseq '(1 2 3 4 5) 0 -1)
      (seq-take '(1 2 3 4 5) 3)
      (seq-drop '(1 2 3 4 5) 2)
      (seq-take-while (lambda (x) (< x 4)) '(1 2 3 4 5))
      (seq-drop-while (lambda (x) (< x 4)) '(1 2 3 4 5)))
"##,
        expect,
    )
}

#[test]
fn div_cx377_seq_concatenate_into_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((1 2 3 4 53 54) [1 2 3 4 53 54] \"ABcd\" [1 2 3] (1 2 3) (97 98 99))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (seq-concatenate 'list '(1 2) [3 4] "56")
      (seq-concatenate 'vector '(1 2) [3 4] "56")
      (seq-concatenate 'string '(65 66) "cd")
      (seq-into '(1 2 3) 'vector)
      (seq-into [1 2 3] 'list)
      (seq-into "abc" 'list))
"##,
        expect,
    )
}

#[test]
fn div_cx377_seq_set_operations_with_key() {
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
fn div_cx377_seq_do_and_do_indexed() {
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
    )
}

#[test]
fn div_cx377_seq_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((input '((("café" . 1) ("世界" . 2) ("alpha" . 3))))
       (keys (seq-map #'caar input))
       (vals (seq-map #'cdar input)))
  (with-temp-buffer
    (buffer-enable-undo)
    (insert (format "seq mega: keys=%S vals=%S" keys vals))
    (put-text-property 1 5 'face 'bold)
    (let ((m (set-marker (make-marker) 10))
          (ov (make-overlay 4 20)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 28)
      (let ((state (list keys vals
                         (seq-uniq keys)
                         (seq-sort-by #'length #'< keys)
                         (buffer-string)
                         (marker-position m)
                         (overlay-start ov) (overlay-end ov)
                         (text-properties-at 1))))
        (undo)
        (widen()
        (list state (buffer-string) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              (text-properties-at 1)))))))
"##,
        expect,
    )
}
