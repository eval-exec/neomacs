//! Complex combo batch 293 — `cl-lib` sequence operations deep:
//! `cl-position`/`cl-find`/`cl-count`/`cl-search`/`cl-mismatch` with
//! `:start`/`:end`/`:from-end`/`:key`/`:test`.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx293_cl_position_with_start_end_from_end() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-position)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((nums '(1 2 3 4 3 2 1)))
  (list (cl-position 3 nums)
        (cl-position 3 nums :from-end t)
        (cl-position 3 nums :start 4)
        (cl-position 3 nums :start 4 :from-end t)
        (cl-position 99 nums)
        (cl-position 1 nums :start 1)))
"##,
        expect,
    )
}

#[test]
fn div_cx293_cl_find_with_key_and_test() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-find)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((alist '((1 . "a") (2 . "b") (3 . "c"))))
  (list (cl-find 2 alist :key #'car)
        (cl-find 4 alist :key #'car)
        (cl-find "b" alist :key #'cdr :test #'string=)
        (cl-find 3 alist :key #'car :test #'=)))
"##,
        expect,
    )
}

#[test]
fn div_cx293_cl_count_with_predicate() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-count)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((nums '(1 2 3 4 5 6 7 8 9 10)))
  (list (cl-count 3 nums)
        (cl-count-if #'evenp nums)
        (cl-count-if-not #'evenp nums)
        (cl-count-if #'evenp nums :start 5)))
"##,
        expect,
    )
}

#[test]
fn div_cx293_cl_search_in_sequence() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-search)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (cl-search '(2 3) '(1 2 3 4 5))
      (cl-search '(4 5) '(1 2 3 4 5))
      (cl-search '(1 2) '(1 2 3 4 5) :from-end t)
      (cl-search '(9 9) '(1 2 3 4 5))
      (cl-search "abc" "xxabcyy"))
"##,
        expect,
    )
}

#[test]
fn div_cx293_cl_mismatch_between_sequences() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-mismatch)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (cl-mismatch '(1 2 3) '(1 2 3))
      (cl-mismatch '(1 2 3) '(1 2 4))
      (cl-mismatch '(1 2 3) '(1 2 3 4))
      (cl-mismatch '(1 2 3 4) '(1 2 3))
      (cl-mismatch "abc" "axc"))
"##,
        expect,
    )
}

#[test]
fn div_cx293_cl_substitute_with_count() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-substitute)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (cl-substitute 9 1 '(1 2 1 3 1 4))
      (cl-substitute 9 1 '(1 2 1 3 1 4) :count 2)
      (cl-substitute 9 1 '(1 2 1 3 1 4) :count 2 :from-end t)
      (cl-substitute-if 9 #'evenp '(1 2 3 4 5))
      (cl-substitute-if 9 #'evenp '(1 2 3 4 5) :count 1))
"##,
        expect,
    )
}

#[test]
fn div_cx293_cl_remove_duplicates_with_test() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-remove-duplicates)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (cl-remove-duplicates '(1 2 2 3 3 3 4) :test #'=)
      (cl-remove-duplicates '(1 2 2 3 3 3 4) :from-end t)
      (cl-remove-duplicates '("A" "a" "B") :test #'string-equal)
      (cl-remove-duplicates '("A" "a" "B") :test #'eq))
"##,
        expect,
    )
}

#[test]
fn div_cx293_cl_merge_stable_sort_variants() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-merge)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (cl-merge 'list '(1 3 5) '(2 4 6) #'<)
      (cl-merge 'list '(1 3 5) '(2 4 6) #'>)
      (cl-sort (copy-sequence '(3 1 4 1 5 9 2 6)) #'<)
      (cl-stable-sort (copy-sequence '((3 . "c") (1 . "a") (1 . "b"))) #'< :key #'car))
"##,
        expect,
    )
}

#[test]
fn div_cx293_cl_set_operations_with_key() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-union)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((a '((1 . "a") (2 . "b") (3 . "c")))
      (b '((2 . "x") (4 . "y"))))
  (list (sort (cl-union a b :key #'car) (lambda (x y) (< (car x) (car y))))
        (sort (cl-intersection a b :key #'car) (lambda (x y) (< (car x) (car y))))
        (sort (cl-set-difference a b :key #'car) (lambda (x y) (< (car x) (car y))))
        (sort (cl-set-exclusive-or a b :key #'car) (lambda (x y) (< (car x) (car y))))))
"##,
        expect,
    )
}

#[test]
fn div_cx293_cl_seq_ops_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-sort)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((data '((3 . "c") (1 . "a") (4 . "d") (1 . "e") (5 . "b"))))
  (with-temp-buffer
    (buffer-enable-undo)
    (insert (format "Seq ops mega: %S" data))
    (put-text-property 1 6 'face 'bold)
    (let ((m (set-marker (make-marker) 10))
          (ov (make-overlay 4 18)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 25)
      (let ((state (list (cl-sort (copy-sequence data) #'< :key #'car)
                         (cl-remove-duplicates (copy-sequence data) :key #'car :test #'= :from-end t)
                         (cl-count 1 data :key #'car)
                         (cl-find 4 data :key #'car)
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
